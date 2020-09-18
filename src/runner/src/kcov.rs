use std::convert::TryInto;
use std::fs::{File, OpenOptions};
use std::mem::size_of;
use std::os::unix::io::AsRawFd;
use std::ptr;
use std::slice;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use anyhow::{bail, Context, Result};
use nix::{ioctl_read, ioctl_write_int_bad, request_code_none};

const COVER_SIZE: usize = 16 << 10;

/// See include/uapi/linux/kcov.h
const KCOV_IOCTL_MAGIC: u8 = b'c';
const KCOV_INIT_TRACE_IOCTL_SEQ: u8 = 1;
const KCOV_ENABLE_IOCTL_SEQ: u8 = 100;
const KCOV_DISABLE_IOCTL_SEQ: u8 = 101;
const KCOV_TRACE_PC: u64 = 0;

ioctl_read!(
    kcov_init_trace,
    KCOV_IOCTL_MAGIC,
    KCOV_INIT_TRACE_IOCTL_SEQ,
    u64
);
// Can't use nix::ioctl_none b/c kcov "broke" API by accepting an arg in a no-arg ioctl
ioctl_write_int_bad!(
    kcov_enable,
    request_code_none!(KCOV_IOCTL_MAGIC, KCOV_ENABLE_IOCTL_SEQ)
);
ioctl_write_int_bad!(
    kcov_disable,
    request_code_none!(KCOV_IOCTL_MAGIC, KCOV_DISABLE_IOCTL_SEQ)
);

pub struct Kcov {
    fd: i32,
    ptr: *mut libc::c_void,
    /// Must hold onto the kcov control file b/c `as_raw_fd()` does not transfer ownership
    _file: File,
}

impl Kcov {
    pub fn new() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/sys/kernel/debug/kcov")
            .with_context(|| "Failed to open kcov control file".to_string())?;
        let fd = file.as_raw_fd();

        if unsafe {
            kcov_init_trace(fd, COVER_SIZE as *mut u64)
                .with_context(|| "Failed to KCOV_INIT_TRACE".to_string())?
        } != 0
        {
            bail!("Failed to KCOV_INIT_TRACE");
        }

        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                COVER_SIZE * size_of::<usize>(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            bail!("Failed to mmap shared kcov buffer");
        }

        Ok(Self {
            fd,
            ptr,
            _file: file,
        })
    }

    pub fn enable(&mut self) -> Result<()> {
        // Reset the counter
        self.coverage()[0].store(0, Ordering::Relaxed);

        if unsafe {
            kcov_enable(self.fd, KCOV_TRACE_PC.try_into().unwrap())
                .with_context(|| "Failed to enable kcov PC tracing".to_string())?
        } != 0
        {
            bail!("Failed to enable kcov PC tracing");
        }

        // Reset counter again in case we traced anything as the ioctl returned
        self.coverage()[0].store(0, Ordering::Relaxed);

        Ok(())
    }

    pub fn disable(&mut self) -> Result<usize> {
        let len = self.coverage()[0].load(Ordering::Relaxed);

        if unsafe {
            kcov_disable(self.fd, 0 as i32)
                .with_context(|| "Failed to disable kcov tracing".to_string())?
        } != 0
        {
            bail!("Failed to disable kcov tracing");
        }

        Ok(len)
    }

    pub fn coverage(&self) -> &[AtomicUsize] {
        // We can transmute from `usize` to `AtomicUsize` b/c they have the same in-memory
        // representations (as promised by the docs)
        unsafe { slice::from_raw_parts(self.ptr as *const AtomicUsize, COVER_SIZE) }
    }
}

impl Drop for Kcov {
    /// Panic if we fail to free resources. Current thinking is it's better to fail
    /// early here and cause afl to report a crash rather than slowly leak memory.
    fn drop(&mut self) {
        if unsafe { libc::munmap(self.ptr, COVER_SIZE * size_of::<usize>()) } != 0 {
            panic!("Failed to munmap shared kcov buffer");
        }

        if unsafe { libc::close(self.fd) } != 0 {
            panic!("Failed to close kcov fd");
        }
    }
}
