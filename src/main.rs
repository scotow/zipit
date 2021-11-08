use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() {
    let mut file = File::create("archive.zip").await.unwrap();

    let filename = "file1.txt";
    let payload = "hello";

    file.write_u32_le(0x04034b50).await.unwrap(); // Magic.
    file.write_u16_le(10).await.unwrap(); // Version needed to extract.
    file.write_u16_le(0x08).await.unwrap(); // General purpose flag.
    file.write_u16_le(0).await.unwrap(); // Compression method (store).
    file.write_u16_le(0).await.unwrap(); // Modification time.
    file.write_u16_le(0).await.unwrap(); // Modification date.
    file.write_u32_le(0).await.unwrap(); // CRC32.
    file.write_u32_le(0).await.unwrap(); // Compressed size.
    file.write_u32_le(0).await.unwrap(); // Uncompressed size.
    file.write_u16_le(filename.len() as u16).await.unwrap(); // Filename length.
    file.write_u16_le(0).await.unwrap(); // Extra field length.
    file.write_all(filename.as_bytes()).await.unwrap(); // Filename.
    file.write_all(payload.as_bytes()).await.unwrap(); // Payload.
}
