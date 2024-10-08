mod file_entry;
mod file_reader;
mod walk_dir;

pub use self::file_entry::Attributes;
pub use self::file_entry::FileEntry;
pub use self::file_entry::FileTimes;
use self::file_reader::decompress_file_data;
use self::file_reader::CompressedFileReaderInner;
pub use self::file_reader::FileReader;
use self::file_reader::FileReaderInner;
use self::file_reader::UncompressedFileReaderInner;
pub use self::walk_dir::WalkDirIter;
use crate::create_key;
use crate::Error;
use crate::Key;
use crate::DEFAULT_KEY_STRING;
use encoding_rs::SHIFT_JIS;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

const FILE_ENTRY_SIZE: usize = 64;

fn key_xor(position: u64, key: Key, buffer: &mut [u8]) {
    let position_usize = usize::try_from(position).unwrap();
    let key_len = key.len();

    for (i, out_byte) in buffer.iter_mut().enumerate() {
        let key_byte = key[(position_usize + i) % key_len];

        *out_byte ^= key_byte;
    }
}

/// A reader for an archive.
#[derive(Debug)]
pub struct ArchiveReader<R> {
    reader: RefCell<R>,
    position: Cell<u64>,
    key: Key,

    /// The string encoding.
    ///
    /// This is populated by reading the header and should not be used before.
    /// This is not a part of the header data because creating the header data requires an encoding.
    encoding: &'static encoding_rs::Encoding,
    header_data: Option<ArchiveHeaderData>,
}

impl<R> ArchiveReader<R> {
    /// Create a reader for a Data.wolf file.
    ///
    /// Note: Currently, only version 2.20 is supported.
    pub fn new(reader: R) -> Self {
        let key = create_key(DEFAULT_KEY_STRING);
        Self {
            reader: RefCell::new(reader),
            position: Cell::new(0),
            key,

            encoding: SHIFT_JIS,
            header_data: None,
        }
    }

    /// Get the name of a file entry.
    pub fn get_file_name(&self, file_entry: &FileEntry) -> Result<&str, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        let file_name = header_data
            .file_name_table
            .get(&file_entry.name_position)
            .ok_or(Error::InvalidFileNamePosition)?;

        Ok(file_name)
    }

    /// Get a dir from a file that is for a dir.
    pub fn get_dir_from_file(&self, file_entry: &FileEntry) -> Result<&DirectoryEntry, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        if !file_entry.is_dir() {
            return Err(Error::NotADir);
        }

        let directory_entry = header_data
            .directory_table
            .get(&file_entry.data_position)
            .ok_or(Error::InvalidDirectoryPosition)?;

        Ok(directory_entry)
    }

    /// Get the nth child of a directory.
    pub fn get_dir_file(
        &self,
        directory: &DirectoryEntry,
        index: usize,
    ) -> Result<Option<&FileEntry>, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        let num_files = match usize::try_from(directory.num_files) {
            Ok(num_files) => num_files,
            Err(_err) => return Ok(None),
        };

        if index >= num_files {
            return Ok(None);
        }

        let position =
            directory.file_head_position + u64::try_from(index * FILE_ENTRY_SIZE).unwrap();
        let file_entry = header_data
            .file_table
            .get(&position)
            .ok_or(Error::InvalidDirectoryFileIndex)?;

        Ok(Some(file_entry))
    }

    /// Get the file for a dir.
    pub fn get_file_from_dir(&self, directory_entry: &DirectoryEntry) -> Result<&FileEntry, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        let file_entry = header_data
            .file_table
            .get(&directory_entry.directory_position)
            .ok_or(Error::InvalidFilePosition)?;

        Ok(file_entry)
    }

    /// Walk over the given dir.
    pub fn walk_dir(&self, dir: &DirectoryEntry) -> Result<WalkDirIter<R>, Error> {
        let file_entry = self.get_file_from_dir(dir)?;

        Ok(WalkDirIter::new(self, file_entry))
    }
}

