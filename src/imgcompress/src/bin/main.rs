use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::Result;
use rmp_serde::{decode::from_read_ref, Serializer};
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

fn decompress(input: PathBuf, output: PathBuf) -> Result<()> {
    let mut input = OpenOptions::new().read(true).open(input)?;

    let mut serialized_input = Vec::new();
    input.read_to_end(&mut serialized_input)?;
    let deserialized_input: imgcompress::CompressedBtrfsImage = from_read_ref(&serialized_input)?;
    let decompressed_image = imgcompress::decompress(&deserialized_input)?;

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(output)?;
    output.write_all(&decompressed_image)?;

    Ok(())
}

fn main() -> Result<()> {
    let opts = Opt::from_args();

    match opts.cmd {
        Command::Compress { input, output } => compress(input, output),
        Command::Decompress { input, output } => decompress(input, output),
    }
}
