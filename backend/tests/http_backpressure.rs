//! Transport-level regressions for incremental bodies and slow readers.

mod support;

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use support::{make_state, router};

async fn wait_for_staged_file(directory: &std::path::Path) -> std::path::PathBuf {
    for _ in 0..100 {
        let mut entries = tokio::fs::read_dir(directory).await.unwrap();
        if let Some(entry) = entries.next_entry().await.unwrap() {
            return entry.path();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("upload was not staged");
}

async fn wait_until_empty(directory: &std::path::Path) {
    for _ in 0..100 {
        let mut entries = tokio::fs::read_dir(directory).await.unwrap();
        if entries.next_entry().await.unwrap().is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("staging did not drain");
}

#[tokio::test]
async fn upload_is_incremental_and_disconnect_releases_staging() {
    let (state, storage) = make_state(true, true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });

    let boundary = "BACKPRESSURE_BOUNDARY";
    let declared_bytes = 8 * 1024 * 1024;
    let header = format!(
        "POST /api/upload HTTP/1.1\r\nHost: {address}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {declared_bytes}\r\nConnection: close\r\n\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"large.mp4\"\r\nContent-Type: video/mp4\r\n\r\n"
    );
    let delivered = vec![b'x'; 32 * 1024];
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client.write_all(header.as_bytes()).await.unwrap();
    client.write_all(&delivered).await.unwrap();
    client.flush().await.unwrap();

    let staging = storage.path().join("staging");
    let staged_file = wait_for_staged_file(&staging).await;
    let mut staged_bytes = 0;
    for _ in 0..100 {
        staged_bytes = tokio::fs::metadata(&staged_file).await.unwrap().len();
        if staged_bytes > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(staged_bytes > 0);
    assert!(staged_bytes <= delivered.len() as u64);
    assert!(staged_bytes < declared_bytes as u64);

    client.shutdown().await.unwrap();
    wait_until_empty(&staging).await;
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn slow_range_reader_does_not_block_api_progress() {
    let (state, storage) = make_state(true, true).await;
    tokio::fs::write(
        storage.path().join("sources/large.mp4"),
        vec![b'x'; 4 * 1024 * 1024],
    )
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });

    let mut slow_client = tokio::net::TcpStream::connect(address).await.unwrap();
    slow_client
        .write_all(
            format!(
                "GET /files/sources/large.mp4 HTTP/1.1\r\nHost: {address}\r\nRange: bytes=0-4194303\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut prefix = [0_u8; 512];
    let read = tokio::time::timeout(Duration::from_secs(1), slow_client.read(&mut prefix))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&prefix[..read]).contains("206 Partial Content"));

    let mut health_client = tokio::net::TcpStream::connect(address).await.unwrap();
    health_client
        .write_all(
            format!("GET /api/health HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(1),
        health_client.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(String::from_utf8_lossy(&response).contains("200 OK"));

    drop(slow_client);
    server.abort();
    let _ = server.await;
}
