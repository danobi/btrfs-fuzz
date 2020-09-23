use std::fs::OpenOptions;
use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use rmp_serde::Serializer;
use serde::Serialize;
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

fn compress(input: PathBuf, output: PathBuf) -> Result<()> {
    let mut input = OpenOptions::new().read(true).open(input)?;

    let mut input_image = Vec::new();
    input.read_to_end(&mut input_image)?;
    let compressed_image = imgcompress::compress(&input_image)?;

    let output = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(output)?;
    compressed_image.serialize(&mut Serializer::new(output))?;

    Ok(())
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
