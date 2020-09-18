use std::cmp;
use std::hash::Hasher;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::{bail, Result};
use libc::c_void;
use nix::errno::{errno, Errno};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::{lseek, Whence};
use siphasher::sip::SipHasher;
use structopt::StructOpt;

mod constants;
mod forkserver;
mod kcov;
mod mount;

use forkserver::{Forkserver, RunStatus};
use kcov::Kcov;
use mount::Mount;

#[derive(Debug, StructOpt)]
#[structopt(name = "runner", about = "Run btrfs-fuzz test cases")]
struct Opt {
    /// Path to filesystem image under test
    #[structopt(parse(from_os_str))]
    image: PathBuf,
}

/// Opens kmsg fd and seeks to end.
///
/// Note we avoid using the higher level std::fs interfaces b/c /dev/kmsg is a bit special in that
/// each read(2) returns exactly 1 entry in the kernel's printk buffer. So we don't want any high
/// level APIs issuing multiple reads. The fd must also be opened in non-blocking mode otherwise
/// reads will block until a new entry is available.
fn open_kmsg() -> Result<i32> {
    let fd = open(
        "/dev/kmsg",
        OFlag::O_RDONLY | OFlag::O_NONBLOCK,
        Mode::empty(),
    )?;
    lseek(fd, 0, Whence::SeekEnd)?;
    Ok(fd)
}

fn kmsg_contains_bug(fd: i32) -> Result<bool> {
    let mut buf: Vec<u8> = vec![0; 8192];

    loop {
        let n = unsafe { libc::read(fd, (&mut buf).as_mut_ptr() as *mut c_void, buf.len()) };
        match n.cmp(&0) {
            cmp::Ordering::Equal => break,
            cmp::Ordering::Less => {
                let errno = Errno::from_i32(errno());
                if errno == Errno::EAGAIN {
                    // No more entries in kmsg
                    break;
                } else {
                    bail!("Failed to read from /dev/kmsg");
                }
            }
            cmp::Ordering::Greater => {
                buf[n as usize] = 0;

                let line = String::from_utf8_lossy(&buf);
                if line.contains("Call Trace") || line.contains("RIP:") || line.contains("Code:") {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Test code
///
/// Note how this doesn't return errors. That's because our definition of error is a kernel BUG()
/// or panic. We expect that some operations here fail (such as mount(2))
fn work(image: &PathBuf) {
    // The `_` is an immediate drop after creation
    let _ = Mount::new(image, "/mnt/btrfs");
}

fn main() -> Result<()> {
    let opts = Opt::from_args();

    // Initialize forkserver and handshake with AFL
    let mut forkserver = Forkserver::new()?;

    // Initialize kernel coverage interface
    let mut kcov = Kcov::new()?;

    // Open /dev/kmsg
    let kmsg = open_kmsg()?;

    loop {
        // Tell AFL we want to start a new run
        forkserver.new_run()?;

        // Start coverage collection, do work, then disable collection
        kcov.enable()?;
        work(&opts.image);
        let size = kcov.disable()?;

        // Report edge transitions to AFL
        let coverage = kcov.coverage();
        let shmem = forkserver.shmem();
        let mut hasher = SipHasher::new();
        let mut prev_loc: u64 = 0;
        for i in 0..size {
            // First calculate which idx in shmem to write to
            let current_loc = coverage[i + 1].load(Ordering::Relaxed);
            hasher.write(&current_loc.to_ne_bytes());
            let current_loc_hash: u64 = hasher.finish();
            let mixed: u64 = (current_loc_hash & 0xFFFF) ^ prev_loc;
            prev_loc = (current_loc_hash & 0xFFFF) >> 1;

            // Increment value in shmem
            let (val, overflow) = shmem[mixed as usize].overflowing_add(1);
            if overflow {
                shmem[mixed as usize] = u8::MAX;
            } else {
                shmem[mixed as usize] = val;
            }
        }

        // Report run status to AFL
        if kmsg_contains_bug(kmsg)? {
            forkserver.report(RunStatus::Failure)?;
        } else {
            forkserver.report(RunStatus::Success)?;
        }
    }
}
