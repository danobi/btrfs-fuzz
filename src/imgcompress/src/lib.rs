use std::convert::TryInto;
#[cfg(test)]
use std::io::{Read, Seek, SeekFrom};
#[cfg(test)]
use std::process::Command;

use anyhow::{bail, Result};
use crc32c::crc32c_append;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use tempfile::NamedTempFile;
use zstd::stream::decode_all;

mod btrfs;
mod chunk_tree;
mod structs;
mod tree;

use btrfs::Btrfs;
use structs::*;

/// Metadata for a metadata extent.
#[derive(Deserialize, Serialize, Default)]
pub struct MetadataExtent {
    /// If true, this metadata extent begins with a csum field that needs fixups
    pub needs_csum_fixup: bool,
    /// Offset in the decompressed image this extent needs to be written
    pub offset: u64,
    /// Length of metadata extent
    pub size: u64,
}

#[derive(Deserialize, Serialize, Default)]
pub struct CompressedBtrfsImage {
    /// Compressed original image. Fuzzed metadata should be laid on top of the original image.
    base: Vec<u8>,
    /// Each entry in this vector describes a metadata extent in `data`.
    ///
    /// For example, if `metadata` contained entries [(offset 0, size 10), (offset 50, size 5)],
    /// then `data.len()` == 15, where the first 10 bytes would go to offset 0 and the last 5 bytes
    /// would go to offset 50.
    pub metadata: Vec<MetadataExtent>,
    pub data: Vec<u8>,
    /// Size of each node in the btree. Used to calculate checksum in node headers.
    node_size: usize,
}

impl CompressedBtrfsImage {
    /// Mark a range of data as metadata
    pub(crate) fn mark_as_metadata(
        &mut self,
        physical: u64,
        metadata: &[u8],
        needs_csum_fixup: bool,
    ) -> Result<()> {
        self.metadata.push(MetadataExtent {
            needs_csum_fixup,
            offset: physical,
            size: metadata.len().try_into()?,
        });
        self.data.extend_from_slice(metadata);

        Ok(())
    }
}

/// Compress a btrfs image
pub fn compress(img: &[u8]) -> Result<CompressedBtrfsImage> {
    let btrfs = Btrfs::new(img)?;
    btrfs.compress()
}

/// Decompressed an `imgcompress::compress`d btrfs image.
///
/// Also rewrites superblock magic and checksums to be valid.
pub fn decompress(compressed: &CompressedBtrfsImage) -> Result<Vec<u8>> {
    // Decompress the base image
    let mut image: Vec<u8> = decode_all(compressed.base.as_slice())?;

    // Now overwrite `image` with the metadata placed at their original offsets
    let mut data_idx = 0;
    for metadata in &compressed.metadata {
        let offset: usize = metadata.offset.try_into()?;
        let size: usize = metadata.size.try_into()?;

        let _: Vec<_> = image
            .splice(
                offset..(offset + size),
                compressed.data[data_idx..(data_idx + size)].iter().cloned(),
            )
            .collect();
        data_idx += size;
    }

    // Fixup the fist superblock
    if image.len() < (BTRFS_SUPERBLOCK_OFFSET + BTRFS_SUPERBLOCK_SIZE) {
        bail!("Decompressed image too short to contain superblock");
    } else {
        let superblock_ptr = image[BTRFS_SUPERBLOCK_OFFSET..].as_mut_ptr() as *mut BtrfsSuperblock;
        let superblock = unsafe { &mut *superblock_ptr };

        // We only support CRC32 for now
        if superblock.csum_type != BTRFS_CSUM_TYPE_CRC32 {
            let ty: u16 = superblock.csum_type;
            println!("Warning: wrong csum type in superblock, type={}", ty);
        }

        if superblock.magic != BTRFS_SUPERBLOCK_MAGIC {
            superblock.magic = BTRFS_SUPERBLOCK_MAGIC;
        }
    }

    // Recalculate checksum for each block
    for metadata in &compressed.metadata {
        if !metadata.needs_csum_fixup {
            continue;
        }

        let offset: usize = metadata.offset.try_into()?;

        let block_size = if offset == BTRFS_SUPERBLOCK_OFFSET
            || offset == BTRFS_SUPERBLOCK_OFFSET2
            || offset == BTRFS_SUPERBLOCK_OFFSET3
        {
            BTRFS_SUPERBLOCK_SIZE
        } else {
            compressed.node_size
        };
        assert_ne!(block_size, 0);

        // Calculate checksum for block
        let begin = offset + BTRFS_CSUM_SIZE;
        let end = offset + block_size;
        let checksum: u32 = crc32c_append(BTRFS_CSUM_CRC32_SEED, &image[begin..end]);

        // Write checksum back into block
        //
        // NB: a crc32c checksum is only 4 bytes long. We'll leave the other 28 bytes alone.
        let _: Vec<_> = image
            .splice(offset..(offset + 4), checksum.to_le_bytes().iter().cloned())
            .collect();
    }

    Ok(image)
}