impl<R> ArchiveReader<R>
where
    R: Read + Seek,
{
    /// Read encoded bytes to a buffer.
    fn read_encoded(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        let mut reader = self.reader.borrow_mut();
        reader.read_exact(buffer)?;

        let position = self.position.get();
        key_xor(position, self.key, buffer);
        let new_position = position + u64::try_from(buffer.len()).unwrap();

        self.position.set(new_position);

        Ok(())
    }

    /// Read an encoded u16.
    fn read_encoded_u16(&mut self) -> Result<u16, Error> {
        let mut value: [u8; 2] = [0; 2];
        self.read_encoded(&mut value)?;
        Ok(u16::from_le_bytes(value))
    }

    /// Read an encoded u32.
    fn read_encoded_u32(&mut self) -> Result<u32, Error> {
        let mut value: [u8; 4] = [0; 4];
        self.read_encoded(&mut value)?;
        Ok(u32::from_le_bytes(value))
    }

    /// Read an encoded u64.
    fn read_encoded_u64(&mut self) -> Result<u64, Error> {
        let mut value: [u8; 8] = [0; 8];
        self.read_encoded(&mut value)?;
        Ok(u64::from_le_bytes(value))
    }

    /// Read file name data.
    fn read_file_name_data(&mut self) -> Result<(String, String), Error> {
        let len = self.read_encoded_u16()?;
        let parity = self.read_encoded_u16()?;

        if len == 0 {
            if parity != 0 {
                return Err(Error::InvalidFileNameParity);
            }

            return Ok((String::new(), String::new()));
        }

        let mut bytes_upper = vec![0; usize::from(len * 4)];
        self.read_encoded(&mut bytes_upper)?;
        let bytes_upper_parity = bytes_upper
            .iter()
            .fold(0_u16, |acc, byte| acc.wrapping_add((*byte).into()));
        if bytes_upper_parity != parity {
            return Err(Error::InvalidFileNameParity);
        }

        let mut bytes = vec![0; usize::from(len * 4)];
        self.read_encoded(&mut bytes)?;

        let (bytes_upper, is_malformed) = self.encoding.decode_without_bom_handling(&bytes_upper);
        if is_malformed {
            return Err(Error::InvalidFileName);
        }
        let mut bytes_upper = bytes_upper.into_owned();
        while bytes_upper.ends_with("\0") {
            bytes_upper.pop();
        }

        let (bytes, is_malformed) = self.encoding.decode_without_bom_handling(&bytes);
        if is_malformed {
            return Err(Error::InvalidFileName);
        }
        let mut bytes = bytes.into_owned();
        while bytes.ends_with("\0") {
            bytes.pop();
        }

        Ok((bytes_upper, bytes))
    }

    /// Read a file entry.
    fn read_file_entry(&mut self) -> Result<FileEntry, Error> {
        let name_position = self.read_encoded_u64()?;
        let attributes = self.read_encoded_u64()?;
        let created = self.read_encoded_u64()?;
        let accessed = self.read_encoded_u64()?;
        let modified = self.read_encoded_u64()?;
        let data_position = self.read_encoded_u64()?;
        let data_size = self.read_encoded_u64()?;
        let compressed_data_size = self.read_encoded_u64()?;

        let attributes = Attributes::from_bits_retain(attributes);
        let compressed_data_size = if compressed_data_size == u64::MAX {
            None
        } else {
            Some(compressed_data_size)
        };

        Ok(FileEntry {
            name_position,
            attributes,
            file_times: FileTimes {
                created,
                accessed,
                modified,
            },
            data_position,
            data_size,
            compressed_data_size,
        })
    }

    /// Read a directory entry.
    fn read_directory_entry(&mut self) -> Result<DirectoryEntry, Error> {
        let directory_position = self.read_encoded_u64()?;
        let parent_directory_position = self.read_encoded_u64()?;
        let num_files = self.read_encoded_u64()?;
        let file_head_position = self.read_encoded_u64()?;

        let parent_directory_position = if parent_directory_position == u64::MAX {
            None
        } else {
            Some(parent_directory_position)
        };

        Ok(DirectoryEntry {
            directory_position,
            parent_directory_position,
            num_files,
            file_head_position,
        })
    }

    /// Read the header.
    pub fn read_header(&mut self) -> Result<(), Error> {
        if self.header_data.is_some() {
            return Err(Error::HeaderAlreadyRead);
        }

        let mut magic: [u8; 2] = [0; 2];
        self.read_encoded(&mut magic)?;
        if magic != *b"DX" {
            return Err(Error::InvalidMagic { magic });
        }

        let version = self.read_encoded_u16()?;
        if version != 6 {
            return Err(Error::InvalidVersion { version });
        }

        let file_header_size = self.read_encoded_u32()?;
        let data_position = self.read_encoded_u64()?;
        let file_name_table_position = self.read_encoded_u64()?;
        let file_table_position = self.read_encoded_u64()?;
        let directory_table_position = self.read_encoded_u64()?;
        let code_page = self.read_encoded_u64()?;

        self.encoding = match code_page {
            932 => SHIFT_JIS,
            _ => {
                return Err(Error::UnknownCodePage { code_page });
            }
        };

        self.position.set(
            self.reader
                .borrow_mut()
                .seek(SeekFrom::Start(file_name_table_position))?,
        );

        let mut file_name_table = BTreeMap::new();
        let mut file_table = BTreeMap::new();
        let mut directory_table = BTreeMap::new();

        loop {
            let relative_position = self.position.get() - file_name_table_position;
            if relative_position >= file_table_position {
                break;
            }

            let (_upper_file_name, file_name) = self.read_file_name_data()?;
            file_name_table.insert(relative_position, file_name);
        }

        loop {
            let header_position = self.position.get() - file_name_table_position;
            if header_position >= directory_table_position {
                break;
            }
            let relative_position =
                self.position.get() - file_name_table_position - file_table_position;

            let file_entry = self.read_file_entry()?;
            file_table.insert(relative_position, file_entry);
        }

        loop {
            let header_position = self.position.get() - file_name_table_position;
            if header_position >= u64::from(file_header_size) {
                break;
            }
            let relative_position =
                self.position.get() - file_name_table_position - directory_table_position;

            let directory_entry = self.read_directory_entry()?;
            directory_table.insert(relative_position, directory_entry);
        }

        self.header_data = Some(ArchiveHeaderData {
            data_position,
            file_name_table,
            file_table,
            directory_table,
        });

        Ok(())
    }

    /// Get the root directory
    pub fn get_root_dir(&self) -> Result<Option<&DirectoryEntry>, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        Ok(header_data.directory_table.get(&0))
    }

    /// Get the parent dir for a dir, if it exists.
    pub fn get_parent_dir(
        &self,
        directory_entry: &DirectoryEntry,
    ) -> Result<Option<&DirectoryEntry>, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        let parent_directory_position = match directory_entry.parent_directory_position {
            Some(parent_directory_position) => parent_directory_position,
            None => return Ok(None),
        };

        let directory_entry = header_data
            .directory_table
            .get(&parent_directory_position)
            .ok_or(Error::InvalidDirectoryPosition)?;

        Ok(Some(directory_entry))
    }

    /// Get a file reader.
    pub fn get_file_reader(&self, file_entry: &FileEntry) -> Result<FileReader<R>, Error> {
        let header_data = self.header_data.as_ref().ok_or(Error::HeaderNotRead)?;

        if file_entry.is_dir() {
            return Err(Error::NotAFile);
        }

        let mut reader = self
            .reader
            .try_borrow_mut()
            .map_err(|_| Error::ReaderBusy)?;

        let new_position = reader.seek(SeekFrom::Start(
            header_data.data_position + file_entry.data_position,
        ))?;

        self.position.set(new_position);
        match file_entry.compressed_data_size {
            Some(compressed_size) => {
                // Yes, we secretly buffer compressed files.
                // This is because the compression protocol seems to refer to byte sequences from the output,
                // which is difficult to wrap with a Read interface.
                //
                // We could choose use the compressed data via the Read interface,
                // but that wouldn't save too much data and add more complexity,
                // as we would still need to buffer the entire output in memory.
                let mut input = Vec::with_capacity(usize::try_from(compressed_size).unwrap());
                reader
                    .by_ref()
                    .take(compressed_size)
                    .read_to_end(&mut input)?;
                key_xor(file_entry.data_size, self.key, &mut input);

                let output = decompress_file_data(&input, file_entry.data_size)
                    .ok_or(Error::DecompressionFailed)?;

                Ok(FileReader {
                    inner: FileReaderInner::Compressed(CompressedFileReaderInner {
                        file_data: std::io::Cursor::new(output),
                    }),
                })
            }
            None => {
                let reader = UncompressedFileReaderInner {
                    reader,
                    key: self.key,
                    offset: 0,
                    size: file_entry.data_size,
                };

                Ok(FileReader {
                    inner: FileReaderInner::Uncompressed(reader),
                })
            }
        }
    }
}

/// Data extracted from the header
#[derive(Debug)]
struct ArchiveHeaderData {
    data_position: u64,
    file_name_table: BTreeMap<u64, String>,
    file_table: BTreeMap<u64, FileEntry>,
    directory_table: BTreeMap<u64, DirectoryEntry>,
}

/// The header for a directory entry
#[derive(Debug)]
pub struct DirectoryEntry {
    directory_position: u64,
    parent_directory_position: Option<u64>,
    num_files: u64,
    file_head_position: u64,
}

impl DirectoryEntry {
    /// Get the number of files in this dir.
    pub fn num_files(&self) -> u64 {
        self.num_files
    }
}
