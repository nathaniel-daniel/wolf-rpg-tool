use anyhow::Context;
use std::fs::File;
use wolf_rpg_data::ArchiveReader;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).context("missing path")?;
    let file = File::open(path)?;
    let mut reader = ArchiveReader::new(file);

    reader.read_header()?;

    let root_dir = reader.get_root_directory()?.context("no root directory")?;
    dbg!(root_dir.num_files());
    dbg!(root_dir);

    let mut stack = vec![(root_dir, Vec::new())];
    while let Some((dir, path)) = stack.pop() {
        for file_index in 0..dir.num_files() {
            let file_index = usize::try_from(file_index)?;
            let file = reader
                .get_directory_file(dir, file_index)?
                .context("no file")?;

            let file_name = reader.get_file_name(file)?;
            dbg!(&path);
            dbg!(file_name);

            if file.is_dir() {
                let dir = reader.get_dir_from_file(file)?;
                let mut path = path.clone();
                path.push(file_name);
                stack.push((dir, path));
            }
        }
    }

    println!("Hello, world!");

    Ok(())
}
