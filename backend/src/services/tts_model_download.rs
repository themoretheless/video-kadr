//! Verified per-file download cache for the pinned public Kokoro bundle.
use super::tts_model_catalog::{ModelCatalog, ModelFile};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

pub fn file_url(catalog: &ModelCatalog, file: &ModelFile) -> Result<url::Url> {
    catalog.validate()?;
    ensure!(
        catalog
            .files
            .iter()
            .any(|f| f.path == file.path && f.sha256 == file.sha256 && f.bytes == file.bytes),
        "unknown model file"
    );
    let mut url = url::Url::parse("https://huggingface.co/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid model URL"))?
        .extend(catalog.repository.split('/'))
        .extend(["resolve", &catalog.revision])
        .extend(file.path.split('/'));
    Ok(url)
}
/// Storage root must be trusted. Refuse symlink components below it.
pub async fn directory(root: &Path, relative: &Path) -> Result<PathBuf> {
    ensure!(
        tokio::fs::symlink_metadata(root)
            .await?
            .file_type()
            .is_dir(),
        "invalid storage root"
    );
    let mut path = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            anyhow::bail!("invalid storage path")
        };
        path.push(name);
        match tokio::fs::create_dir(&path).await {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                        .await?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        ensure!(
            tokio::fs::symlink_metadata(&path)
                .await?
                .file_type()
                .is_dir(),
            "unsafe model directory"
        );
    }
    Ok(path)
}
async fn verify_file(path: &Path, file: &ModelFile) -> Result<()> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() == file.bytes,
        "invalid cached model file"
    );
    let (bytes, hash) = super::local_speech::fingerprint_model(path)
        .await
        .map_err(|_| anyhow::anyhow!("read cached model"))?;
    ensure!(
        bytes == file.bytes && hash == file.sha256,
        "cached model checksum mismatch"
    );
    Ok(())
}
pub(crate) async fn fetch_file(
    client: &reqwest::Client,
    url: url::Url,
    root: &Path,
    file: &ModelFile,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    let parent = Path::new(&file.path).parent().context("model parent")?;
    directory(root, parent).await?;
    let target = root.join(&file.path);
    let part_name = format!("{:x}", Sha256::digest(file.path.as_bytes()));
    if tokio::fs::symlink_metadata(&target).await.is_ok() {
        verify_file(&target, file).await?;
        let parts = root.join(".download-parts");
        if tokio::fs::symlink_metadata(&parts).await.is_ok() {
            ensure!(
                tokio::fs::symlink_metadata(&parts)
                    .await?
                    .file_type()
                    .is_dir(),
                "unsafe parts directory"
            );
            let stale = parts.join(&part_name);
            if let Ok(metadata) = tokio::fs::symlink_metadata(&stale).await {
                ensure!(
                    metadata.file_type().is_file(),
                    "unsafe interrupted download"
                );
                tokio::fs::remove_file(stale).await?;
            }
        }
        progress(file.bytes);
        return Ok(());
    }
    // Separate cache files from the final bundle so no suffix collides with a catalog entry.
    let parts = directory(root, Path::new(".download-parts")).await?;
    let part = parts.join(part_name);
    match tokio::fs::symlink_metadata(&part).await {
        Ok(meta) => {
            ensure!(meta.file_type().is_file(), "unsafe interrupted download");
            tokio::fs::remove_file(&part).await?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        output
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .await?;
    }
    let mut response = tokio::select! { _=cancel.cancelled()=>anyhow::bail!("download cancelled"), result=client.get(url).send()=>result? }.error_for_status()?;
    if let Some(length) = response.content_length() {
        ensure!(length == file.bytes, "model response length mismatch");
    }
    let mut received = 0u64;
    let mut hash = Sha256::new();
    loop {
        let chunk = tokio::select! { _=cancel.cancelled()=>anyhow::bail!("download cancelled"), result=response.chunk()=>result? };
        let Some(chunk) = chunk else { break };
        received = received
            .checked_add(chunk.len() as u64)
            .context("model size overflow")?;
        ensure!(received <= file.bytes, "model response exceeds catalog");
        output.write_all(&chunk).await?;
        hash.update(&chunk);
        progress(chunk.len() as u64);
    }
    ensure!(
        received == file.bytes && format!("{:x}", hash.finalize()) == file.sha256,
        "downloaded model checksum mismatch"
    );
    output.sync_all().await?;
    drop(output);
    ensure!(!cancel.is_cancelled(), "download cancelled");
    // No-replace publication of this verified file inside the owned job cache.
    tokio::fs::hard_link(&part, &target).await?;
    tokio::fs::remove_file(part).await?;
    Ok(())
}
/// Complete files survive restart; an interrupted individual file restarts from zero.
pub async fn download(
    client: &reqwest::Client,
    catalog: &ModelCatalog,
    root: &Path,
    cancel: &CancellationToken,
    progress: tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<()> {
    catalog.validate()?;
    let mut completed = 0u64;
    for file in &catalog.files {
        ensure!(!cancel.is_cancelled(), "download cancelled");
        fetch_file(
            client,
            file_url(catalog, file)?,
            root,
            file,
            cancel,
            |bytes| {
                completed += bytes;
                let _ = progress.send((completed as f64 / catalog.bytes as f64 * 95.).min(95.));
            },
        )
        .await?;
    }
    let parts = root.join(".download-parts");
    if tokio::fs::symlink_metadata(&parts).await.is_ok() {
        ensure!(
            tokio::fs::symlink_metadata(&parts)
                .await?
                .file_type()
                .is_dir(),
            "unsafe parts directory"
        );
        // Only empty owned cache metadata is removed; unknown contents cause refusal.
        tokio::fs::remove_dir(parts).await?;
    }
    let root = root.to_path_buf();
    let catalog = catalog.clone();
    tokio::task::spawn_blocking(move || catalog.verify_directory(&root)).await??;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_urls_encode_spaces_without_changing_path_segments() {
        let mut escaped = 0;
        for catalog in ModelCatalog::all().unwrap() {
            for file in &catalog.files {
                let url = file_url(&catalog, file).unwrap();
                assert_eq!(url.scheme(), "https");
                assert_eq!(url.host_str(), Some("huggingface.co"));
                assert!(url.path().starts_with(&format!(
                    "/{}/resolve/{}/",
                    catalog.repository, catalog.revision
                )));
                assert_eq!(
                    url.path_segments().unwrap().count(),
                    4 + file.path.split('/').count()
                );
                if file.path.contains(' ') {
                    assert!(url.as_str().contains("%20"));
                    escaped += 1;
                }
                assert!(url.query().is_none());
                assert!(url.fragment().is_none());
            }
        }
        assert!(
            escaped > 0,
            "source inventories must exercise space encoding"
        );
    }
    #[tokio::test]
    async fn file_download_reuses_verified_cache_and_refuses_corruption() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut b = [0; 4096];
            socket.read(&mut b).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc")
                .await
                .unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let file = ModelFile {
            path: "nested/file".into(),
            bytes: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        };
        let cancel = CancellationToken::new();
        let client = reqwest::Client::new();
        let url = url::Url::parse(&format!("http://{addr}/file")).unwrap();
        let mut received = 0;
        fetch_file(&client, url.clone(), dir.path(), &file, &cancel, |n| {
            received += n
        })
        .await
        .unwrap();
        assert_eq!(received, 3);
        server.await.unwrap();
        // Listener is gone: success must come from checksum-verified persisted bytes.
        fetch_file(&client, url.clone(), dir.path(), &file, &cancel, |_| {})
            .await
            .unwrap();
        tokio::fs::write(dir.path().join(&file.path), b"abd")
            .await
            .unwrap();
        assert!(fetch_file(&client, url, dir.path(), &file, &cancel, |_| {})
            .await
            .is_err());
    }
    #[tokio::test]
    async fn interrupted_transfer_restarts_only_incomplete_file() {
        use tokio::io::AsyncReadExt;
        for cancelled in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let file = ModelFile {
                path: "nested/model".into(),
                bytes: 10,
                sha256: format!("{:x}", Sha256::digest(b"0123456789")),
            };
            let complete = ModelFile {
                path: "prior".into(),
                bytes: 3,
                sha256: format!("{:x}", Sha256::digest(b"abc")),
            };
            tokio::fs::write(root.path().join("prior"), b"abc")
                .await
                .unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (started, ready) = tokio::sync::oneshot::channel();
            let (release, released) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                socket.read(&mut request).await.unwrap();
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n012",
                    )
                    .await
                    .unwrap();
                started.send(()).unwrap();
                if cancelled {
                    let _ = released.await;
                }
                drop(socket);
                let (mut socket, _) = listener.accept().await.unwrap();
                let length = socket.read(&mut request).await.unwrap();
                assert!(!String::from_utf8_lossy(&request[..length])
                    .to_ascii_lowercase()
                    .contains("range:"));
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789").await.unwrap();
            });
            let url = url::Url::parse(&format!("http://{address}/model")).unwrap();
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap();
            let token = CancellationToken::new();
            let attempt = fetch_file(&client, url.clone(), root.path(), &file, &token, |_| {});
            let interrupt = async {
                ready.await.unwrap();
                if cancelled {
                    let partial = root
                        .path()
                        .join(".download-parts")
                        .join(format!("{:x}", Sha256::digest(file.path.as_bytes())));
                    tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        loop {
                            if tokio::fs::metadata(&partial)
                                .await
                                .is_ok_and(|m| m.len() == 3)
                            {
                                break;
                            }
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .unwrap();
                    token.cancel();
                }
            };
            let (result, ()) = tokio::join!(attempt, interrupt);
            assert!(result.is_err());
            if !cancelled {
                assert!(result
                    .unwrap_err()
                    .chain()
                    .any(|e| e.downcast_ref::<reqwest::Error>().is_some()));
            }
            assert!(!root.path().join(&file.path).exists());
            let _ = release.send(());
            // An independent client/token models a resumed attempt. Completed files
            // are rehashed and reused without contacting the deliberately wrong URL.
            let resumed = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap();
            let token = CancellationToken::new();
            let mut reused = 0;
            fetch_file(&resumed, url.clone(), root.path(), &complete, &token, |n| {
                reused += n
            })
            .await
            .unwrap();
            assert_eq!(reused, 3);
            fetch_file(&resumed, url, root.path(), &file, &token, |_| {})
                .await
                .unwrap();
            assert_eq!(
                tokio::fs::read(root.path().join(&file.path)).await.unwrap(),
                b"0123456789"
            );
            assert_eq!(
                tokio::fs::read(root.path().join("prior")).await.unwrap(),
                b"abc"
            );
            assert_eq!(
                tokio::fs::read_dir(root.path().join(".download-parts"))
                    .await
                    .unwrap()
                    .next_entry()
                    .await
                    .unwrap()
                    .is_none(),
                true
            );
            server.await.unwrap();
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn storage_symlink_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("models")).unwrap();
        assert!(directory(root.path(), Path::new("models/tts"))
            .await
            .is_err());
        assert!(!outside.path().join("tts").exists());
    }
}
