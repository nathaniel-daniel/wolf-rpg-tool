use anyhow::Context;
use std::fs::File;
use std::path::PathBuf;
use wolf_rpg_data::ArchiveReader;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).context("missing path")?;
    let file = File::open(path)?;
    let mut reader = ArchiveReader::new(file);

    let output = PathBuf::from("out");

    std::fs::create_dir_all(&output)?;

    reader.read_header()?;

    dbg!(&reader);

    let root_dir = reader.get_root_dir()?.context("no root dir")?;

    for entry in reader.walk_dir(root_dir)? {
        let entry = entry?;
        let file = entry.file();
        let path_components = entry.path_components();

        dbg!(path_components);

        let mut output = output.clone();
        output.extend(path_components);

        if file.is_dir() {
            std::fs::create_dir_all(output)?;
        } else {
            let mut reader = reader.get_file_reader(file)?;

            let mut file = File::create(output)?;
            std::io::copy(&mut reader, &mut file)?;
        }
    }

    println!("Hello, world!");

    Ok(())
}
