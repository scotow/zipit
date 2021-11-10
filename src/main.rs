use std::io::Cursor;
use std::mem::size_of;

#[cfg(feature = "chrono-datetime")]
use chrono::{Datelike, DateTime, Local, Timelike, TimeZone};
use crc32fast::Hasher;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Error as TokioIoError};

#[tokio::main]
async fn main() {
    let file = File::create("archive.zip").await.unwrap();
    let mut archive = Archive::new(file);
    archive.append(
        "file1.txt".to_owned(),
        FileDateTime::now(),
        &mut Cursor::new(b"hello\n".to_vec()),
    ).await.unwrap();
    archive.append(
        "file2.txt".to_owned(),
        FileDateTime::now(),
        &mut Cursor::new(b"world\n".to_vec()),
    ).await.unwrap();
    archive.finalize().await.unwrap();
}

struct FileInfo {
    name: String,
    size: usize,
    crc: u32,
    offset: usize,
    datetime: (u16, u16),
}

pub enum FileDateTime {
    Zero,
    Custom(u16, u16, u16, u16, u16, u16),
}

impl FileDateTime {
    fn tuple(&self) -> (u16, u16, u16, u16, u16, u16) {
        match self {
            FileDateTime::Zero => Default::default(),
            &FileDateTime::Custom(ye, mo, da, ho, mi, se) => (ye, mo, da, ho, mi, se),
        }
    }

    fn ms_dos(&self) -> (u16, u16) {
        let (year, month, day, hour, min, sec) = self.tuple();
        (
            day | month << 5 | year.saturating_sub(1980) << 9,
            sec / 2 | min << 5 | hour << 11,
        )
    }
}

#[cfg(feature = "chrono-datetime")]
impl FileDateTime {
    pub fn now() -> Self {
        Self::from_chrono_datetime(Local::now())
    }

    pub fn from_chrono_datetime<Tz: TimeZone>(datetime: DateTime<Tz>) -> Self {
        Self::Custom(
            datetime.year() as u16,
            datetime.month() as u16,
            datetime.day() as u16,
            datetime.hour() as u16,
            datetime.minute() as u16,
            datetime.second() as u16,
        )
    }
}

pub struct Archive<W> {
    sink: W,
    files_info: Vec<FileInfo>,
    written: usize,
}

impl<W: AsyncWrite + Unpin> Archive<W> {
    pub fn new(sink: W) -> Self {
        Self {
            sink,
            files_info: Vec::new(),
            written: 0,
        }
    }

    pub async fn append<R: AsyncRead + Unpin>(&mut self, name: String, datetime: FileDateTime, reader: &mut R) -> Result<(), TokioIoError> {
        let (date, time) = datetime.ms_dos();
        let offset = self.written;
        let mut header = Vec::with_capacity(7 * size_of::<u16>() + 4 * size_of::<u32>() + name.len());

        header.extend_from_slice(&0x04034b50u32.to_le_bytes()); // Local file header signature.
        header.extend_from_slice(&10u16.to_le_bytes()); // Version needed to extract.
        header.extend_from_slice(&0x08u16.to_le_bytes()); // General purpose flag.
        header.extend_from_slice(&0u16.to_le_bytes()); // Compression method (store).
        header.extend_from_slice(&time.to_le_bytes()); // Modification time.
        header.extend_from_slice(&date.to_le_bytes()); // Modification date.
        header.extend_from_slice(&0u32.to_le_bytes()); // Temporary CRC32.
        header.extend_from_slice(&0u32.to_le_bytes()); // Temporary compressed size.
        header.extend_from_slice(&0u32.to_le_bytes()); // Temporary uncompressed size.
        header.extend_from_slice(&(name.len() as u16).to_le_bytes()); // Filename length.
        header.extend_from_slice(&0u16.to_le_bytes()); // Extra field length.
        header.extend_from_slice(name.as_bytes()); // Filename.
        self.sink.write_all(&header).await?;
        self.written += header.len();

        let mut total_read = 0;
        let mut hasher = Hasher::new();
        let mut buf = vec![0; 4096];
        loop {
            let read = reader.read(&mut buf).await?;
            if read == 0 {
                break;
            }

            total_read += read;
            hasher.update(&buf[..read]);
            self.sink.write_all(&buf[..read]).await?; // Payload chunk.
        }
        let crc = hasher.finalize();
        self.written += total_read;

        let mut descriptor = Vec::with_capacity(4 * size_of::<u32>());
        descriptor.extend_from_slice(&0x08074b50u32.to_le_bytes()); // Data descriptor signature.
        descriptor.extend_from_slice(&crc.to_le_bytes()); // CRC32.
        descriptor.extend_from_slice(&(total_read as u32).to_le_bytes()); // Compressed size.
        descriptor.extend_from_slice(&(total_read as u32).to_le_bytes()); // Uncompressed size.
        self.sink.write_all(&descriptor).await?;
        self.written += descriptor.len();

        self.files_info.push(FileInfo {
            name,
            size: total_read,
            crc,
            offset,
            datetime: (date, time),
        });

        Ok(())
    }

