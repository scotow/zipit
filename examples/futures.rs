use zipit::{Archive, FileDateTime};
use futures_util::io::Cursor;

#[tokio::main]
async fn main() {
    let mut archive = Archive::new(Vec::<u8>::new());
    archive
        .append(
            "file1.txt".to_owned(),
            FileDateTime::now(),
            &mut Cursor::new(b"hello\n".to_vec()),
        )
        .await
        .unwrap();
    archive
        .append(
            "file2.txt".to_owned(),
            FileDateTime::now(),
            &mut Cursor::new(b"world\n".to_vec()),
        )
        .await
        .unwrap();
    let data = archive.finalize().await.unwrap();
    println!("The archive size is {}.", data.len());
}
