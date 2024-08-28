mod archive_reader;
mod key_string;

pub use self::archive_reader::ArchiveReader;
pub use self::key_string::KeyString;

const KEY_LEN: usize = 12;

/// A key
type Key = [u8; KEY_LEN];

const DEFAULT_KEY_STRING: KeyString = KeyString([
    0x38, 0x50, 0x40, 0x28, 0x72, 0x4F, 0x21, 0x70, 0x3B, 0x73, 0x35, 0x38,
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

    /// The header has already been read
    #[error("header already read")]
    HeaderAlreadyRead,

    /// The header hasn't been read
    #[error("header not read")]
    HeaderNotRead,

    /// A directory file index was invalid
    #[error("invalid directory file index")]
    InvalidDirectoryFileIndex,

    /// A file name position was invalid
    #[error("invalid file name position")]
    InvalidFileNamePosition,

    /// An object should have been a dir but was not
    #[error("not a dir")]
    NotADir,

    /// A directory position was invalid
    #[error("invalid directory position")]
    InvalidDirectoryPosition,

    /// An object should have been a file but was not
    #[error("not a file")]
    NotAFile,

    /// The reader is busy
    #[error("reader busy")]
    ReaderBusy,
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
