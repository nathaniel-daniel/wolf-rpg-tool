use anyhow::Context;
use std::collections::VecDeque;
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

    // dbg!(&reader);

    let root_dir = reader.get_root_directory()?.context("no root directory")?;

    let mut queue = VecDeque::new();
    queue.push_back((root_dir, Vec::new()));
    while let Some((dir, path)) = queue.pop_front() {
        for file_index in 0..dir.num_files() {
            let file_index = usize::try_from(file_index)?;
            let file = reader
                .get_directory_file(dir, file_index)?
                .context("no file")?;

            let file_name = reader.get_file_name(file)?;

            let mut path = path.clone();
            path.push(file_name);
            dbg!(&path);

            if file.is_dir() {
                let dir = reader.get_dir_from_file(file)?;
                queue.push_back((dir, path));
            } else {
                let mut output = output.clone();
                output.extend(&path);

                if let Some(parent) = output.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut reader = reader.get_file_reader(file)?;

                let mut file = File::create(output)?;
                std::io::copy(&mut reader, &mut file)?;
            }
        }
    }

    println!("Hello, world!");

    Ok(())
}
