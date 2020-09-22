use std::path::PathBuf;

use anyhow::Result;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "imgcompress", about = "Compress a btrfs image")]
struct Opt {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Compress a btrfs image
    Compress {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        #[structopt(parse(from_os_str))]
        output: PathBuf,
    },
    /// Decompress an imgcompress'd btrfs image
    Decompress {
        #[structopt(parse(from_os_str))]
        input: PathBuf,
        #[structopt(parse(from_os_str))]
        output: PathBuf,
    },
}

fn compress(_input: PathBuf, _output: PathBuf) -> Result<()> {
    unimplemented!();
}

fn decompress(_input: PathBuf, _output: PathBuf) -> Result<()> {
    unimplemented!();
}

fn main() -> Result<()> {
    let opts = Opt::from_args();

    match opts.cmd {
        Command::Compress { input, output } => compress(input, output),
        Command::Decompress { input, output } => decompress(input, output),
    }
}
