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