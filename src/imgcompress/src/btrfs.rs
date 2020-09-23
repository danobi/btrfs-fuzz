use std::convert::TryInto;
use std::mem::size_of;

use anyhow::{anyhow, bail, Context, Result};

use crate::chunk_tree::{ChunkTreeCache, ChunkTreeKey, ChunkTreeValue};
use crate::structs::*;
use crate::tree;
use crate::CompressedBtrfsImage;

pub struct Btrfs<'a> {
    image: &'a [u8],
    superblock: &'a BtrfsSuperblock,
    chunk_tree_cache: ChunkTreeCache,
}

impl<'a> Btrfs<'a> {
    /// Constructor processes superblock and chunk tree. The chunk tree is used to map logical to
    /// physical addresses so it needs to be bootstrapped ASAP.
    pub fn new(img: &'a [u8]) -> Result<Self> {
        // Read superblock
        let superblock =
            parse_superblock(img).with_context(|| "Failed to parse superblock".to_string())?;

        // Bootstraup chunk tree
        let mut chunk_tree_cache = bootstrap_chunk_tree(superblock)
            .with_context(|| "Failed to boostrap chunk tree".to_string())?;

        // Read root chunk tree node
        let chunk_root = read_root_node(img, superblock.chunk_root, &chunk_tree_cache)
            .with_context(|| "Failed to read chunk tree root".to_string())?;

        // Read rest of chunk tree
        read_chunk_tree(img, &chunk_root, &mut chunk_tree_cache, &superblock)
            .with_context(|| "Failed to read chunk tree".to_string())?;

        Ok(Self {
            image: img,
            superblock,
            chunk_tree_cache,
        })
    }

    /// Compress the image
    pub fn compress(&self) -> Result<CompressedBtrfsImage> {
        let mut compressed = CompressedBtrfsImage::default();

        // Save superblock
        compressed.metadata.push((
            BTRFS_SUPERBLOCK_OFFSET.try_into()?,
            size_of::<BtrfsSuperblock>().try_into()?,
        ));
        compressed.data.extend_from_slice(
            &self.image
                [BTRFS_SUPERBLOCK_OFFSET..(BTRFS_SUPERBLOCK_OFFSET + size_of::<BtrfsSuperblock>())],
        );

        // Parse everything in the root tree
        self.parse_root_tree(&mut compressed)
            .with_context(|| "Failed to parse root tree".to_string())?;

        // The log tree seems to be maintained separately from the root tree, so parse everything
        // in there separately
        self.parse_tree(self.superblock.log_root, &mut compressed)?;

        Ok(compressed)
    }

    fn parse_root_tree(&self, compressed: &mut CompressedBtrfsImage) -> Result<()> {
        let physical = self
            .chunk_tree_cache
            .offset(self.superblock.root)
            .ok_or_else(|| anyhow!("Root tree root logical addr not mapped"))?;
        let node = read_root_node(self.image, self.superblock.root, &self.chunk_tree_cache)
            .with_context(|| "Failed to read root tree root".to_string())?;

        let header = tree::parse_btrfs_header(node)?;

        if header.level == 0 {
            // Store the header b/c it's metadata
            let metadata_size =
                size_of::<BtrfsHeader>() + (header.nritems as usize * size_of::<BtrfsItem>());
            compressed
                .metadata
                .push((physical, metadata_size.try_into()?));
            compressed.data.extend_from_slice(&node[..metadata_size]);

            // Now recursively walk the tree
            let items = tree::parse_btrfs_leaf(node)?;
            for item in items.iter().rev() {
                if item.key.ty != BTRFS_ROOT_ITEM_KEY {
                    continue;
                }

                let root_item = unsafe {
                    &*(node
                        .as_ptr()
                        .add(std::mem::size_of::<BtrfsHeader>() + item.offset as usize)
                        as *const BtrfsRootItem)
                };

                self.parse_tree(root_item.bytenr, compressed)?;
            }
        } else {
            bail!("The root tree root should only contain one level")
        }

        Ok(())
    }

    fn parse_tree(&self, logical: u64, compressed: &mut CompressedBtrfsImage) -> Result<()> {
        let physical = self
            .chunk_tree_cache
            .offset(logical)
            .ok_or_else(|| anyhow!("Node logical addr not mapped"))?;
        let node = read_root_node(self.image, logical, &self.chunk_tree_cache)
            .with_context(|| "Failed to read node".to_string())?;

        // Store the header b/c it's metadata
        let header = tree::parse_btrfs_header(node)?;
        let mut metadata_size = size_of::<BtrfsHeader>();

        if header.level == 0 {
            // We're at a leaf: still store the metadata but don't store any payloads
            metadata_size += header.nritems as usize * size_of::<BtrfsItem>();
            compressed
                .metadata
                .push((physical, metadata_size.try_into()?));
            compressed.data.extend_from_slice(&node[..metadata_size]);
        } else {
            // We're at an internal node: there's no payload
            metadata_size += header.nritems as usize * size_of::<BtrfsKeyPtr>();
            compressed
                .metadata
                .push((physical, metadata_size.try_into()?));
            compressed.data.extend_from_slice(&node[..metadata_size]);

            // Recursively visit children
            let ptrs = tree::parse_btrfs_node(node)?;
            for ptr in ptrs {
                self.parse_tree(ptr.blockptr, compressed)?;
            }
        }

        Ok(())
    }
}

