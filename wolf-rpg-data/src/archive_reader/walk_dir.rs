use super::ArchiveReader;
use super::FileEntry;
use crate::Error;

/// An iterator over a dir and its descendants.
#[derive(Debug)]
pub struct WalkDirIter<'a, R> {
    archive_reader: &'a ArchiveReader<R>,
    stack: Vec<(&'a FileEntry, Vec<&'a str>)>,
}

impl<'a, R> WalkDirIter<'a, R> {
    /// Make a new walk dir iter.
    pub(super) fn new(archive_reader: &'a ArchiveReader<R>, file_entry: &'a FileEntry) -> Self {
        Self {
            archive_reader,
            stack: vec![(file_entry, Vec::new())],
        }
    }
}

impl<'a, R> Iterator for WalkDirIter<'a, R> {
    type Item = Result<WalkDirEntry<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let (file_entry, path_components) = self.stack.pop()?;

        if file_entry.is_dir() {
            let dir_entry = match self.archive_reader.get_dir_from_file(file_entry) {
                Ok(dir_entry) => dir_entry,
                Err(error) => return Some(Err(error)),
            };

            for file_index in (0..dir_entry.num_files()).rev() {
                let file_index = usize::try_from(file_index).unwrap();
                let file_entry = match self.archive_reader.get_dir_file(dir_entry, file_index) {
                    Ok(Some(file_entry)) => file_entry,
                    Ok(None) => return Some(Err(Error::InvalidFilePosition)),
                    Err(error) => return Some(Err(error)),
                };
                let file_name = match self.archive_reader.get_file_name(file_entry) {
                    Ok(file_name) => file_name,
                    Err(error) => return Some(Err(error)),
                };

                let mut path_components = path_components.clone();
                path_components.push(file_name);

                self.stack.push((file_entry, path_components));
            }
        }

        Some(Ok(WalkDirEntry {
            file_entry,
            path_components,
        }))
    }
}

/// A file or dir entry.
#[derive(Debug)]
pub struct WalkDirEntry<'a> {
    file_entry: &'a FileEntry,
    path_components: Vec<&'a str>,
}

impl<'a> WalkDirEntry<'a> {
    /// Get the file.
    ///
    /// Note that this may be the file data for a dir.
    pub fn file(&self) -> &'a FileEntry {
        self.file_entry
    }

    /// Get the path components
    pub fn path_components(&self) -> &[&'a str] {
        self.path_components.as_slice()
    }
}
