use anyhow::Context;
use std::fs::File;
use wolf_rpg_data::ArchiveReader;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).context("missing path")?;
    let file = File::open(path)?;
    let mut reader = ArchiveReader::new(file);

    reader.read_header()?;

    dbg!(&reader);

    println!("Hello, world!");

    Ok(())
}
