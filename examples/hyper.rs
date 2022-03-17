use hyper::service::{make_service_fn, service_fn};
use hyper::{header, Body, Request, Response, Server, StatusCode};
use std::io::Cursor;
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
            .append(filename_1, FileDateTime::now(), &mut fd_1)
            .await
            .unwrap();
        archive
            .append(filename_2, FileDateTime::now(), &mut fd_2)
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let address = ([127, 0, 0, 1], 8080).into();
    let service =
        make_service_fn(|_| async { Ok::<_, hyper::http::Error>(service_fn(zip_archive)) });
    let server = Server::bind(&address).serve(service);

    println!("Listening on http://{}", address);
    server.await?;

    Ok(())
}
