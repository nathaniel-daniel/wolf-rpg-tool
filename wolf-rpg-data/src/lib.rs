mod key_string;

pub use self::key_string::KeyString;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

const KEY_LEN: usize = 12;

/// A key
type Key = [u8; KEY_LEN];

const DEFAULT_KEY_STRING: KeyString = KeyString([
    0x38, 0x50, 0x40, 0x28, 0x72, 0x4f, 0x21, 0x70, 0x3b, 0x73, 0x35, 0x38,
]);

/// The error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An io error
    #[error("io error")]
    Io(#[from] std::io::Error),

    /// Invalid Magic Number
    #[error("invalid magic number \"{magic:?}\"")]
    InvalidMagic { magic: [u8; 2] },

    /// Invalid version
    #[error("invalid version {version}")]
    InvalidVersion { version: u16 },

    /// Invalid file name parity
    #[error("invalid file name parity")]
    InvalidFileNameParity,

    /// Invalid file name
    #[error("invalid file name")]
    InvalidFileName,

    /// Unknown code page
    #[error("Unknown code page {code_page}")]
    UnknownCodePage { code_page: u64 },
}

/// Create a key from a key string
fn create_key(key_string: KeyString) -> [u8; KEY_LEN] {
    let mut key = key_string.0;
    key[0] = !key[0];
    key[1] = (key[1] >> 4) | (key[1] << 4);
    key[2] ^= 0x8a;
    key[3] = !((key[3] >> 4) | (key[3] << 4));
    key[4] = !key[4];
    key[5] ^= 0xac;
    key[6] = !key[6];
    key[7] = !((key[7] >> 3) | (key[7] << 5));
    key[8] = (key[8] >> 5) | (key[8] << 3);
    key[9] ^= 0x7f;
    key[10] = ((key[10] >> 4) | (key[10] << 4)) ^ 0xd6;
    key[11] ^= 0xcc;

    key
}

/// A reader for an archive.
#[derive(Debug)]
pub struct ArchiveReader<R> {
    reader: R,
    position: u64,
    key: Key,
    
    encoding: &'static encoding_rs::Encoding,
    // file_name_entries: BTreeMap<u64, ()>,
    // file_entries
    // dir_entries
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
            
            encoding: encoding_rs::SHIFT_JIS,
            // file_name_entries: BTreeMap::new(),
            // file_entries
            // dir_entries
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
    pub fn read_file_name_data(&mut self) -> Result<(String, String), Error> {
        let len = self.read_encoded_u16()?;
        let parity = self.read_encoded_u16()?;

        if len == 0 {
            if parity != 0 {
                return Err(Error::InvalidFileNameParity);
            }

            return Ok((String::new(), String::new()));
        }

        let mut bytes_upper = vec![0; (len * 4).into()];
        self.read_encoded(&mut bytes_upper)?;
        let bytes_upper_parity = bytes_upper
            .iter()
            .fold(0_u16, |acc, byte| acc.wrapping_add((*byte).into()));
        if bytes_upper_parity != parity {
            return Err(Error::InvalidFileNameParity);
        }

        let mut bytes = vec![0; (len * 4).into()];
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

    /// Read the header.
    pub fn read_header(&mut self) -> Result<(), Error> {
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
        let directory_table_start = self.read_encoded_u64()?;
        let code_page = self.read_encoded_u64()?;

        self.encoding = match code_page {
            932 => encoding_rs::SHIFT_JIS,
            _ => {
                return Err(Error::UnknownCodePage { code_page });
            }
        };

        dbg!(
            file_header_size,
            data_position,
            file_table_position,
            directory_table_start,
        );

        self.position = self
            .reader
            .seek(SeekFrom::Start(file_name_table_position))?;
            
        loop {
            let relative_position = self.position - file_name_table_position;
            if relative_position >= file_table_position {
                break;
            }
            
            let (upper_file_name, file_name) = self.read_file_name_data()?;
        }
        
        loop {
            let relative_position = self.position - file_name_table_position;
            if relative_position >= directory_table_start {
                break;
            }
            
            let name_position = self.read_encoded_u64()?;
            let attributes = self.read_encoded_u64()?;
            let time = self.read_encoded_u64()?;
            let data_position = self.read_encoded_u64()?;
            let data_size = self.read_encoded_u64()?;
            let compressed_data_size = self.read_encoded_u64()?;
            
            dbg!(name_position, attributes,time, data_position,data_size, compressed_data_size);
        }
        
        for _ in 0..11 {
            let relative_position = self.position - file_name_table_position;
            if relative_position >= u64::from(file_header_size) {
                break;
            }
            
            let directory_position = self.read_encoded_u64()?;
            let parent_directory_position = self.read_encoded_u64()?;
            let num_files = self.read_encoded_u64()?;
            let file_head_position = self.read_encoded_u64()?;
            
            dbg!(directory_position, parent_directory_position, num_files, file_head_position);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const DEFAULT_KEY: Key = [199, 5, 202, 125, 141, 227, 222, 241, 217, 12, 133, 244];

    #[test]
    fn create_key_works() {
        let key = create_key(DEFAULT_KEY_STRING);
        dbg!(key);

        assert!(key == DEFAULT_KEY);
    }
}
