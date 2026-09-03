use std::path::{Path as FilePath, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt, WriteMultipart};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;
use uuid::Uuid;

const TRANSFER_CHUNK_BYTES: usize = 8 * 1024 * 1024;

/// Provider-independent source backup backed by an S3-compatible object store.
/// The URL may include a key prefix, for example `s3://bucket/video-kadr`.
#[derive(Clone)]
pub struct SourceObjectStore {
    store: Arc<dyn ObjectStore>,
    prefix: Path,
}

impl SourceObjectStore {
    pub fn from_url(raw_url: &str) -> Result<Self> {
        let url = Url::parse(raw_url).context("OBJECT_STORE_URL must be a valid URL")?;
        let (store, prefix) = object_store::parse_url_opts(&url, std::env::vars())
            .context("failed to configure OBJECT_STORE_URL")?;
        Ok(Self {
            store: Arc::from(store),
            prefix,
        })
    }

    #[cfg(test)]
    pub fn in_memory(prefix: &str) -> Result<Self> {
        Ok(Self {
            store: Arc::new(object_store::memory::InMemory::new()),
            prefix: Path::parse(prefix)?,
        })
    }

    fn object_path(&self, storage_key: &FilePath) -> Result<Path> {
        let key = storage_key
            .to_str()
            .context("media storage key must be UTF-8")?;
        let key = Path::parse(key).context("invalid media object key")?;
        Ok(self
            .prefix
            .clone()
            .join("sources")
            .parts()
            .chain(key.parts())
            .collect())
    }

    /// Upload a source when the object is absent or has a different length.
    /// Space sources are immutable, making size equality a safe idempotency key
    /// without reading the complete remote object on every reconciliation pass.
    pub async fn sync(&self, local_path: &FilePath, storage_key: &FilePath) -> Result<bool> {
        let object_path = self.object_path(storage_key)?;
        let local_size = tokio::fs::metadata(local_path).await?.len();
        match self.store.head(&object_path).await {
            Ok(remote) if remote.size == local_size => return Ok(false),
            Ok(_) | Err(object_store::Error::NotFound { .. }) => {}
            Err(error) => return Err(error.into()),
        }
        let upload = self.store.put_multipart(&object_path).await?;
        let mut writer = WriteMultipart::new_with_chunk_size(upload, TRANSFER_CHUNK_BYTES);
        let mut file = tokio::fs::File::open(local_path).await?;
        let mut buffer = vec![0_u8; TRANSFER_CHUNK_BYTES];
        loop {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            writer.wait_for_capacity(4).await?;
            writer.write(&buffer[..read]);
        }
        writer.finish().await?;
        Ok(true)
    }

    pub async fn hydrate(&self, local_path: &FilePath, storage_key: &FilePath) -> Result<()> {
        if tokio::fs::try_exists(local_path).await? {
            return Ok(());
        }
        let parent = local_path.parent().context("media path has no parent")?;
        tokio::fs::create_dir_all(parent).await?;
        let temp_path = temporary_path(local_path);
        let mut temp = tokio::fs::File::create(&temp_path).await?;
        let mut stream = self
            .store
            .get(&self.object_path(storage_key)?)
            .await?
            .into_stream();
        while let Some(chunk) = stream.next().await {
            temp.write_all(&chunk?).await?;
        }
        temp.flush().await?;
        temp.sync_all().await?;
        drop(temp);
        if let Err(error) = tokio::fs::rename(&temp_path, local_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            if tokio::fs::try_exists(local_path).await? {
                return Ok(());
            }
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn delete(&self, storage_key: &FilePath) -> Result<()> {
        let path = self.object_path(storage_key)?;
        match self.store.delete(&path).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn temporary_path(path: &FilePath) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.hydrate", Uuid::new_v4()));
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn uploads_and_atomically_hydrates_a_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.bin");
        let restored = temp.path().join("nested/restored.bin");
        let bytes = vec![7_u8; TRANSFER_CHUNK_BYTES + 17];
        tokio::fs::write(&source, &bytes).await.unwrap();
        let store = SourceObjectStore::in_memory("workspace").unwrap();

        assert!(store
            .sync(&source, FilePath::new("spaces/a/source.bin"))
            .await
            .unwrap());
        assert!(!store
            .sync(&source, FilePath::new("spaces/a/source.bin"))
            .await
            .unwrap());
        store
            .hydrate(&restored, FilePath::new("spaces/a/source.bin"))
            .await
            .unwrap();

        assert_eq!(tokio::fs::read(restored).await.unwrap(), bytes);
    }
}
