use crate::create_key;
use crate::Error;
use crate::Key;
use crate::DEFAULT_KEY_STRING;
use encoding_rs::SHIFT_JIS;
use std::collections::BTreeMap;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

/// A reader for an archive.
#[derive(Debug)]
pub struct ArchiveReader<R> {
    reader: R,
    position: u64,
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
            reader,
            position: 0,
            key,

            encoding: SHIFT_JIS,
            header_data: None,
        }
    }
}

impl<R> ArchiveReader<R>
where
    R: Read + Seek,
{
    /// Read encoded bytes to a buffer.
    fn read_encoded(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        let position_usize = usize::try_from(self.position).unwrap();
        let key_len = self.key.len();

        self.reader.read_exact(buffer)?;
        for (i, out_byte) in buffer.iter_mut().enumerate() {
            let key_byte = self.key[(position_usize + i) % key_len];

            *out_byte ^= key_byte;
        }
        self.position += u64::try_from(buffer.len()).unwrap();

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

    /// Read a file header.
    fn read_file_header(&mut self) -> Result<FileHeader, Error> {
        let name_position = self.read_encoded_u64()?;
        let attributes = self.read_encoded_u64()?;
        let created = self.read_encoded_u64()?;
        let read = self.read_encoded_u64()?;
        let updated = self.read_encoded_u64()?;
        let data_position = self.read_encoded_u64()?;
        let data_size = self.read_encoded_u64()?;
        let compressed_data_size = self.read_encoded_u64()?;

        Ok(FileHeader {
            name_position,
            attributes,
            time: FileTimes {
                created,
                read,
                updated,
            },
            data_position,
            data_size,
            compressed_data_size,
        })
    }

    /// Read a directory header
    fn read_directory_header(&mut self) -> Result<DirectoryHeader, Error> {
        let directory_position = self.read_encoded_u64()?;
        let parent_directory_position = self.read_encoded_u64()?;
        let num_files = self.read_encoded_u64()?;
        let file_head_position = self.read_encoded_u64()?;

        Ok(DirectoryHeader {
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

        dbg!(data_position);

        self.position = self
            .reader
            .seek(SeekFrom::Start(file_name_table_position))?;

        let mut file_name_table = BTreeMap::new();
        let mut file_table = BTreeMap::new();
        let mut directory_table = BTreeMap::new();

        loop {
            let relative_position = self.position - file_name_table_position;
            if relative_position >= file_table_position {
                break;
            }

            let (_upper_file_name, file_name) = self.read_file_name_data()?;
            file_name_table.insert(relative_position, file_name);
        }

        loop {
            let header_position = self.position - file_name_table_position;
            if header_position >= directory_table_position {
                break;
            }
            let relative_position = self.position - file_name_table_position - file_table_position;

            let file_header = self.read_file_header()?;
            file_table.insert(relative_position, file_header);
        }

        loop {
            let header_position = self.position - file_name_table_position;
            if header_position >= u64::from(file_header_size) {
                break;
            }
            let relative_position =
                self.position - file_name_table_position - directory_table_position;

            let directory_header = self.read_directory_header()?;
            directory_table.insert(relative_position, directory_header);
        }

        self.header_data = Some(ArchiveHeaderData {
            file_name_table,
            file_table,
            directory_table,
        });

        Ok(())
    }
}

/// Data extracted from the header
#[derive(Debug)]
struct ArchiveHeaderData {
    file_name_table: BTreeMap<u64, String>,
    file_table: BTreeMap<u64, FileHeader>,
    directory_table: BTreeMap<u64, DirectoryHeader>,
}

/// The header for a file entry
#[derive(Debug)]
struct FileHeader {
    name_position: u64,
    attributes: u64,
    time: FileTimes,
    data_position: u64,
    data_size: u64,
    compressed_data_size: u64,
}

/// File times
#[derive(Debug)]
struct FileTimes {
    created: u64,
    read: u64,
    updated: u64,
}

/// The header for a directory entry
#[derive(Debug)]
pub struct DirectoryHeader {
    directory_position: u64,
    parent_directory_position: u64,
    num_files: u64,
    file_head_position: u64,
}
