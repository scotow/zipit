//! ## Features
//!
//! - Stream on the fly an archive from multiple AsyncRead objects.
//! - Single read / seek free implementation (the CRC and file size are calculated while streaming and are sent afterwards).
//! - Archive size pre-calculation (useful if you want to set the `Content-Length` before streaming).
//! - [futures](https://docs.rs/futures/latest/futures/) and [tokio](https://docs.rs/tokio/latest/tokio/io/index.html) `AsyncRead` / `AsyncWrite` compatible. Enable either the `futures-async-io` or the `tokio-async-io` feature accordingly.
//!
//! ## Limitations
//!
//! - No compression (stored method only).
//! - Only files (no directories).
//! - No customizable external file attributes.
//!
//! ## Examples
//!
//! ### [File system](examples/fs.rs)
//!
//! Write a zip archive to the file system using [`tokio::fs::File`](https://docs.rs/tokio/1.13.0/tokio/fs/struct.File.html):
//!
//! ```
//! use std::io::Cursor;
//! use tokio::fs::File;
//! use zipit::{Archive, FileDateTime};
//!
//! #[tokio::main]
//! async fn main() {
//!     let file = File::from_std(tempfile::tempfile().unwrap());
//!     let mut archive = Archive::new(file);
//!     archive.append(
//!         "file1.txt".to_owned(),
//!         FileDateTime::now(),
//!         &mut Cursor::new(b"hello\n".to_vec()),
//!     ).await.unwrap();
//!     archive.append(
//!         "file2.txt".to_owned(),
//!         FileDateTime::now(),
//!         &mut Cursor::new(b"world\n".to_vec()),
//!     ).await.unwrap();
//!     archive.finalize().await.unwrap();
//! }
//! ```
//!
//! ### [Hyper](examples/hyper.rs)
//!
//! Stream a zip archive as a [`hyper`](https://docs.rs/hyper/0.14.14/hyper/) response:
//!
//! ```
//! use std::io::Cursor;
//! use hyper::{header, Body, Request, Response, Server, StatusCode};
//! use tokio::io::duplex;
//! use tokio_util::io::ReaderStream;
//! use zipit::{archive_size, Archive, FileDateTime};
//!
//! async fn zip_archive(_req: Request<Body>) -> Result<Response<Body>, hyper::http::Error> {
//!     let (filename_1, mut fd_1) = (String::from("file1.txt"), Cursor::new(b"hello\n".to_vec()));
//!     let (filename_2, mut fd_2) = (String::from("file2.txt"), Cursor::new(b"world\n".to_vec()));
//!     let archive_size = archive_size([
//!         (filename_1.as_ref(), fd_1.get_ref().len()),
//!         (filename_2.as_ref(), fd_2.get_ref().len()),
//!     ]);
//!
//!     let (w, r) = duplex(4096);
//!     tokio::spawn(async move {
//!         let mut archive = Archive::new(w);
//!         archive
//!             .append(
//!                 filename_1,
//!                 FileDateTime::now(),
//!                 &mut fd_1,
//!             )
//!             .await
//!             .unwrap();
//!         archive
//!             .append(
//!                 filename_2,
//!                 FileDateTime::now(),
//!                 &mut fd_2,
//!             )
//!             .await
//!             .unwrap();
//!         archive.finalize().await.unwrap();
//!     });
//!
//!     Response::builder()
//!         .status(StatusCode::OK)
//!         .header(header::CONTENT_LENGTH, archive_size)
//!         .header(header::CONTENT_TYPE, "application/zip")
//!         .body(Body::wrap_stream(ReaderStream::new(r)))
//! }
//! ```

#![deny(dead_code, unsafe_code, missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
use std::io::Error as IoError;
use std::mem::size_of;

#[cfg(feature = "chrono-datetime")]
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
use crc32fast::Hasher;

#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
#[derive(Debug)]
struct FileInfo {
    name: String,
    size: usize,
    crc: u32,
    offset: usize,
    datetime: (u16, u16),
}

