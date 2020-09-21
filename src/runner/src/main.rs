use std::cmp;
use std::hash::Hasher;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;

use anyhow::{bail, Context, Result};
use libc::c_void;
use nix::errno::{errno, Errno};
use nix::fcntl::{open, OFlag};
use nix::sys::stat::Mode;
use nix::unistd::{lseek, Whence};
use siphasher::sip::SipHasher;
use structopt::StructOpt;
use tempfile::Builder;

mod constants;
mod forkserver;
mod kcov;
mod mount;

use forkserver::{Forkserver, RunStatus};
use kcov::Kcov;
use mount::Mount;

const FUZZED_IMAGE_PATH: &str = "/tmp/btrfsimage";

#[derive(Debug, StructOpt)]
#[structopt(name = "runner", about = "Run btrfs-fuzz test cases")]
struct Opt {
    /// Turn on debug output
    #[structopt(short, long)]
    debug: bool,
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
                if line.contains("Call Trace")
                    || line.contains("RIP:")
                    || line.contains("Code:")
                    || line.contains("BUG")
                {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Get next testcase from AFL and write it into file `into`
fn get_next_testcase<P: AsRef<Path>>(into: P) -> Result<()> {
    let mut buffer = Vec::new();

    // AFL feeds inputs via stdin
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_to_end(&mut buffer)?;

    // Write out minimized btrfs-image to a tempfile
    let tempfile = Builder::new().prefix("btrfs-image-min").tempfile()?;
    tempfile.as_file().write_all(&buffer)?;

    // Expand image image
    let path = tempfile.into_temp_path();
    let output = Command::new("/bin/btrfs-image")
        .arg("-r")
        .arg(path)
        .arg(into.as_ref())
        .output()?;

    if !output.status.success() {
        if let Some(c) = output.status.code() {
            bail!(
                "btrfs-image failed with exit code={}. Error={}",
                c,
                &String::from_utf8_lossy(&output.stderr)
            );
        } else {
            bail!(
                "btrfs-image was terminated by a signal. Error={}",
                &String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    Ok(())
}

/// Test code
///
/// Note how this doesn't return errors. That's because our definition of error is a kernel BUG()
/// or panic. We expect that some operations here fail (such as mount(2))
fn work<P: AsRef<Path>>(image: P) {
    // The `_` is an immediate drop after creation
    let _ = Mount::new(image.as_ref(), "/mnt/btrfs");
}

fn main() -> Result<()> {
    let _opts = Opt::from_args();

    // Initialize forkserver and handshake with AFL
    let mut forkserver = Forkserver::new()?;

    // Initialize kernel coverage interface
    let mut kcov = Kcov::new()?;

    // Open /dev/kmsg
    let kmsg = open_kmsg()?;

    loop {
        // Tell AFL we want to start a new run
        forkserver.new_run()?;

        // Now pull the next testcase from AFL and write it to tmpfs
        get_next_testcase(FUZZED_IMAGE_PATH)
            .with_context(|| "Failed to get next testcase".to_string())?;

        // Start coverage collection, do work, then disable collection
        kcov.enable()?;
        work(FUZZED_IMAGE_PATH);
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
