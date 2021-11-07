use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() {
    let mut file = File::create("archive.zip").await.unwrap();

    file.write_u32_le(0x04034b50).await.unwrap();
    file.write_u16_le(10).await.unwrap();
    file.write_u16_le(0x08).await.unwrap();
    file.write_u16_le(0).await.unwrap();
}
