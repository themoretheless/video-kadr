//! Pinned two-file SlimSAM bundle. ORT and executable remain host controlled.
use super::{local_sam, tts_model_catalog::ModelFile, tts_model_download};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
use tokio_util::sync::CancellationToken;
pub const ID: &str = "slimsam-77-uniform";
pub const REVISION: &str = "5850ab45f587c112167512ffef949107115e26a0";
pub const ENCODER: &str = "vision_encoder.onnx";
pub const DECODER: &str = "prompt_encoder_mask_decoder.onnx";
pub const BYTES: u64 = 23_276_014 + 16_557_892;
pub fn files() -> [ModelFile; 2] {
    [
        ModelFile {
            path: ENCODER.into(),
            bytes: 23_276_014,
            sha256: local_sam::ENCODER_HASH.into(),
        },
        ModelFile {
            path: DECODER.into(),
            bytes: 16_557_892,
            sha256: local_sam::DECODER_HASH.into(),
        },
    ]
}
pub fn verify_directory(root: &Path) -> Result<()> {
    ensure!(
        std::fs::symlink_metadata(root)?.file_type().is_dir(),
        "unsafe SAM bundle"
    );
    let entries = std::fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    ensure!(
        entries.len() == 2
            && entries
                .iter()
                .all(|e| [ENCODER, DECODER].iter().any(|name| e.file_name() == *name)),
        "unexpected SAM bundle contents"
    );
    for file in files() {
        let path = root.join(&file.path);
        let meta = std::fs::symlink_metadata(&path)?;
        ensure!(
            meta.file_type().is_file() && meta.len() == file.bytes,
            "invalid SAM model inventory"
        );
        let mut input = std::fs::File::open(path)?;
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            ensure!(total <= file.bytes, "SAM model grew");
            hash.update(&buffer[..n]);
        }
        ensure!(
            total == file.bytes && format!("{:x}", hash.finalize()) == file.sha256,
            "SAM model checksum mismatch"
        );
    }
    Ok(())
}
/// Caller serializes its private staging directory; verified files are reusable offline.
pub async fn download(
    client: &reqwest::Client,
    root: &Path,
    cancel: &CancellationToken,
    progress: tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "download cancelled");
    let mut completed = 0u64;
    for file in files() {
        ensure!(!cancel.is_cancelled(), "download cancelled");
        let url = url::Url::parse(&format!(
            "https://huggingface.co/Xenova/{ID}/resolve/{REVISION}/onnx/{}",
            file.path
        ))?;
        tts_model_download::fetch_file(client, url, root, &file, cancel, |bytes| {
            completed += bytes;
            let _ = progress.send((completed as f64 / BYTES as f64 * 95.).min(95.));
        })
        .await?;
    }
    ensure!(!cancel.is_cancelled(), "download cancelled");
    let parts = root.join(".download-parts");
    if tokio::fs::symlink_metadata(&parts).await.is_ok() {
        ensure!(
            tokio::fs::symlink_metadata(&parts)
                .await?
                .file_type()
                .is_dir(),
            "unsafe SAM parts directory"
        );
        tokio::fs::remove_dir(parts).await?;
    }
    let owned = root.to_path_buf();
    tokio::task::spawn_blocking(move || verify_directory(&owned)).await??;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn offline() -> reqwest::Client {
        reqwest::Client::builder()
            .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap()
    }
    #[tokio::test]
    async fn cancellation_and_corrupt_existing_files_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        assert!(download(&offline(), root.path(), &cancel, tx.clone())
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        std::fs::write(root.path().join(ENCODER), b"corrupt").unwrap();
        assert!(
            download(&offline(), root.path(), &CancellationToken::new(), tx)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(root.path().join(ENCODER)).unwrap(),
            b"corrupt"
        );
        assert!(verify_directory(root.path()).is_err());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn linked_cache_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(".download-parts")).unwrap();
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        assert!(
            download(&offline(), root.path(), &CancellationToken::new(), tx)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
    #[tokio::test]
    #[ignore = "requires retained real pinned SAM model files"]
    async fn real_pair_reuses_offline_and_rejects_changed_inventory() {
        let source = std::env::var_os("SAM_TEST_MODELS").expect("SAM_TEST_MODELS");
        let root = tempfile::tempdir().unwrap();
        for (target, name) in [
            (ENCODER, "slimsam-77-encoder.onnx"),
            (DECODER, "slimsam-77-decoder.onnx"),
        ] {
            std::fs::copy(Path::new(&source).join(name), root.path().join(target)).unwrap();
        }
        verify_directory(root.path()).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        download(&offline(), root.path(), &CancellationToken::new(), tx)
            .await
            .unwrap();
        let mut last = 0.;
        while let Some(value) = rx.recv().await {
            assert!(value >= last);
            last = value;
        }
        assert_eq!(last, 95.);
        std::fs::write(root.path().join("extra"), b"unknown").unwrap();
        assert!(verify_directory(root.path()).is_err());
        std::fs::remove_file(root.path().join("extra")).unwrap();
        std::fs::write(root.path().join(DECODER), b"damaged").unwrap();
        assert!(verify_directory(root.path()).is_err());
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
            name: "SlimSAM — выделение объекта".into(),
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
            "invalid SAM catalog"
        );
        verify_directory(root)
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
        "unknown SAM catalog"
    );
    download(client, root, cancel, progress).await
}
