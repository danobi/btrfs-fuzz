use anyhow::{bail, Result};

use crate::structs::*;

/// Parse BtrfsHeader from a tree node (internal or leaf)
pub fn parse_btrfs_header<'a>(buf: &'a [u8]) -> Result<&'a BtrfsHeader> {
    let header_size = std::mem::size_of::<BtrfsHeader>();
    if buf.len() < header_size {
        bail!("Failed to parse BtrfsHeader b/c buf too small");
    }

    Ok(unsafe { &*(buf.as_ptr() as *const BtrfsHeader) })
}

/// Parse an internal tree node
///
/// Precondition is that `buf` is not a leaf node.
pub fn parse_btrfs_node<'a>(buf: &'a [u8]) -> Result<Vec<&'a BtrfsKeyPtr>> {
    let header = parse_btrfs_header(buf)?;
    let mut offset = std::mem::size_of::<BtrfsHeader>();
    let mut key_ptrs = Vec::new();
    for _ in 0..header.nritems {
        key_ptrs.push(unsafe { &*(buf.as_ptr().add(offset) as *const BtrfsKeyPtr) });
        offset += std::mem::size_of::<BtrfsKeyPtr>();
    }

    Ok(key_ptrs)
}

/// Parse leaf tree node
pub fn parse_btrfs_leaf<'a>(buf: &'a [u8]) -> Result<Vec<&'a BtrfsItem>> {
    let header = parse_btrfs_header(buf)?;
    let mut offset = std::mem::size_of::<BtrfsHeader>();
    let mut items = Vec::new();
    for _ in 0..header.nritems {
        items.push(unsafe { &*(buf.as_ptr().add(offset) as *const BtrfsItem) });
        offset += std::mem::size_of::<BtrfsItem>();
    }

    Ok(items)
}
