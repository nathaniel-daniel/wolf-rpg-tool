use super::key_xor;
use crate::Key;
use std::io::Read;

/// A reader for files
#[derive(Debug)]
pub struct FileReader<'a, R> {
    pub(super) inner: FileReaderInner<'a, R>,
}

impl<R> Read for FileReader<'_, R>
where
    R: Read,
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.inner {
            FileReaderInner::Uncompressed(reader) => reader.read(buffer),
            FileReaderInner::Compressed(reader) => reader.read(buffer),
        }
    }
}

#[derive(Debug)]
pub(super) enum FileReaderInner<'a, R> {
    Uncompressed(UncompressedFileReaderInner<'a, R>),
    Compressed(CompressedFileReaderInner),
}

#[derive(Debug)]
pub(super) struct UncompressedFileReaderInner<'a, R> {
    pub(super) reader: std::cell::RefMut<'a, R>,
    pub(super) key: Key,
    pub(super) offset: u64,
    pub(super) size: u64,
}

impl<R> Read for UncompressedFileReaderInner<'_, R>
where
    R: Read,
{
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.size {
            return Ok(0);
        }

        let limit = usize::try_from(self.size - self.offset).unwrap();
        let limit = std::cmp::min(limit, buffer.len());

        let n = self.reader.read(&mut buffer[..limit])?;

        let buffer = &mut buffer[..n];
        // I have no idea why the position is offset + size, but it works...
        key_xor(self.offset + self.size, self.key, buffer);

        let buffer_len_u64 = u64::try_from(buffer.len()).unwrap();
        self.offset += buffer_len_u64;

        Ok(n)
    }
}

#[derive(Debug)]
pub(super) struct CompressedFileReaderInner {
    pub(super) file_data: std::io::Cursor<Vec<u8>>,
}

impl Read for CompressedFileReaderInner {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.file_data.read(buffer)
    }
}

#[allow(clippy::get_first)]
pub(super) fn decompress_file_data(mut input: &[u8], size: u64) -> Option<Vec<u8>> {
    const MIN_COMPRESS: u16 = 4;

    if input.len() < 9 {
        return None;
    }
    let dest_size = u32::from_le_bytes(input[..4].try_into().unwrap());
    let src_size = u32::from_le_bytes(input[4..8].try_into().unwrap());
    let key_code = input[8];

    if u64::from(dest_size) != size {
        return None;
    }
    if u32::try_from(input.len()).unwrap() != src_size {
        return None;
    }
    input = &input[9..];

    let mut output = Vec::with_capacity(usize::try_from(size).unwrap());
    while !input.is_empty() {
        let input_0 = *input.get(0)?;
        if input_0 != key_code {
            output.push(input_0);
            input = &input[1..];
            continue;
        }

        let input_1 = *input.get(1)?;
        if input_1 == key_code {
            output.push(key_code);
            input = &input[2..];
            continue;
        }

        let mut code = u16::from(input_1);
        if code > u16::from(key_code) {
            code -= 1;
        }
        input = &input[2..];

        let mut run_len = code >> 3;
        if code & (0x1 << 2) != 0 {
            run_len |= u16::from(*input.get(0)?) << 5;
            input = &input[1..];
        }
        run_len += MIN_COMPRESS;

        let index_size = code & 0x3;
        let mut index = match index_size {
            0 => {
                let index = *input.get(0)?;
                input = &input[1..];
                u32::from(index)
            }
            1 => {
                let bytes = input.get(..2)?;
                let index = u16::from_le_bytes(bytes.try_into().unwrap());
                input = &input[2..];
                u32::from(index)
            }
            2 => {
                let low = input.get(..2)?;
                let low = u16::from_le_bytes(low.try_into().unwrap());
                let low = u32::from(low);

                let high = u32::from(*input.get(2)?);

                let index = low | high << 16;
                input = &input[3..];

                index
            }
            _ => {
                return None;
            }
        };
        index += 1;

        let mut run_len = u32::from(run_len);
        if index < run_len {
            let mut num = index;
            while run_len > num {
                let old_output_len = output.len();
                let num_usize = usize::try_from(num).ok()?;

                output.resize(old_output_len + num_usize, 0);
                let start = old_output_len - num_usize;
                output.copy_within(start..(start + num_usize), old_output_len);

                run_len -= num;
                num += num;
            }

            if run_len != 0 {
                let old_output_len = output.len();
                let run_len_usize = usize::try_from(run_len).ok()?;
                let num_usize = usize::try_from(num).ok()?;

                output.resize(old_output_len + run_len_usize, 0);
                let start = old_output_len - num_usize;
                output.copy_within(start..(start + run_len_usize), old_output_len);
            }
        } else {
            let old_output_len = output.len();
            let run_len_usize = usize::try_from(run_len).ok()?;
            let index_usize = usize::try_from(index).ok()?;

            output.resize(old_output_len + run_len_usize, 0);
            let start = old_output_len - index_usize;
            output.copy_within(start..(start + run_len_usize), old_output_len);
        }
    }

    Some(output)
}
