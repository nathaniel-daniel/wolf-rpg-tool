use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// A file entry
#[derive(Debug)]
pub struct FileEntry {
    pub(super) name_position: u64,
    pub(super) attributes: Attributes,
    pub(super) file_times: FileTimes,
    pub(super) data_position: u64,
    pub(super) data_size: u64,
    pub(super) compressed_data_size: Option<u64>,
}

impl FileEntry {
    /// Returns true if this is for a directory.
    pub fn is_dir(&self) -> bool {
        self.attributes.contains(Attributes::Directory)
    }

    /// Returns true if this is for a file.
    pub fn is_file(&self) -> bool {
        !self.is_dir()
    }

    /// Returns true if this is compressed.
    pub fn is_compressed(&self) -> bool {
        self.compressed_data_size.is_some()
    }

    /// Get the file size.
    pub fn size(&self) -> u64 {
        self.data_size
    }

    /// Get the compressed file size, if it is compressed.
    pub fn compressed_size(&self) -> Option<u64> {
        self.compressed_data_size
    }

    /// Get the file times.
    pub fn file_times(&self) -> FileTimes {
        self.file_times
    }

    /// Get the file attributes.
    pub fn get_attributes(&self) -> Attributes {
        self.attributes
    }
}

bitflags::bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct Attributes: u64 {
        const Directory = 0x0010;
        const Archive = 0x0020;
    }
}

const FILE_TIME_TO_UNIX_EPOCH_DIFF: u64 = 11_644_473_600_000_000;
const NANOS_PER_SEC: u64 = 1_000_000_000;

/// File times
#[derive(Debug, Copy, Clone)]
pub struct FileTimes {
    pub(super) created: u64,
    pub(super) accessed: u64,
    pub(super) modified: u64,
}

impl FileTimes {
    /// Get the time this was created.
    pub fn created(&self) -> Option<SystemTime> {
        file_time_to_system_time(self.created)
    }

    /// Get the time this was accessed.
    pub fn accessed(&self) -> Option<SystemTime> {
        file_time_to_system_time(self.accessed)
    }

    /// Get the time this was modified.
    pub fn modified(&self) -> Option<SystemTime> {
        file_time_to_system_time(self.modified)
    }

    /// Set the time this was created.
    ///
    /// Returns
    /// Returns None if the time could not be set
    pub fn set_created(&mut self, system_time: SystemTime) -> Option<()> {
        self.created = system_time_to_file_time(system_time)?;
        Some(())
    }

    /// Set the time this was accessed.
    ///
    /// Returns
    /// Returns None if the time could not be set
    pub fn set_accessed(&mut self, system_time: SystemTime) -> Option<()> {
        self.accessed = system_time_to_file_time(system_time)?;
        Some(())
    }

    /// Set the time this was modified.
    ///
    /// Returns
    /// Returns None if the time could not be set
    pub fn set_modified(&mut self, system_time: SystemTime) -> Option<()> {
        self.modified = system_time_to_file_time(system_time)?;
        Some(())
    }
}

fn file_time_to_system_time(file_time: u64) -> Option<SystemTime> {
    let file_time_nanos = file_time.checked_mul(100)?;
    let unix_epoch_nanos = file_time_nanos.checked_sub(FILE_TIME_TO_UNIX_EPOCH_DIFF)?;

    let seconds = unix_epoch_nanos.checked_div(NANOS_PER_SEC)?;
    let nanoseconds = u32::try_from(unix_epoch_nanos % NANOS_PER_SEC).unwrap();

    UNIX_EPOCH.checked_add(std::time::Duration::new(seconds, nanoseconds))
}

fn system_time_to_file_time(system_time: SystemTime) -> Option<u64> {
    let duration_since_epoch = system_time.duration_since(UNIX_EPOCH).ok()?;
    let unix_epoch_nanos = duration_since_epoch
        .as_secs()
        .checked_mul(NANOS_PER_SEC)?
        .checked_add(u64::from(duration_since_epoch.subsec_nanos()))?;

    let filetime_nanos = unix_epoch_nanos.checked_add(FILE_TIME_TO_UNIX_EPOCH_DIFF)?;
    let filetime_100_nanos = filetime_nanos.checked_div(100)?;

    Some(filetime_100_nanos)
}
