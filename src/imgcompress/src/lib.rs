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

pub fn compress(img: &[u8]) -> Result<CompressedBtrfsImage> {
    let btrfs = Btrfs::new(img)?;
    btrfs.compress()
}

pub fn decompress(_img: &CompressedBtrfsImage) -> Result<Vec<u8>> {
    unimplemented!();
}

#[test]
fn test_mount() {
    // TODO: test compress + decompress + mount
}
