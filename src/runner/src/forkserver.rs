use std::convert::TryInto;
use std::env;
use std::ptr;
use std::slice;
use std::str::FromStr;

use anyhow::{bail, Result};
use libc::{c_void, calloc, free, shmat, shmdt};
use nix::{unistd::read, unistd::write};

use crate::constants::*;

enum SharedMemPtr {
    /// Allocated using `shmat`, must be deallocated with `shmdt`
    Shm(*mut c_void),
    /// Allocated
    Anon(*mut c_void),
}

pub enum RunStatus {
    Success,
    Failure,
}

/// This struct implements a fake AFL++ forkserver that does not actually fork children. Instead,
/// we'll do our own persistent mode [0].
///
/// The reference implementation can be found at [1].
///
/// [0]: https://github.com/AFLplusplus/AFLplusplus/blob/stable/llvm_mode/README.persistent_mode.md
/// [1]: https://github.com/AFLplusplus/AFLplusplus/blob/stable/gcc_plugin/afl-gcc-rt.o.c
pub struct Forkserver {
    /// `false` implies AFL++ is running us. `true` implies we're in standalone mode (most likely
    /// to reproduce a test).
    disabled: bool,
    shared_mem: SharedMemPtr,
}

impl Forkserver {
    pub fn new() -> Result<Self> {
        let mut disabled = env::var_os("AFL_NO_FORKSRV").is_some();

        // https://github.com/AFLplusplus/AFLplusplus/blob/fac108476c1cb5/include/config.h#L305
        let shared_mem = match env::var_os("__AFL_SHM_ID") {
            Some(id) => {
                let id = i32::from_str(&id.as_os_str().to_string_lossy())?;
                let ptr = unsafe { shmat(id, ptr::null(), 0) };
                if ptr == -1i64 as *mut c_void {
                    bail!("Failed to shmat() edge buffer");
                }

                SharedMemPtr::Shm(ptr)
            }
            None => {
                println!("Running outside of AFL");
                disabled = true;

                let ptr = unsafe { calloc(AFL_MAP_SIZE.try_into()?, 1) };
                if ptr.is_null() {
                    bail!("Failed to calloc() edge buffer");
                }

                SharedMemPtr::Anon(ptr)
            }
        };

        // Phone home and tell parent we're OK
        if !disabled {
            // Must be exactly 4 bytes
            let val: u32 = AFL_FS_OPT_ENABLED
                | AFL_FS_OPT_MAPSIZE
                | Self::forkserver_opt_set_mapsize(AFL_MAP_SIZE);

            if write(AFL_FORKSERVER_WRITE_FD, &val.to_ne_bytes())? != 4 {
                bail!("Forkserver failed to phone home");
            }
        }

        Ok(Self {
            disabled,
            shared_mem,
        })
    }

    /// Encode map size to a value we pass through the our phone home
    fn forkserver_opt_set_mapsize(size: u32) -> u32 {
        (size - 1) << 1
    }

    /// Get edge transition shared memory buffer
    pub fn shmem(&mut self) -> &mut [u8] {
        let ptr = match self.shared_mem {
            SharedMemPtr::Shm(p) => p,
            SharedMemPtr::Anon(p) => p,
        };

        unsafe { slice::from_raw_parts_mut(ptr as *mut u8, AFL_MAP_SIZE.try_into().unwrap()) }
    }

    /// Initiate a new test run with AFL
    pub fn new_run(&mut self) -> Result<()> {
        if self.disabled {
            return Ok(());
        }

        // Exactly 4 bytes
        let mut was_killed: Vec<u8> = vec![0; 4];

        // We don't really care if AFL "killed" our child b/c we gave AFL a dummy PID anyways,
        // so ignore `was_killed` result
        if read(AFL_FORKSERVER_READ_FD, &mut was_killed)? != 4 {
            bail!("Failed to read was_killed from AFL");
        }

        let fake_pid = i32::MAX.to_ne_bytes();
        if write(AFL_FORKSERVER_WRITE_FD, &fake_pid)? != 4 {
            bail!("Failed to give AFL fake_pid");
        }

        Ok(())
    }

    /// Report result of test run to AFL
    pub fn report(&mut self, status: RunStatus) -> Result<()> {
        if self.disabled {
            return Ok(());
        }

        let val: i32 = match status {
            RunStatus::Success => 0,
            // 139 is SIGSEGV terminated exit code as encoded in `wait(2)`s `wstatus`
            RunStatus::Failure => 139,
        };

        if write(AFL_FORKSERVER_WRITE_FD, &val.to_ne_bytes())? != 4 {
            bail!("Failed to report status to AFL");
        }

        Ok(())
    }
}

impl Drop for Forkserver {
    fn drop(&mut self) {
        match self.shared_mem {
            SharedMemPtr::Shm(p) => {
                if unsafe { shmdt(p) } != 0 {
                    // Panic instead of leak memory over time
                    panic!("Failed to shmdt() edge buffer");
                }
            }
            SharedMemPtr::Anon(p) => unsafe { free(p) },
        }
    }
}
