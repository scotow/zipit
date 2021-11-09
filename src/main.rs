use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use crc32fast::Hasher;

#[tokio::main]
async fn main() {
    let mut file = File::create("archive.zip").await.unwrap();

    // let time = (42 >> 1) | (16 << 5) | (12 << 11);
    let time = 0x5a5d;
    let date = 0x5368;
    let filename = "file1.txt";
    let payload = b"hello\n";
    let extra = &[
        0x55, 0x54, 0x09, 0x00, 0x03, 0x91, 0xf9, 0x88, 0x61, 0xe3, 0xf9, 0x88, 0x61, 0x75, 0x78,
        0x0b, 0x00, 0x01, 0x04, 0xe9, 0x03, 0x00, 0x00, 0x04, 0xe9, 0x03, 0x00, 0x00,
    ];
    let extra2 = &[
        0x55, 0x54, 0x05, 0x00, 0x03, 0x91, 0xf9, 0x88, 0x61, 0x75, 0x78, 0x0b, 0x00, 0x01, 0x04,
        0xe9, 0x03, 0x00, 0x00, 0x04, 0xe9, 0x03, 0x00, 0x00,
    ];

    let mut hasher = Hasher::new();
    hasher.update(payload);
    let crc = hasher.finalize();

    file.write_u32_le(0x04034b50).await.unwrap(); // Local file header signature.
    file.write_u16_le(10).await.unwrap(); // Version needed to extract.
    file.write_u16_le(0x08).await.unwrap(); // General purpose flag.
    file.write_u16_le(0).await.unwrap(); // Compression method (store).
    file.write_u16_le(time).await.unwrap(); // Modification time.
    file.write_u16_le(date).await.unwrap(); // Modification date.
    file.write_u32_le(0).await.unwrap(); // Temporary CRC32.
    file.write_u32_le(payload.len() as u32).await.unwrap(); // Compressed size.
    file.write_u32_le(payload.len() as u32).await.unwrap(); // Uncompressed size.
    file.write_u16_le(filename.len() as u16).await.unwrap(); // Filename length.
    file.write_u16_le(extra.len() as u16).await.unwrap(); // Extra field length.
    file.write_all(filename.as_bytes()).await.unwrap(); // Filename.
    file.write_all(extra).await.unwrap(); // Extra field.
    file.write_all(payload).await.unwrap(); // Payload.

    file.write_u32_le(0x08074b50).await.unwrap(); // Data descriptor signature.
    file.write_u32_le(crc).await.unwrap(); // CRC32.
    file.write_u32_le(payload.len() as u32).await.unwrap(); // Compressed size.
    file.write_u32_le(payload.len() as u32).await.unwrap(); // Uncompressed size.

    file.write_u32_le(0x02014b50).await.unwrap(); // Central directory entry signature.
    file.write_u16_le(0x031e).await.unwrap(); // Version made by.
    file.write_u16_le(10).await.unwrap(); // Version needed to extract.
    file.write_u16_le(0x08).await.unwrap(); // General purpose flag.
    file.write_u16_le(0).await.unwrap(); // Compression method (store).
    file.write_u16_le(time).await.unwrap(); // Modification time.
    file.write_u16_le(date).await.unwrap(); // Modification date.
    file.write_u32_le(crc).await.unwrap(); // CRC32.
    file.write_u32_le(payload.len() as u32).await.unwrap(); // Compressed size.
    file.write_u32_le(payload.len() as u32).await.unwrap(); // Uncompressed size.
    file.write_u16_le(filename.len() as u16).await.unwrap(); // Filename length.
    file.write_u16_le(extra2.len() as u16).await.unwrap(); // Extra field length.
    file.write_u16_le(0).await.unwrap(); // File comment length.
    file.write_u16_le(0).await.unwrap(); // File's Disk number.
    file.write_u16_le(0).await.unwrap(); // Internal file attributes.
    file.write_u32_le(0b1000000110100100 << 16).await.unwrap(); // External file attributes (https://unix.stackexchange.com/questions/14705/the-zip-formats-external-file-attribute).
    file.write_u32_le(0).await.unwrap(); // Offset from start of file to local file header.
    file.write_all(filename.as_bytes()).await.unwrap(); // Filename.
    file.write_all(extra2).await.unwrap(); // Extra field.

    file.write_u32_le(0x06054b50).await.unwrap(); // End of central directory signature.
    file.write_u16_le(0).await.unwrap(); // Number of this disk.
    file.write_u16_le(0).await.unwrap(); // Number of the disk where central directory starts.
    file.write_u16_le(1).await.unwrap(); // Number of central directory records on this disk.
    file.write_u16_le(1).await.unwrap(); // Total number of central directory records.
    file.write_u32_le(79).await.unwrap(); // Size of central directory.
    file.write_u32_le(89).await.unwrap(); // Offset from start of file to central directory.
    file.write_u16_le(0).await.unwrap(); // Comment length.
}