/// The (timezone-less) date and time that will be written in the archive alongside the file.
///
/// Use `FileDateTime::Zero` if the date and time are insignificant. This will set the value to 0 which is 1980, January 1th, 12AM.  
/// Use `FileDateTime::Custom` if you need to set a custom date and time.  
/// Use `FileDateTime::now()` if you want to use the current date and time (`chrono-datetime` feature required).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FileDateTime {
    /// 1980, January 1th, 12AM.
    Zero,
    /// (year, month, day, hour, minute, second)
    Custom {
        /// Year.
        year: u16,
        /// Month.
        month: u16,
        /// Day.
        day: u16,
        /// Hour (24 format).
        hour: u16,
        /// Minute.
        minute: u16,
        /// Second.
        second: u16,
    },
}

#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
impl FileDateTime {
    fn tuple(&self) -> (u16, u16, u16, u16, u16, u16) {
        match self {
            FileDateTime::Zero => Default::default(),
            &FileDateTime::Custom {
                year,
                month,
                day,
                hour,
                minute,
                second,
            } => (year, month, day, hour, minute, second),
        }
    }

    fn ms_dos(&self) -> (u16, u16) {
        let (year, month, day, hour, min, sec) = self.tuple();
        (
            day | month << 5 | year.saturating_sub(1980) << 9,
            (sec / 2) | min << 5 | hour << 11,
        )
    }
}

#[cfg(feature = "chrono-datetime")]
impl FileDateTime {
    /// Use the local date and time of the system.
    pub fn now() -> Self {
        Self::from_chrono_datetime(Local::now())
    }

    /// Use a custom date and time.
    pub fn from_chrono_datetime<Tz: TimeZone>(datetime: DateTime<Tz>) -> Self {
        Self::Custom {
            year: datetime.year() as u16,
            month: datetime.month() as u16,
            day: datetime.day() as u16,
            hour: datetime.hour() as u16,
            minute: datetime.minute() as u16,
            second: datetime.second() as u16,
        }
    }
}

#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
macro_rules! header {
    [$capacity:expr; $($elem:expr),*$(,)?] => {
        {
            let mut header = Vec::with_capacity($capacity);
            $(
                header.extend_from_slice(&$elem.to_le_bytes());
            )*
            header
        }
    };
}

const FILE_HEADER_BASE_SIZE: usize = 7 * size_of::<u16>() + 4 * size_of::<u32>();
const DESCRIPTOR_SIZE: usize = 4 * size_of::<u32>();
const CENTRAL_DIRECTORY_ENTRY_BASE_SIZE: usize = 11 * size_of::<u16>() + 6 * size_of::<u32>();
const END_OF_CENTRAL_DIRECTORY_SIZE: usize = 5 * size_of::<u16>() + 3 * size_of::<u32>();

/// A streamed zip archive.
///
/// Create an archive using the `new` function and a `AsyncWrite`. Then, append files one by one using the `append` function. When finished, use the `finalize` function.
///
/// ## Example
///
/// ```no_run
/// use std::io::Cursor;
/// use zipit::{Archive, FileDateTime};
///
/// #[tokio::main]
/// async fn main() {
///     let mut archive = Archive::new(Vec::new());
///     archive.append(
///         "file1.txt".to_owned(),
///         FileDateTime::now(),
///         &mut Cursor::new(b"hello\n".to_vec()),
///     ).await.unwrap();
///     archive.append(
///         "file2.txt".to_owned(),
///         FileDateTime::now(),
///         &mut Cursor::new(b"world\n".to_vec()),
///     ).await.unwrap();
///     let data = archive.finalize().await.unwrap();
///     println!("{:?}", data);
/// }
/// ```
#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
#[derive(Debug)]
pub struct Archive<W> {
    sink: W,
    files_info: Vec<FileInfo>,
    written: usize,
}