    pub async fn finalize(mut self) -> Result<W, TokioIoError> {
        let mut central_directory_size = 0;
        for file_info in &self.files_info {
            let entry_size = 11 * size_of::<u16>() +
                6 * size_of::<u32>() +
                file_info.name.len();
            central_directory_size += entry_size;

            let mut entry = Vec::with_capacity(entry_size);
            entry.extend_from_slice(&0x02014b50u32.to_le_bytes()); // Central directory entry signature.
            entry.extend_from_slice(&0x031eu16.to_le_bytes()); // Version made by.
            entry.extend_from_slice(&10u16.to_le_bytes()); // Version needed to extract.
            entry.extend_from_slice(&0x08u16.to_le_bytes()); // General purpose flag.
            entry.extend_from_slice(&0u16.to_le_bytes()); // Compression method (store).
            entry.extend_from_slice(&file_info.datetime.1.to_le_bytes()); // Modification time.
            entry.extend_from_slice(&file_info.datetime.0.to_le_bytes()); // Modification date.
            entry.extend_from_slice(&file_info.crc.to_le_bytes()); // CRC32.
            entry.extend_from_slice(&(file_info.size as u32).to_le_bytes()); // Compressed size.
            entry.extend_from_slice(&(file_info.size as u32).to_le_bytes()); // Uncompressed size.
            entry.extend_from_slice(&(file_info.name.len() as u16).to_le_bytes()); // Filename length.
            entry.extend_from_slice(&0u16.to_le_bytes()); // Extra field length.
            entry.extend_from_slice(&0u16.to_le_bytes()); // File comment length.
            entry.extend_from_slice(&0u16.to_le_bytes()); // File's Disk number.
            entry.extend_from_slice(&0u16.to_le_bytes()); // Internal file attributes.
            entry.extend_from_slice(&((0o100000u32 | 0o0000400 | 0o0000200 | 0o0000040 | 0o0000004) << 16).to_le_bytes()); // External file attributes (regular file rw-r-r-).
            entry.extend_from_slice(&(file_info.offset as u32).to_le_bytes()); // Offset from start of file to local file header.
            entry.extend_from_slice(file_info.name.as_bytes()); // Filename.
            self.sink.write_all(&entry).await?;
        }

        let mut end_of_central_directory = Vec::with_capacity(
            5 * size_of::<u16>() +
                3 * size_of::<u32>()
        );
        end_of_central_directory.extend_from_slice(&0x06054b50u32.to_le_bytes()); // End of central directory signature.
        end_of_central_directory.extend_from_slice(&0u16.to_le_bytes()); // Number of this disk.
        end_of_central_directory.extend_from_slice(&0u16.to_le_bytes()); // Number of the disk where central directory starts.
        end_of_central_directory.extend_from_slice(&(self.files_info.len() as u16).to_le_bytes()); // Number of central directory records on this disk.
        end_of_central_directory.extend_from_slice(&(self.files_info.len() as u16).to_le_bytes()); // Total number of central directory records.
        end_of_central_directory.extend_from_slice(&(central_directory_size as u32).to_le_bytes()); // Size of central directory.
        end_of_central_directory.extend_from_slice(&(self.written as u32).to_le_bytes()); // Offset from start of file to central directory.
        end_of_central_directory.extend_from_slice(&0u16.to_le_bytes()); // Comment length.
        self.sink.write_all(&end_of_central_directory).await?;

        Ok(self.sink)
    }
}
