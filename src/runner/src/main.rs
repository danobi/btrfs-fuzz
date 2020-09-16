use std::sync::atomic::Ordering;
use std::thread;
use std::time;

mod constants;
mod kcov;

use anyhow::Result;

use kcov::Kcov;

fn main() -> Result<()> {
    let mut kcov = Kcov::new()?;
    kcov.enable()?;

    thread::sleep(time::Duration::from_secs(100));

    let size = kcov.disable()?;
    kcov.coverage()
        .iter()
        .skip(1) // control index
        .take(size)
        .for_each(|v| {
            println!("0x{:X}", v.load(Ordering::Relaxed))
        });

    Ok(())
}