fn parse_superblock(img: &[u8]) -> Result<&BtrfsSuperblock> {
    if BTRFS_SUPERBLOCK_OFFSET + size_of::<BtrfsSuperblock>() > img.len() {
        bail!("Image to small to contain superblock");
    }

    let superblock_ptr = img[BTRFS_SUPERBLOCK_OFFSET..].as_ptr() as *const BtrfsSuperblock;
    let superblock = unsafe { &*superblock_ptr };

    if superblock.magic != BTRFS_SUPERBLOCK_MAGIC {
        bail!("Superblock magic is wrong");
    }

    Ok(unsafe { &*superblock_ptr })
}

fn bootstrap_chunk_tree(superblock: &BtrfsSuperblock) -> Result<ChunkTreeCache> {
    let array_size = superblock.sys_chunk_array_size as usize;
    let mut offset: usize = 0;
    let mut chunk_tree_cache = ChunkTreeCache::default();

    while offset < array_size {
        let key_size = std::mem::size_of::<BtrfsKey>();
        if offset + key_size > array_size as usize {
            bail!("Short key read");
        }

        let key_slice = &superblock.sys_chunk_array[offset..];
        let key = unsafe { &*(key_slice.as_ptr() as *const BtrfsKey) };
        if key.ty != BTRFS_CHUNK_ITEM_KEY {
            bail!(
                "Unknown item type={} in sys_array at offset={}",
                key.ty,
                offset
            );
        }
        offset += key_size;

        if offset + std::mem::size_of::<BtrfsChunk>() > array_size {
            bail!("short chunk item read");
        }

        let chunk_slice = &superblock.sys_chunk_array[offset..];
        let chunk = unsafe { &*(chunk_slice.as_ptr() as *const BtrfsChunk) };
        if chunk.num_stripes == 0 {
            bail!("num_stripes cannot be 0");
        }

        // To keep things simple, we'll only process 1 stripe, as stripes should have
        // identical content. The device the stripe is on will be the device passed in
        // via cmd line args.
        let num_stripes = chunk.num_stripes; // copy to prevent unaligned access
        if num_stripes != 1 {
            println!(
                "Warning: {} stripes detected but only processing 1",
                num_stripes
            );
        }

        // Add chunk to cache if not already in cache
        let logical = key.offset;
        if chunk_tree_cache.offset(logical).is_none() {
            chunk_tree_cache.insert(
                ChunkTreeKey {
                    start: logical,
                    size: chunk.length,
                },
                ChunkTreeValue {
                    offset: chunk.stripe.offset,
                },
            );
        }

        // Despite only processing one stripe, we need to be careful to skip over the
        // entire chunk item.
        let chunk_item_size = std::mem::size_of::<BtrfsChunk>()
            + (std::mem::size_of::<BtrfsStripe>() * (chunk.num_stripes as usize - 1));
        if offset + chunk_item_size > array_size {
            bail!("short chunk item + stripe read");
        }
        offset += chunk_item_size;
    }

    Ok(chunk_tree_cache)
}

fn read_root_node<'a>(img: &'a [u8], logical: u64, cache: &ChunkTreeCache) -> Result<&'a [u8]> {
    let size: usize = cache
        .mapping_kv(logical)
        .ok_or_else(|| anyhow!("Root node logical addr not mapped"))?
        .0
        .size
        .try_into()?;
    let physical: usize = cache
        .offset(logical)
        .ok_or_else(|| anyhow!("Root node logical addr not mapped"))?
        .try_into()?;
    let end = physical + size;

    Ok(&img[physical..end])
}

fn read_chunk_tree(
    img: &[u8],
    root: &[u8],
    chunk_tree_cache: &mut ChunkTreeCache,
    superblock: &BtrfsSuperblock,
) -> Result<()> {
    let header = tree::parse_btrfs_header(root)?;

    // Level 0 is leaf node, !0 is internal node
    if header.level == 0 {
        let items = tree::parse_btrfs_leaf(root)?;
        for item in items {
            if item.key.ty != BTRFS_CHUNK_ITEM_KEY {
                continue;
            }

            let chunk = unsafe {
                // `item.offset` is offset from data portion of `BtrfsLeaf` where associated
                // `BtrfsChunk` starts
                &*(root
                    .as_ptr()
                    .add(std::mem::size_of::<BtrfsHeader>() + item.offset as usize)
                    as *const BtrfsChunk)
            };

            chunk_tree_cache.insert(
                ChunkTreeKey {
                    start: item.key.offset,
                    size: chunk.length,
                },
                ChunkTreeValue {
                    offset: chunk.stripe.offset,
                },
            );
        }
    } else {
        let ptrs = tree::parse_btrfs_node(root)?;
        for ptr in ptrs {
            let physical: usize = chunk_tree_cache
                .offset(ptr.blockptr)
                .ok_or_else(|| anyhow!("Chunk tree node not mapped"))?
                .try_into()?;
            let end: usize = physical + superblock.node_size as usize;

            read_chunk_tree(img, &img[physical..end], chunk_tree_cache, superblock)?;
        }
    }

    Ok(())
}
