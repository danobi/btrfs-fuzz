use std::path::PathBuf;
use std::sync::atomic::Ordering;

use anyhow::Result;
use structopt::StructOpt;

mod constants;
mod forkserver;
mod kcov;
mod mount;

use forkserver::Forkserver;
use kcov::Kcov;
use mount::Mount;

#[derive(Debug, StructOpt)]
#[structopt(name = "runner", about = "Run btrfs-fuzz test cases")]
struct Opt {
    /// Path to filesystem image under test
    #[structopt(parse(from_os_str))]
    image: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opt::from_args();

    let mut _forkserver = Forkserver::new()?;

    let mut kcov = Kcov::new()?;
    kcov.enable()?;

    // The `_` is an immediate drop after creation
    let _ = Mount::new(&opts.image, "/mnt/btrfs")?;

    let size = kcov.disable()?;
    kcov.coverage()
        .iter()
        .skip(1) // control index
        .take(size)
        .for_each(|v| println!("0x{:x}", v.load(Ordering::Relaxed)));

    Ok(())
}
