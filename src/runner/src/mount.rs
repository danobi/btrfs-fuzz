use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use loopdev::{LoopControl, LoopDevice};
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use sys_mount::{FilesystemType, MountFlags, Unmount, UnmountFlags};

pub struct Mounter {
    loopdev: LoopDevice,
    /// If a file is attached to the loopdev
    attached: AtomicBool,
}

impl Mounter {
    pub fn new() -> Result<Self> {
        let control =
            LoopControl::open().with_context(|| "Failed to open loop control".to_string())?;
        let device = control
            .next_free()
            .with_context(|| "Failed to get next free loop dev".to_string())?;

        // Disable O_CLOEXEC on underlying loopdev FD so that instances of this mounter
        // may be used in forked child processes.
        let fd = device.as_raw_fd();
        let mut flags = FdFlag::from_bits(fcntl(fd, FcntlArg::F_GETFD)?)
            .ok_or_else(|| anyhow!("Failed to interpret FdFlag"))?;
        flags &= !FdFlag::FD_CLOEXEC;
        fcntl(fd, FcntlArg::F_SETFD(flags))?;

        Ok(Self {
            loopdev: device,
            attached: AtomicBool::new(false),
        })
    }

    pub fn mount<P: AsRef<Path>>(&mut self, src: P, dest: &'static str) -> Result<Mount> {
        // Will fail if directory already exists
        let _ = fs::create_dir(dest);

        if self.attached.load(Ordering::SeqCst) {
            bail!("Loop dev is still being used by a previous mount");
        }

        self.loopdev
            .attach_file(src)
            .with_context(|| "Failed to attach file to loop dev".to_string())?;
        self.attached.store(true, Ordering::SeqCst);

        let mount = sys_mount::Mount::new(
            self.loopdev
                .path()
                .ok_or_else(|| anyhow!("Failed to get path of loop dev"))?,
            dest,
            FilesystemType::Manual("btrfs"),
            MountFlags::empty(),
            None,
        )
        .with_context(|| "Failed to mount btrfs image".to_string());

        match mount {
            Ok(m) => Ok(Mount {
                inner: m,
                loopdev: &self.loopdev,
                attached: &self.attached,
            }),
            Err(e) => {
                // Be careful to detach the backing file from the loopdev if the mount fails,
                // otherwise following attaches will fail with EBUSY
                self.loopdev.detach()?;
                self.attached.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }
}

impl Drop for Mounter {
    fn drop(&mut self) {
        // Panic here if detaching fails b/c otherwise we'd slowly leak resources.
        if self.attached.load(Ordering::SeqCst) {
            self.loopdev.detach().unwrap();
        }
    }
}

/// A mounted filesystem.
///
/// Will umount on drop.
pub struct Mount<'a> {
    inner: sys_mount::Mount,
    loopdev: &'a LoopDevice,
    attached: &'a AtomicBool,
}

impl<'a> Drop for Mount<'a> {
    fn drop(&mut self) {
        // Panic here if detaching fails b/c otherwise we'd slowly leak resources.
        self.inner.unmount(UnmountFlags::empty()).unwrap();
        self.loopdev.detach().unwrap();
        self.attached.store(false, Ordering::SeqCst);
    }
}
