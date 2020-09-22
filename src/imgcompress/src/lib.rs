use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct CompressedBtrfsImage {
    pub metadata: Vec<u8>,
    pub data: Vec<u8>,
}

pub fn compress(_img: &[u8]) -> Result<CompressedBtrfsImage> {
    unimplemented!();
}

pub fn decompress(_img: &CompressedBtrfsImage) -> Result<Vec<u8>> {
    unimplemented!();
}

#[test]
fn test_mount() {
    // TODO: test compress + decompress + mount
}
