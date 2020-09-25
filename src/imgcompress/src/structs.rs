pub const BTRFS_CSUM_SIZE: usize = 32;
const BTRFS_LABEL_SIZE: usize = 256;
const BTRFS_FSID_SIZE: usize = 16;
const BTRFS_UUID_SIZE: usize = 16;
const BTRFS_SYSTEM_CHUNK_ARRAY_SIZE: usize = 2048;

pub const BTRFS_SUPERBLOCK_OFFSET: usize = 0x10_000;
pub const BTRFS_SUPERBLOCK_OFFSET2: usize = 0x4_000_000;
pub const BTRFS_SUPERBLOCK_OFFSET3: usize = 0x4_000_000_000;
pub const BTRFS_SUPERBLOCK_MAGIC: [u8; 8] = *b"_BHRfS_M";
pub const BTRFS_SUPERBLOCK_SIZE: usize = 4096;
pub const BTRFS_CSUM_TYPE_CRC32: u16 = 0;
/// All the docs and code suggest it's `u32::MAX` but after many hours of debugging it turns out
/// only 0 works. Something is definitely fishy here. At least we have tests that test checksum
/// integrity.
pub const BTRFS_CSUM_CRC32_SEED: u32 = 0;

pub const BTRFS_CHUNK_ITEM_KEY: u8 = 228;
pub const BTRFS_ROOT_ITEM_KEY: u8 = 132;

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsDevItem {
    /// the internal btrfs device id
    pub devid: u64,
    /// size of the device
    pub total_bytes: u64,
    /// bytes used
    pub bytes_used: u64,
    /// optimal io alignment for this device
    pub io_align: u32,
    /// optimal io width for this device
    pub io_width: u32,
    /// minimal io size for this device
    pub sector_size: u32,
    /// type and info about this device
    pub ty: u64,
    /// expected generation for this device
    pub generation: u64,
    /// starting byte of this partition on the device, to allow for stripe alignment in the future
    pub start_offset: u64,
    /// grouping information for allocation decisions
    pub dev_group: u32,
    /// seek speed 0-100 where 100 is fastest
    pub seek_speed: u8,
    /// bandwidth 0-100 where 100 is fastest
    pub bandwidth: u8,
    /// btrfs generated uuid for this device
    pub uuid: [u8; BTRFS_UUID_SIZE],
    /// uuid of FS who owns this device
    pub fsid: [u8; BTRFS_UUID_SIZE],
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsRootBackup {
    pub tree_root: u64,
    pub tree_root_gen: u64,
    pub chunk_root: u64,
    pub chunk_root_gen: u64,
    pub extent_root: u64,
    pub extent_root_gen: u64,
    pub fs_root: u64,
    pub fs_root_gen: u64,
    pub dev_root: u64,
    pub dev_root_gen: u64,
    pub csum_root: u64,
    pub csum_root_gen: u64,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub num_devices: u64,
    /// future
    pub unused_64: [u64; 4],
    pub tree_root_level: u8,
    pub chunk_root_level: u8,
    pub extent_root_level: u8,
    pub fs_root_level: u8,
    pub dev_root_level: u8,
    pub csum_root_level: u8,
    /// future and to align
    pub unused_8: [u8; 10],
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsSuperblock {
    pub csum: [u8; BTRFS_CSUM_SIZE],
    pub fsid: [u8; BTRFS_FSID_SIZE],
    /// Physical address of this block
    pub bytenr: u64,
    pub flags: u64,
    pub magic: [u8; 0x8],
    pub generation: u64,
    /// Logical address of the root tree root
    pub root: u64,
    /// Logical address of the chunk tree root
    pub chunk_root: u64,
    /// Logical address of the log tree root
    pub log_root: u64,
    pub log_root_transid: u64,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub root_dir_objectid: u64,
    pub num_devices: u64,
    pub sector_size: u32,
    pub node_size: u32,
    /// Unused and must be equal to `nodesize`
    pub leafsize: u32,
    pub stripesize: u32,
    pub sys_chunk_array_size: u32,
    pub chunk_root_generation: u64,
    pub compat_flags: u64,
    pub compat_ro_flags: u64,
    pub incompat_flags: u64,
    pub csum_type: u16,
    pub root_level: u8,
    pub chunk_root_level: u8,
    pub log_root_level: u8,
    pub dev_item: BtrfsDevItem,
    pub label: [u8; BTRFS_LABEL_SIZE],
    pub cache_generation: u64,
    pub uuid_tree_generation: u64,
    pub metadata_uuid: [u8; BTRFS_FSID_SIZE],
    /// Future expansion
    pub _reserved: [u64; 28],
    pub sys_chunk_array: [u8; BTRFS_SYSTEM_CHUNK_ARRAY_SIZE],
    pub root_backups: [BtrfsRootBackup; 4],
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsStripe {
    pub devid: u64,
    pub offset: u64,
    pub dev_uuid: [u8; BTRFS_UUID_SIZE],
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsChunk {
    /// size of this chunk in bytes
    pub length: u64,
    /// objectid of the root referencing this chunk
    pub owner: u64,
    pub stripe_len: u64,
    pub ty: u64,
    /// optimal io alignment for this chunk
    pub io_align: u32,
    /// optimal io width for this chunk
    pub io_width: u32,
    /// minimal io size for this chunk
    pub sector_size: u32,
    /// 2^16 stripes is quite a lot, a second limit is the size of a single item in the btree
    pub num_stripes: u16,
    /// sub stripes only matter for raid10
    pub sub_stripes: u16,
    pub stripe: BtrfsStripe,
    // additional stripes go here
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsTimespec {
    pub sec: u64,
    pub nsec: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsInodeItem {
    /// nfs style generation number
    pub generation: u64,
    /// transid that last touched this inode
    pub transid: u64,
    pub size: u64,
    pub nbytes: u64,
    pub block_group: u64,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub rdev: u64,
    pub flags: u64,
    /// modification sequence number for NFS
    pub sequence: u64,
    pub reserved: [u64; 4],
    pub atime: BtrfsTimespec,
    pub ctime: BtrfsTimespec,
    pub mtime: BtrfsTimespec,
    pub otime: BtrfsTimespec,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsRootItem {
    pub inode: BtrfsInodeItem,
    pub generation: u64,
    pub root_dirid: u64,
    pub bytenr: u64,
    pub byte_limit: u64,
    pub bytes_used: u64,
    pub last_snapshot: u64,
    pub flags: u64,
    pub refs: u32,
    pub drop_progress: BtrfsKey,
    pub drop_level: u8,
    pub level: u8,
    pub generation_v2: u64,
    pub uuid: [u8; BTRFS_UUID_SIZE],
    pub parent_uuid: [u8; BTRFS_UUID_SIZE],
    pub received_uuid: [u8; BTRFS_UUID_SIZE],
    /// updated when an inode changes
    pub ctransid: u64,
    /// trans when created
    pub otransid: u64,
    /// trans when sent. non-zero for received subvol
    pub stransid: u64,
    /// trans when received. non-zero for received subvol
    pub rtransid: u64,
    pub ctime: BtrfsTimespec,
    pub otime: BtrfsTimespec,
    pub stime: BtrfsTimespec,
    pub rtime: BtrfsTimespec,
    pub reserved: [u64; 8],
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsDirItem {
    pub location: BtrfsKey,
    pub transid: u64,
    pub data_len: u16,
    pub name_len: u16,
    pub ty: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsInodeRef {
    pub index: u64,
    pub name_len: u16,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsKey {
    pub objectid: u64,
    pub ty: u8,
    pub offset: u64,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsHeader {
    pub csum: [u8; BTRFS_CSUM_SIZE],
    pub fsid: [u8; BTRFS_FSID_SIZE],
    /// Which block this node is supposed to live in
    pub bytenr: u64,
    pub flags: u64,
    pub chunk_tree_uuid: [u8; BTRFS_UUID_SIZE],
    pub generation: u64,
    pub owner: u64,
    pub nritems: u32,
    pub level: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
/// A `BtrfsLeaf` is full of `BtrfsItem`s. `offset` and `size` (relative to start of data area)
/// tell us where to find the item in the leaf.
pub struct BtrfsItem {
    pub key: BtrfsKey,
    pub offset: u32,
    pub size: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsLeaf {
    pub header: BtrfsHeader,
    // `BtrfsItem`s begin here
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
/// All non-leaf blocks are nodes and they hold only keys are pointers to other blocks
pub struct BtrfsKeyPtr {
    pub key: BtrfsKey,
    pub blockptr: u64,
    pub generation: u64,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct BtrfsNode {
    pub header: BtrfsHeader,
    // `BtrfsKeyPtr`s begin here
}
