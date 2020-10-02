use std::cmp;
use std::convert::TryInto;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use anyhow::{bail, Context, Result};
use libc::c_void;
use nix::errno::{errno, Errno};
use nix::fcntl::{open, OFlag};
use nix::ioctl_write_ptr;
use nix::sys::stat::Mode;
use nix::unistd::{lseek, Whence};
use rmp_serde::decode::from_read_ref;
use static_assertions::const_assert;
use structopt::StructOpt;

mod constants;
mod forkserver;
mod kcov;
mod mount;

use forkserver::{Forkserver, RunStatus};
use kcov::Kcov;
use mount::Mounter;

const FUZZED_IMAGE_PATH: &str = "/tmp/btrfsimage";

/// See /usr/include/linux/btrfs.h
const BTRFS_IOCTL_MAGIC: u8 = 0x94;
const BTRFS_FORGET_DEV_IOCTL_SEQ: u8 = 5;
const BTRFS_PATH_NAME_MAX: usize = 4087;

#[repr(C, packed)]
pub struct BtrfsIoctlVolArgs {
    fd: i64,
    name: [u8; BTRFS_PATH_NAME_MAX + 1],
}
const_assert!(std::mem::size_of::<BtrfsIoctlVolArgs>() == 4096);

ioctl_write_ptr!(
    btrfs_forget_dev,
    BTRFS_IOCTL_MAGIC,
    BTRFS_FORGET_DEV_IOCTL_SEQ,
    BtrfsIoctlVolArgs
);

#[derive(Debug, StructOpt)]
#[structopt(name = "runner", about = "Run btrfs-fuzz test cases")]
struct Opt {
    /// Turn on debug output
    #[structopt(short, long)]
    debug: bool,

    /// Directory to save current test cases into
    ///
    /// Useful when the current test case panics the kernel or crashes `runner` (via `BUG()`).
    /// A management process can pull out the test case and feed it back to `runner` as a
    /// crashing test.
    #[structopt(short, long)]
    current_dir: Option<PathBuf>,
    /// Saves the last N test cases into `--current-dir`
    ///
    /// Only effective when used with `--current-dir`
    #[structopt(short = "n", long, default_value = "15")]
    last_n: u64,
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
    let mut found = false;

    // NB: make sure we consume all the entries in kmsg otherwise the next test might see entries
    // from the previous run
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
                if line.contains("BUG") {
                    found = true;
                }
            }
        }
    }

    Ok(found)
}

/// Get next testcase from AFL and write it into file `into`
///
/// Returns true on success, false on no more input
fn get_next_testcase<P: AsRef<Path>>(
    into: P,
    current_dir: &Option<PathBuf>,
    last_n: u64,
    count: u64,
) -> Result<bool> {
    let mut buffer = Vec::new();

    // AFL feeds inputs via stdin
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_to_end(&mut buffer)?;
    if buffer.is_empty() {
        return Ok(false);
    }

    // Save current input if requested
    if let Some(current_dir) = current_dir {
        let path = current_dir.as_path().join((count % last_n).to_string());

        let mut current = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)?;

        current.write_all(&buffer)?;
    }

    // Decompress input
    let deserialized = from_read_ref(&buffer)?;
    let image = imgcompress::decompress(&deserialized)?;

    // Write out FS image
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(into)?;

    file.write_all(&image)?;

    Ok(true)
}

/// Reset btrfs device cache
///
/// Necessary to clean up kernel state between test cases
fn reset_btrfs_devices() -> Result<()> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/btrfs-control")
        .with_context(|| "Failed to open btrfs control file".to_string())?;
    let fd = file.as_raw_fd();

    let args: BtrfsIoctlVolArgs = unsafe { std::mem::zeroed() };
    unsafe { btrfs_forget_dev(fd, &args) }
        .with_context(|| "Failed to forget btrfs devs".to_string())?;

    Ok(())
}

/// Test code
///
/// Note how this doesn't return errors. That's because our definition of error is a kernel BUG()
/// or panic. We expect that some operations here fail (such as mount(2))
fn work<P: AsRef<Path>>(mounter: &mut Mounter, image: P, debug: bool) {
    let r = mounter.mount(image.as_ref(), "/mnt/btrfs");

    if debug {
        match r {
            Ok(_) => (),
            Err(e) => println!("Mount error: {}", e),
        }
    }
}

fn main() -> Result<()> {
    let opts = Opt::from_args();

    // Initialize forkserver and handshake with AFL
    let mut forkserver = Forkserver::new()?;

    // Initialize kernel coverage interface
    let mut kcov = Kcov::new()?;

    // Open /dev/kmsg
    let kmsg = open_kmsg()?;

    // Create a persistent loopdev to use
    let mut mounter = Mounter::new()?;

    let mut count: u64 = 0;

    loop {
        // Tell AFL we want to start a new run
        forkserver.new_run()?;

        // Now pull the next testcase from AFL and write it to tmpfs
        if !get_next_testcase(FUZZED_IMAGE_PATH, &opts.current_dir, opts.last_n, count)? {
            break;
        }

        // Reset kernel state
        reset_btrfs_devices()?;

        // Start coverage collection, do work, then disable collection
        kcov.enable()?;
        work(&mut mounter, FUZZED_IMAGE_PATH, opts.debug);
        let size = kcov.disable()?;

        if opts.debug {
            println!("{} kcov entries", size);
        }

        // Report edge transitions to AFL
        let coverage = kcov.coverage();
        let shmem = forkserver.shmem();
        let mut prev_loc: u64 = 0xDEAD; // Our compile time "random"
        for i in 0..size {
            // First calculate which idx in shmem to write to
            let current_loc: u64 = coverage[i + 1].load(Ordering::Relaxed).try_into().unwrap();
            // Mask with 0xFFFF for 16 bits b/c AFL_MAP_SIZE == 1 << 16
            let mixed: u64 = (current_loc & 0xFFFF) ^ prev_loc;
            prev_loc = (current_loc & 0xFFFF) >> 1;

            // Increment value in shmem
            let (val, overflow) = shmem[mixed as usize].overflowing_add(1);
            if overflow {
                shmem[mixed as usize] = u8::MAX;
            } else {
                shmem[mixed as usize] = val;
            }

            if opts.debug {
                println!("kcov entry: 0x{:x}", current_loc);
            }
        }

        // Report run status to AFL
        if kmsg_contains_bug(kmsg)? {
            forkserver.report(RunStatus::Failure)?;
        } else {
            forkserver.report(RunStatus::Success)?;
        }

        count += 1;
    }

    Ok(())
}
