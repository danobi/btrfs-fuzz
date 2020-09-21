/// Shared memory region size created by afl-fuzz
pub const AFL_MAP_SIZE: u32 = 1 << 16;

// AFL++ Forkserver options
pub const AFL_FS_OPT_ENABLED: u32 = 0x80000001;
pub const AFL_FS_OPT_MAPSIZE: u32 = 0x40000000;

/// Hardcoded file descriptors to communicate with AFL++
pub const AFL_FORKSERVER_READ_FD: i32 = 198;
pub const AFL_FORKSERVER_WRITE_FD: i32 = AFL_FORKSERVER_READ_FD + 1;