#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
macro_rules! impl_methods {
    (
        $(#[$($attrss:tt)*])*,
        $w:path, $r:path,
        $we:path, $re: path,
        $fa:tt, $ff:tt,
    ) => {
        impl<W> Archive<W> {
            /// Append a new file to the archive using the provided name, date/time and `AsyncRead` object.
            /// Filename must be valid UTF-8. Some (very) old zip utilities might mess up filenames during extraction if they contain non-ascii characters.
            /// File's payload is not compressed and is given `rw-r--r--` permissions.
            ///
            /// # Error
            ///
            /// This function will forward any error found while trying to read from the file stream or while writing to the underlying sink.
            $(#[$($attrss)*])*
            pub async fn $fa<R>(
                &mut self,
                name: String,
                datetime: FileDateTime,
                reader: &mut R,
            ) -> Result<(), IoError> where W: $w + Unpin, R: $r + Unpin {
                use $we;
                use $re;

                let (date, time) = datetime.ms_dos();
                let offset = self.written;
                let mut header = header![
                    FILE_HEADER_BASE_SIZE + name.len();
                    0x04034b50u32,          // Local file header signature.
                    10u16,                  // Version needed to extract.
                    1u16 << 3 | 1 << 11,    // General purpose flag (temporary crc and sizes + UTF-8 filename).
                    0u16,                   // Compression method (store).
                    time,                   // Modification time.
                    date,                   // Modification date.
                    0u32,                   // Temporary CRC32.
                    0u32,                   // Temporary compressed size.
                    0u32,                   // Temporary uncompressed size.
                    name.len() as u16,      // Filename length.
                    0u16,                   // Extra field length.
                ];
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

                let descriptor = header![
                    DESCRIPTOR_SIZE;
                    0x08074b50u32,      // Data descriptor signature.
                    crc,                // CRC32.
                    total_read as u32,  // Compressed size.
                    total_read as u32,  // Uncompressed size.
                ];
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

            /// Finalize the archive by writing the necessary metadata to the end of the archive.
            ///
            /// # Error
            ///
            /// This function will forward any error found while writing to the underlying sink.
            $(#[$($attrss)*])*
            pub async fn $ff(mut self) -> Result<W, IoError> where W: $w + Unpin {
                use $we;

                let mut central_directory_size = 0;
                for file_info in &self.files_info {
                    let mut entry = header![
                        CENTRAL_DIRECTORY_ENTRY_BASE_SIZE + file_info.name.len();
                        0x02014b50u32,                  // Central directory entry signature.
                        0x031eu16,                      // Version made by.
                        10u16,                          // Version needed to extract.
                        1u16 << 3 | 1 << 11,            // General purpose flag (temporary crc and sizes + UTF-8 filename).
                        0u16,                           // Compression method (store).
                        file_info.datetime.1,           // Modification time.
                        file_info.datetime.0,           // Modification date.
                        file_info.crc,                  // CRC32.
                        file_info.size as u32,          // Compressed size.
                        file_info.size as u32,          // Uncompressed size.
                        file_info.name.len() as u16,    // Filename length.
                        0u16,                           // Extra field length.
                        0u16,                           // File comment length.
                        0u16,                           // File's Disk number.
                        0u16,                           // Internal file attributes.
                        (0o100000u32 | 0o0000400 | 0o0000200 | 0o0000040 | 0o0000004) << 16, // External file attributes (regular file / rw-r--r--).
                        file_info.offset as u32,        // Offset from start of file to local file header.
                    ];
                    entry.extend_from_slice(file_info.name.as_bytes()); // Filename.
                    self.sink.write_all(&entry).await?;
                    central_directory_size += entry.len();
                }

                let end_of_central_directory = header![
                    END_OF_CENTRAL_DIRECTORY_SIZE;
                    0x06054b50u32,                  // End of central directory signature.
                    0u16,                           // Number of this disk.
                    0u16,                           // Number of the disk where central directory starts.
                    self.files_info.len() as u16,   // Number of central directory records on this disk.
                    self.files_info.len() as u16,   // Total number of central directory records.
                    central_directory_size as u32,  // Size of central directory.
                    self.written as u32,            // Offset from start of file to central directory.
                    0u16,                           // Comment length.
                ];
                self.sink.write_all(&end_of_central_directory).await?;

                Ok(self.sink)
            }
        }
    };
}

#[cfg(all(feature = "futures-async-io", feature = "tokio-async-io"))]
impl_methods!(
    #[cfg(all(feature = "futures-async-io", feature = "tokio-async-io"))],
    futures_util::AsyncWrite, futures_util::AsyncRead,
    futures_util::AsyncWriteExt, futures_util::AsyncReadExt,
    futures_append, futures_finalize,
);
#[cfg(all(feature = "futures-async-io", feature = "tokio-async-io"))]
impl_methods!(
    #[cfg(all(feature = "futures-async-io", feature = "tokio-async-io"))],
    tokio::io::AsyncWrite, tokio::io::AsyncRead,
    tokio::io::AsyncWriteExt, tokio::io::AsyncReadExt,
    tokio_append, tokio_finalize,
);

#[cfg(all(feature = "futures-async-io", not(feature = "tokio-async-io")))]
impl_methods!(
    #[cfg(all(feature = "futures-async-io", not(feature = "tokio-async-io")))],
    futures_util::AsyncWrite, futures_util::AsyncRead,
    futures_util::AsyncWriteExt, futures_util::AsyncReadExt,
    append, finalize,
);

#[cfg(all(not(feature = "futures-async-io"), feature = "tokio-async-io"))]
impl_methods!(
    #[cfg(all(not(feature = "futures-async-io"), feature = "tokio-async-io"))],
    tokio::io::AsyncWrite, tokio::io::AsyncRead,
    tokio::io::AsyncWriteExt, tokio::io::AsyncReadExt,
    append, finalize,
);

#[cfg(any(feature = "futures-async-io", feature = "tokio-async-io"))]
impl<W> Archive<W> {
    /// Create a new zip archive, using the underlying `AsyncWrite` to write files' header and payload.
    pub fn new(sink: W) -> Self {
        Self {
            sink,
            files_info: Vec::new(),
            written: 0,
        }
    }
}

/// Calculate the size that an archive could be based on the names and sizes of files.
///
/// ## Example
///
/// ```
/// assert_eq!(
///     zipit::archive_size([
///         ("file1.txt", b"hello\n".len()),
///         ("file2.txt", b"world\n".len()),
///     ]),
///     254,
/// );
/// ```
pub fn archive_size<'a, I: IntoIterator<Item = (&'a str, usize)>>(files: I) -> usize {
    files
        .into_iter()
        .map(|(name, size)| {
            FILE_HEADER_BASE_SIZE
                + name.len()
                + size
                + DESCRIPTOR_SIZE
                + CENTRAL_DIRECTORY_ENTRY_BASE_SIZE
                + name.len()
        })
        .sum::<usize>()
        + END_OF_CENTRAL_DIRECTORY_SIZE
}

#[cfg(test)]
mod tests {
    use crate::{Archive, FileDateTime};
    use std::io::Cursor;

    #[test]
    fn archive_size() {
        assert_eq!(
            crate::archive_size([
                ("file1.txt", b"hello\n".len()),
                ("file2.txt", b"world\n".len()),
            ]),
            254,
        );
        assert_eq!(
            crate::archive_size([
                ("file1.txt", b"hello\n".len()),
                ("file2.txt", b"world\n".len()),
                ("file3.txt", b"how are you?\n".len()),
            ]),
            377,
        );
    }

    #[tokio::test]
    async fn archive_structure() {
        let mut archive = Archive::new(Vec::new());
        archive
            .tokio_append(
                "file1.txt".to_owned(),
                FileDateTime::now(),
                &mut Cursor::new(b"hello\n".to_vec()),
            )
            .await
            .unwrap();
        archive
            .tokio_append(
                "file2.txt".to_owned(),
                FileDateTime::now(),
                &mut Cursor::new(b"world\n".to_vec()),
            )
            .await
            .unwrap();
        let data = archive.tokio_finalize().await.unwrap();

        fn match_except_datetime(a1: &[u8], a2: &[u8]) -> bool {
            let datetime_ranges = [
                10..12,
                12..14,
                71..73,
                73..75,
                134..136,
                136..138,
                189..191,
                191..193,
            ];
            let size_ranges = [18..22, 22..26, 79..83, 83..87];
            a1.len() == a2.len()
                && a1
                    .into_iter()
                    .zip(a2)
                    .enumerate()
                    .filter(|(i, _)| {
                        datetime_ranges
                            .iter()
                            .chain(&size_ranges)
                            .all(|range| !range.contains(i))
                    })
                    .all(|(_, (b1, b2))| b1 == b2)
        }
        assert!(match_except_datetime(
            &data,
            include_bytes!("timeless_test_archive.zip")
        ));
        assert!(match_except_datetime(
            &data,
            include_bytes!("zip_command_test_archive.zip")
        ));
    }
}
