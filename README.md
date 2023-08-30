# Zipit

[![crates.io](https://img.shields.io/crates/v/zipit.svg)](https://crates.io/crates/zipit)
[![Documentation](https://docs.rs/zipit/badge.svg)](https://docs.rs/zipit)

🗄️ Create and stream a zip archive into an AsyncWrite 🗄️

```
zipit = "0.4"
```

## Features

- Stream on the fly an archive from multiple AsyncRead objects.
- Single read / seek free implementation (the CRC and file size are calculated while streaming and are sent afterwards).
- Archive size pre-calculation (useful if you want to set the `Content-Length` before streaming).
- [futures](https://docs.rs/futures/latest/futures/) and [tokio](https://docs.rs/tokio/latest/tokio/io/index.html) `AsyncRead` / `AsyncWrite` compatible. Enable either the `futures-async-io` or the `tokio-async-io` feature accordingly. 

## Limitations

- No compression (stored method only).
- Only files (no directories).
- No customizable external file attributes.

## Examples

### [File system](examples/fs.rs)

Write a zip archive to the file system using [`tokio::fs::File`](https://docs.rs/tokio/1.13.0/tokio/fs/struct.File.html):

```rust
use std::io::Cursor;
use tokio::fs::File;
use zipit::{Archive, FileDateTime};

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
```

### [Hyper](examples/hyper.rs)

Stream a zip archive as a [`hyper`](https://docs.rs/hyper/0.14.14/hyper/) response:

```rust
use std::io::Cursor;
use hyper::{header, Body, Request, Response, Server, StatusCode};
use tokio::io::duplex;
use tokio_util::io::ReaderStream;
use zipit::{archive_size, Archive, FileDateTime};

async fn zip_archive(_req: Request<Body>) -> Result<Response<Body>, hyper::http::Error> {
    let (filename_1, mut fd_1) = (String::from("file1.txt"), Cursor::new(b"hello\n".to_vec()));
    let (filename_2, mut fd_2) = (String::from("file2.txt"), Cursor::new(b"world\n".to_vec()));
    let archive_size = archive_size([
        (filename_1.as_ref(), fd_1.get_ref().len()),
        (filename_2.as_ref(), fd_2.get_ref().len()),
    ]);

    let (w, r) = duplex(4096);
    tokio::spawn(async move {
        let mut archive = Archive::new(w);
        archive
            .append(
                filename_1,
                FileDateTime::now(),
                &mut fd_1,
            )
            .await
            .unwrap();
        archive
            .append(
                filename_2,
                FileDateTime::now(),
                &mut fd_2,
            )
            .await
            .unwrap();
        archive.finalize().await.unwrap();
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, archive_size)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(Body::wrap_stream(ReaderStream::new(r)))
}
```
