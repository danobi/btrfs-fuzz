use std::convert::TryInto;

use anyhow::Result;
use serde::{Deserialize, Serialize};

mod btrfs;
mod chunk_tree;
mod structs;
mod tree;

use btrfs::Btrfs;

#[derive(Deserialize, Serialize, Default)]
pub struct CompressedBtrfsImage {
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
    // First figure out how big the image is gonna be
    let mut max = (0, 0);
    for (offset, size) in &compressed.metadata {
        if *offset > max.0 {
            max = (*offset, *size);
        }
    }

    let mut image: Vec<u8> = vec![0; (max.0 + max.1).try_into()?];

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

#[test]
fn test_mount() {
    // TODO: test compress + decompress + mount
}