#[cfg(test)]
fn generate_test_image() -> Vec<u8> {
    let mut orig = NamedTempFile::new().expect("Failed to create tempfile");
    // mkfs.btrfs needs at least 120 MB to create an image
    orig.as_file()
        .set_len(120 << 20)
        .expect("Failed to increase orig image size");
    // Seek to beginning just in case
    orig.as_file_mut()
        .seek(SeekFrom::Start(0))
        .expect("Failed to seek to beginning of orig image");

    // mkfs.brtrfs
    let output = Command::new("mkfs.btrfs")
        .arg(orig.path())
        .output()
        .expect("Failed to run mkfs.btrfs");
    assert!(output.status.success());

    let mut orig_buffer = Vec::new();
    orig.as_file()
        .read_to_end(&mut orig_buffer)
        .expect("Failed to read original image");

    orig_buffer
}

/// Test that compressing and decompressing an image results in bit-for-bit equality
#[test]
fn test_compress_decompress() {
    let orig_buffer = generate_test_image();
    let compressed = compress(&orig_buffer).expect("Failed to compress image");
    let decompressed = decompress(&compressed).expect("Failed to decompress image");

    assert!(orig_buffer == decompressed);
}

/// Test that checksums are correctly fixed up if they get corrupted
#[test]
fn test_checksum_fixup() {
    let orig_buffer = generate_test_image();

    // This is pretty pricey -- 120M copy. Hopefully it doesn't cause any issues
    let mut corrupted_buffer = orig_buffer.clone();
    let random: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
    corrupted_buffer.splice(
        BTRFS_SUPERBLOCK_OFFSET..(BTRFS_SUPERBLOCK_OFFSET + 4),
        random.iter().cloned(),
    );

    // Now compress and decompress corrupted buffer
    let compressed = compress(&corrupted_buffer).expect("Failed to compress corrupted image");
    let decompressed = decompress(&compressed).expect("Failed to decompress corrupted image");

    // Corrupted checksum should be fixed up
    assert!(orig_buffer == decompressed);
}

#[test]
fn test_superblock_magic_fixup() {
    let orig_buffer = generate_test_image();

    let mut compressed = compress(&orig_buffer).expect("Failed to compress corrupted image");

    // Corrupt the magic in the superblock
    let mut data_idx: usize = 0;
    let mut corrupted_super = false;
    for metadata in &compressed.metadata {
        let offset: usize = metadata.offset.try_into().unwrap();
        let size: usize = metadata.size.try_into().unwrap();

        if offset == BTRFS_SUPERBLOCK_OFFSET {
            let superblock =
                unsafe { &mut *(compressed.data[data_idx..].as_mut_ptr() as *mut BtrfsSuperblock) };
            // Magic corruption
            superblock.magic[3] = b'Z';
            corrupted_super = true;
        }

        data_idx += size;
    }
    assert!(corrupted_super);

    let decompressed = decompress(&compressed).expect("Failed to decompress corrupted image");

    // Corrupted checksum should be fixed up
    assert!(orig_buffer == decompressed);
}

/// Test that checksums are recalculated on metadata changes. Note that this is pretty difficult to
/// test accurately so we opt to just check that the checksum was changed.
#[test]
fn test_checksum_fixup_on_metadata_corruption() {
    let orig_buffer = generate_test_image();

    let mut compressed = compress(&orig_buffer).expect("Failed to compress corrupted image");
    let ones: Vec<u8> = vec![1; 45];

    let mut first = true;
    let mut data_idx: usize = 0;
    let mut csum_before: Option<Vec<u8>> = None;
    let mut scribbed_offset: usize = 0;
    for metadata in &compressed.metadata {
        let size: usize = metadata.size.try_into().unwrap();

        if !metadata.needs_csum_fixup {
            data_idx += size;
            continue;
        }

        // Skip superblock to avoid overtesting the superblock
        if first {
            first = false;
            data_idx += size;
            continue;
        }

        scribbed_offset = metadata.offset.try_into().unwrap();

        // Store checksum before
        csum_before = Some(compressed.data[data_idx..(data_idx + BTRFS_CSUM_SIZE)].to_owned());

        // Scribble over metadata a little
        let begin = data_idx + BTRFS_CSUM_SIZE;
        let end = data_idx + BTRFS_CSUM_SIZE + ones.len();
        let _: Vec<_> = compressed
            .data
            .splice(begin..end, ones.iter().cloned())
            .collect();

        data_idx += size;
    }

    let decompressed = decompress(&compressed).expect("Failed to decompress corrupted image");
    let csum_after = &decompressed[scribbed_offset..(scribbed_offset + BTRFS_CSUM_SIZE)];

    // First test that the ones we wrote are where we expect so we know we didn't mess up the
    // offset calculations somewhere
    let begin = scribbed_offset + BTRFS_CSUM_SIZE;
    let end = scribbed_offset + BTRFS_CSUM_SIZE + ones.len();
    assert!(&decompressed[begin..end] == ones.as_slice());

    // Test that checksum changed
    assert!(csum_before.unwrap() != csum_after);
}
