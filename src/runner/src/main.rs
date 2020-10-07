use std::cmp;
use std::convert::TryInto;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::exit;
use std::sync::atomic::Ordering;

use anyhow::{bail, Context, Result};
use libc::c_void;
use nix::errno::{errno, Errno};
use nix::fcntl::{open, OFlag};
use nix::ioctl_write_ptr;
use nix::sys::stat::Mode;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, lseek, ForkResult, Whence};
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

enum TestcaseStatus {
    Ok,
    NoMore,
}

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
                if line.contains("BUG") || line.contains("UBSAN:") {
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
fn get_next_testcase<P: AsRef<Path>>(into: P) -> Result<TestcaseStatus> {
    let mut buffer = Vec::new();

    // AFL feeds inputs via stdin
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    handle.read_to_end(&mut buffer)?;
    if buffer.is_empty() {
        return Ok(TestcaseStatus::NoMore);
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

    Ok(TestcaseStatus::Ok)
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
            Err(e) => {
                eprintln!("Mount error: {}", e);
                return;
            }
        }
    }

    let nested_dir_path = "/mnt/btrfs/one/two/three/four/five/six";
    let ret = create_dir_all(nested_dir_path);
    if debug {
        match ret {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to create some directories in work fn: {}", e);
                return;
            }
        }
    }

    let mut file = match OpenOptions::new()
        .create(true)
        .write(true)
        .open(format!("{}/file", nested_dir_path))
    {
        Ok(f) => f,
        Err(e) => {
            if debug {
                eprintln!("Failed to open test file: {}", e);
            }

            return;
        }
    };

    match writeln!(file, "hello world") {
        Ok(_) => (),
        Err(e) => {
            if debug {
                eprintln!("Failed to write to test file: {}", e);
                return;
            }
        }
    }

    match file.sync_all() {
        Ok(()) => (),
        Err(e) => {
            if debug {
                eprintln!("Failed to sync test file: {}", e);
                return;
            }
        }
    }
}

/// Fork a child and execute test case.
///
/// NB: Returning an error crashes the fuzzer. DO NOT return an error unless it's truly unrecoverable.
fn fork_work_and_wait<P: AsRef<Path>>(
    kcov: &mut Kcov,
    kmsg: i32,
    mounter: &mut Mounter,
    image: P,
    debug: bool,
) -> Result<RunStatus> {
    const EXIT_OK: i32 = 88;
    const EXIT_BAD: i32 = 89;

    match fork()? {
        ForkResult::Parent { child } => {
            let res = waitpid(child, None)?;

            if kmsg_contains_bug(kmsg)? {
                return Ok(RunStatus::Failure);
            }

            match res {
                WaitStatus::Exited(pid, rc) => {
                    if rc != EXIT_OK {
                        bail!("Forked child={} had an unclean exit={}", pid, rc);
                    }

                    Ok(RunStatus::Success)
                }
                WaitStatus::Signaled(_, _, _) => Ok(RunStatus::Failure),
                _ => bail!("Unexpected waitpid() status={:?}", res),
            }
        }
        // Be careful not to return from the child branch -- we must always exit the child
        // process so the parent can reap our status.
        ForkResult::Child => {
            match kcov.enable() {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to enable kcov: {}", e);
                    exit(EXIT_BAD);
                }
            }

            work(mounter, image, debug);

            // Kcov is automatically disabled when the child terminates
            exit(EXIT_OK);
        }
    }
}

fn _main() -> Result<()> {
    let opts = Opt::from_args();

    // Initialize forkserver and handshake with AFL
    let mut forkserver = Forkserver::new()?;

    // Initialize kernel coverage interface
    let mut kcov = Kcov::new()?;

    // Open /dev/kmsg
    let kmsg = open_kmsg()?;

    // Create a persistent loopdev to use
    let mut mounter = Mounter::new()?;

    loop {
        // Tell AFL we want to start a new run
        forkserver.new_run()?;

        // Now pull the next testcase from AFL and write it to tmpfs
        match get_next_testcase(FUZZED_IMAGE_PATH)? {
            TestcaseStatus::Ok => (),
            TestcaseStatus::NoMore => break,
        };

        // Reset kernel state
        reset_btrfs_devices()?;

        // Fork a child and perform test
        let status =
            fork_work_and_wait(&mut kcov, kmsg, &mut mounter, FUZZED_IMAGE_PATH, opts.debug)?;

        // When the child exits coverage is disabled so we're good to read memory mapped data here
        let coverage = kcov.coverage();
        let size = coverage[0].load(Ordering::Relaxed);

        if opts.debug {
            println!("{} kcov entries", size);
        }

        // Report edge transitions to AFL
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
        forkserver.report(status)?;
    }

    Ok(())
}

fn main() {
    match _main() {
        Ok(_) => exit(0),
        Err(e) => {
            eprintln!("Unclean runner exit: {}", e);
            exit(1);
        }
    }
}
