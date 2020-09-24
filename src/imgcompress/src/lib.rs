use std::convert::TryInto;
#[cfg(test)]
use std::io::{Read, Seek, SeekFrom};
#[cfg(test)]
use std::process::Command;

use anyhow::Result;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use tempfile::NamedTempFile;
use zstd::stream::decode_all;

mod btrfs;
mod chunk_tree;
mod structs;
mod tree;

use btrfs::Btrfs;

#[derive(Deserialize, Serialize, Default)]
pub struct CompressedBtrfsImage {
    /// Compressed original image. Fuzzed metadata should be laid on top of the original image.
    pub base: Vec<u8>,
    /// Vector of (offset, size) tuples>
    ///
    /// For example, if `metadata` contained [(0, 10), (50, 5)], then `data.len()` == 15, where the
    /// first 10 bytes would go to offset 0 and the last 5 bytes would go to offset 50.
    pub metadata: Vec<(u64, u64)>,
    pub data: Vec<u8>,
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
    let mut image = decode_all(compressed.base.as_slice())?;

    // Now overwrite `image` with the metadata placed at their original offsets
    let mut data_idx = 0;
    for (offset, size) in &compressed.metadata {
        let offset: usize = (*offset).try_into()?;
        let size: usize = (*size).try_into()?;

        let _: Vec<_> = image
            .splice(
                offset..(offset + size),
                compressed.data[data_idx..(data_idx + size)].iter().cloned(),
            )
            .collect();
        data_idx += size;
    }

    // XXX implement checksum and magic rewrites

    Ok(image)
}

/// Test that compressing and decompressing an image results in bit-for-bit equality
#[test]
fn test_compress_decompress() {
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
    let rc = Command::new("mkfs.btrfs")
        .arg(orig.path())
        .status()
        .expect("Failed to run mkfs.btrfs");
    assert!(rc.success());

    let mut orig_buffer = Vec::new();
    orig.as_file()
        .read_to_end(&mut orig_buffer)
        .expect("Failed to read original image");

    let compressed = compress(&orig_buffer).expect("Failed to compress image");
    let decompressed = decompress(&compressed).expect("Failed to decompress image");

    assert_eq!(orig_buffer, decompressed);
}
