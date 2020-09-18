use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use loopdev::{LoopControl, LoopDevice};
use sys_mount::{FilesystemType, MountFlags, Unmount, UnmountFlags};

pub struct Mount {
    loopdev: LoopDevice,
    fs_mount: sys_mount::Mount,
}

impl Mount {
    pub fn new<P: AsRef<Path>>(src: P, dest: &'static str) -> Result<Self> {
        // Will fail if directory already exists
        let _ = fs::create_dir(dest);

        let control =
            LoopControl::open().with_context(|| "Failed to open loop control".to_string())?;
        let device = control
            .next_free()
            .with_context(|| "Failed to get next free loop dev".to_string())?;
        device
            .attach_file(src)
            .with_context(|| "Failed to attach file to loop dev".to_string())?;

        let mount = sys_mount::Mount::new(
            device
                .path()
                .ok_or_else(|| anyhow!("Failed to get path of loop dev"))?,
            dest,
            FilesystemType::Manual("btrfs"),
            MountFlags::empty(),
            None,
        )
        .with_context(|| "Failed to mount btrfs image".to_string())?;

        Ok(Self {
            loopdev: device,
            fs_mount: mount,
        })
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        // Close fs mount before detaching loop dev as the fs mount holds a refcount
        // in the kernel.
        //
        // Panic here if detaching fails b/c otherwise we'd slowly leak resources.
        self.fs_mount.unmount(UnmountFlags::empty()).unwrap();

        // See above
        self.loopdev.detach().unwrap();
    }
}
