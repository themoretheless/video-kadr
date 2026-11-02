//! Pinned public enhancement model; engine libraries remain host-controlled.
use super::{tts_model_catalog::ModelFile, tts_model_download};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
use tokio_util::sync::CancellationToken;

pub const ID: &str = "realesr-general-x4v3";
pub const SHA256: &str = "1940a93ee08283a0a7286183186357b1688fe9fa8ede74604b424586aaddf112";
pub const BYTES: u64 = 4_866_417;
pub const FILE: &str = "realesr-general-x4v3.onnx";
pub const REVISION: &str = "c6a971706797c7502945a2b4c4274fce4900d4ab";
pub const URL: &str = "https://huggingface.co/CoderViking/realesr-general-x4v3-onnx/resolve/c6a971706797c7502945a2b4c4274fce4900d4ab/realesr-general-x4v3.onnx";

/// Refuse special files, truncated downloads and changed bytes before use.
pub fn verify(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() == BYTES,
        "invalid enhancement model size/type"
    );
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        ensure!(total <= BYTES, "enhancement model grew during verification");
        hash.update(&buffer[..count]);
    }
    ensure!(
        total == BYTES && format!("{:x}", hash.finalize()) == SHA256,
        "enhancement model checksum mismatch"
    );
    Ok(())
}

/// Caller owns and serializes this private job cache. Verified files survive
/// restart; an incomplete file restarts, never becoming an installed model.
pub async fn download(
    client: &reqwest::Client,
    root: &Path,
    cancel: &CancellationToken,
    progress: tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "download cancelled");
    ensure!(
        tokio::fs::symlink_metadata(root)
            .await?
            .file_type()
            .is_dir(),
        "unsafe enhancement cache"
    );
    let file = ModelFile {
        path: FILE.into(),
        bytes: BYTES,
        sha256: SHA256.into(),
    };
    let mut received = 0u64;
    tts_model_download::fetch_file(
        client,
        url::Url::parse(URL)?,
        root,
        &file,
        cancel,
        |bytes| {
            received += bytes;
            let _ = progress.send((received as f64 / BYTES as f64 * 95.).min(95.));
        },
    )
    .await?;
    ensure!(!cancel.is_cancelled(), "download cancelled");
    let parts = root.join(".download-parts");
    if tokio::fs::symlink_metadata(&parts).await.is_ok() {
        ensure!(
            tokio::fs::symlink_metadata(&parts)
                .await?
                .file_type()
                .is_dir(),
            "unsafe enhancement parts directory"
        );
        tokio::fs::remove_dir(parts).await?;
    }
    let path = root.join(FILE);
    tokio::task::spawn_blocking(move || verify(&path)).await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_and_corrupt_cache_never_fetch_or_overwrite() {
        let root = tempfile::tempdir().unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_millis(10))
            .build()
            .unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        assert!(download(&client, root.path(), &cancel, tx.clone())
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        std::fs::write(root.path().join(FILE), b"bad model").unwrap();
        assert!(
            download(&client, root.path(), &CancellationToken::new(), tx)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), b"bad model");
        assert!(verify(&root.path().join(FILE)).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn linked_model_is_refused() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("target"), b"model").unwrap();
        std::os::unix::fs::symlink(root.path().join("target"), root.path().join(FILE)).unwrap();
        assert!(verify(&root.path().join(FILE)).is_err());
    }
    #[tokio::test]
    #[ignore = "downloads pinned public learned model"]
    async fn actual_public_model_download_and_offline_reuse() {
        let root = tempfile::tempdir().unwrap();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        download(&client, root.path(), &CancellationToken::new(), tx)
            .await
            .unwrap();
        verify(&root.path().join(FILE)).unwrap();
        let mut previous = 0.;
        while let Some(value) = rx.recv().await {
            assert!(value >= previous && value <= 95.);
            previous = value;
        }
        assert_eq!(previous, 95.);
        let offline = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        download(&offline, root.path(), &CancellationToken::new(), tx)
            .await
            .unwrap();
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.path().join(FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub revision: String,
}
impl Catalog {
    pub fn bundled() -> Result<Self> {
        Ok(Self {
            id: ID.into(),
            name: "Real-ESRGAN — улучшение изображений".into(),
            bytes: BYTES,
            revision: REVISION.into(),
        })
    }
    pub fn all() -> Result<Vec<Self>> {
        Ok(vec![Self::bundled()?])
    }
    pub fn find(id: &str) -> Result<Option<Self>> {
        Ok(if id == ID {
            Some(Self::bundled()?)
        } else {
            None
        })
    }
    pub fn verify_directory(&self, root: &Path) -> Result<()> {
        ensure!(
            self.id == ID && self.bytes == BYTES && self.revision == REVISION,
            "invalid enhancement catalog"
        );
        ensure!(
            std::fs::symlink_metadata(root)?.file_type().is_dir(),
            "unsafe enhancement bundle"
        );
        let entries = std::fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
        ensure!(
            entries.len() == 1 && entries[0].file_name() == FILE,
            "unexpected enhancement bundle contents"
        );
        verify(&root.join(FILE))
    }
}
pub use super::tts_model_download::directory;
pub async fn download_catalog(
    client: &reqwest::Client,
    catalog: &Catalog,
    root: &Path,
    cancel: &CancellationToken,
    progress: tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<()> {
    ensure!(
        catalog.id == ID && catalog.bytes == BYTES && catalog.revision == REVISION,
        "unknown enhancement catalog"
    );
    download(client, root, cancel, progress).await?;
    let catalog = catalog.clone();
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || catalog.verify_directory(&root)).await??;
    Ok(())
}
