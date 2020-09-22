use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use loopdev::{LoopControl, LoopDevice};
use sys_mount::{FilesystemType, MountFlags, Unmount, UnmountFlags};

pub struct Mounter {
    loopdev: LoopDevice,
}

impl Mounter {
    pub fn new() -> Result<Self> {
        let control =
            LoopControl::open().with_context(|| "Failed to open loop control".to_string())?;
        let device = control
            .next_free()
            .with_context(|| "Failed to get next free loop dev".to_string())?;

        Ok(Self { loopdev: device })
    }

    pub fn mount<P: AsRef<Path>>(&mut self, src: P, dest: &'static str) -> Result<Mount> {
        // Will fail if directory already exists
        let _ = fs::create_dir(dest);

        self.loopdev
            .attach_file(src)
            .with_context(|| "Failed to attach file to loop dev".to_string())?;

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
            }),
            Err(e) => {
                // Be careful to detach the backing file from the loopdev if the mount fails,
                // otherwise following attaches will fail with EBUSY
                self.loopdev.detach()?;
                Err(e)
            }
        }
    }
}

impl Drop for Mounter {
    fn drop(&mut self) {
        // Panic here if detaching fails b/c otherwise we'd slowly leak resources.
        self.loopdev.detach().unwrap();
    }
}

/// A mounted filesystem.
///
/// Will umount on drop.
pub struct Mount<'a> {
    inner: sys_mount::Mount,
    loopdev: &'a LoopDevice,
}

impl<'a> Drop for Mount<'a> {
    fn drop(&mut self) {
        // Panic here if detaching fails b/c otherwise we'd slowly leak resources.
        self.inner.unmount(UnmountFlags::empty()).unwrap();
        self.loopdev.detach().unwrap();
    }
}
