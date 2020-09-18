/// Shared memory region size created by afl-fuzz
pub const AFL_MAP_SIZE: usize = 1 << 16;

/// Hardcoded file descriptors to communicate with AFL++
pub const AFL_FORKSERVER_READ_FD: i32 = 198;
pub const AFL_FORKSERVER_WRITE_FD: i32 = AFL_FORKSERVER_READ_FD + 1;
