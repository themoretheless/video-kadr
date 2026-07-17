use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Connection};
use uuid::Uuid;

use crate::db::Db;
use crate::library::now_secs;

const MANIFEST_NAME: &str = "manifest.json";
const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub schema_version: u32,
    pub created_at: u64,
    pub tool_version: String,
    pub root_hash: String,
    pub files: Vec<BackupFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

pub async fn create_snapshot(storage: &Path, backup_root: &Path, db: &Db) -> Result<PathBuf> {
    tokio::fs::create_dir_all(backup_root).await?;
    let stage = backup_root.join(format!(".snapshot-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&stage).await?;
    if let Err(error) = db.snapshot_to(&stage.join("app.db")).await {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(error).context("snapshot SQLite database");
    }

    let storage = storage.to_path_buf();
    let stage_for_copy = stage.clone();
    let manifest_result = tokio::task::spawn_blocking(move || {
        copy_optional_file(
            &storage.join("library.json"),
            &stage_for_copy.join("library.json"),
        )?;
        copy_tree(&storage.join("sources"), &stage_for_copy.join("sources"))?;
        copy_tree(&storage.join("outputs"), &stage_for_copy.join("outputs"))?;
        build_manifest(&stage_for_copy)
    })
    .await;
    let manifest = match manifest_result {
        Ok(Ok(manifest)) => manifest,
        Ok(Err(error)) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error);
        }
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error).context("join backup worker");
        }
    };

    let manifest_bytes = match serde_json::to_vec_pretty(&manifest) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error.into());
        }
    };
    if let Err(error) = tokio::fs::write(stage.join(MANIFEST_NAME), manifest_bytes).await {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(error).context("write backup manifest");
    }
    let destination = backup_root.join(&manifest.root_hash);
    if destination.exists() {
        if verify_snapshot(&destination).await.is_ok() {
            tokio::fs::remove_dir_all(&stage).await?;
            return Ok(destination);
        }
        if let Err(error) = tokio::fs::remove_dir_all(&destination).await {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error).context("replace corrupt backup snapshot");
        }
    }
    if let Err(error) = tokio::fs::rename(&stage, &destination).await {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(error).context("publish backup snapshot");
    }
    if let Err(error) = verify_snapshot(&destination).await {
        let _ = tokio::fs::remove_dir_all(&destination).await;
        return Err(error);
    }
    Ok(destination)
}

pub async fn verify_snapshot(snapshot: &Path) -> Result<BackupManifest> {
    let snapshot = snapshot.to_path_buf();
    let snapshot_for_files = snapshot.clone();
    let manifest = tokio::task::spawn_blocking(move || verify_files(&snapshot_for_files))
        .await
        .context("join backup verifier")??;

    let options = SqliteConnectOptions::new()
        .filename(snapshot.join("app.db"))
        .read_only(true)
        .disable_statement_logging();
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut connection)
        .await?;
    if integrity != "ok" {
        return Err(anyhow!("SQLite integrity check failed: {integrity}"));
    }
    Ok(manifest)
}

pub async fn restore_snapshot(snapshot: &Path, target: &Path) -> Result<()> {
    let manifest = verify_snapshot(snapshot).await?;
    ensure_empty_target(target)?;
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("restore target needs a parent directory"))?;
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(".restore-{}", Uuid::new_v4()));
    let snapshot = snapshot.to_path_buf();
    let stage_for_copy = stage.clone();
    let restore_result = tokio::task::spawn_blocking(move || -> Result<()> {
        fs::create_dir_all(&stage_for_copy)?;
        for file in &manifest.files {
            let relative = safe_relative_path(&file.path)?;
            let source = snapshot.join(&relative);
            let destination = stage_for_copy.join(&relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, destination)?;
        }
        Ok(())
    })
    .await;
    match restore_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error);
        }
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error).context("join restore worker");
        }
    }

    if target.exists() {
        tokio::fs::remove_dir(target).await?;
    }
    if let Err(error) = tokio::fs::rename(&stage, target).await {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(error).context("publish restored storage");
    }
    Ok(())
}

fn build_manifest(snapshot: &Path) -> Result<BackupManifest> {
    let mut paths = Vec::new();
    collect_files(snapshot, snapshot, &mut paths)?;
    paths.sort();
    let mut files = Vec::with_capacity(paths.len());
    for relative in paths {
        if relative == Path::new(MANIFEST_NAME) {
            continue;
        }
        let absolute = snapshot.join(&relative);
        let metadata = fs::symlink_metadata(&absolute)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!("backup payload contains a non-regular file"));
        }
        files.push(BackupFile {
            path: path_token(&relative)?,
            size: metadata.len(),
            sha256: hash_file(&absolute)?,
        });
    }
    let root_hash = hash_manifest_files(&files);
    Ok(BackupManifest {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        created_at: now_secs(),
        tool_version: env!("CARGO_PKG_VERSION").into(),
        root_hash,
        files,
    })
}

fn verify_files(snapshot: &Path) -> Result<BackupManifest> {
    let bytes = fs::read(snapshot.join(MANIFEST_NAME)).context("read backup manifest")?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(anyhow!("backup manifest is too large"));
    }
    let manifest: BackupManifest = serde_json::from_slice(&bytes)?;
    if manifest.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported backup schema {}",
            manifest.schema_version
        ));
    }
    if manifest.root_hash != hash_manifest_files(&manifest.files) {
        return Err(anyhow!("backup root hash does not match manifest"));
    }

    let mut declared = BTreeSet::new();
    for file in &manifest.files {
        let relative = safe_relative_path(&file.path)?;
        if !declared.insert(relative.clone()) {
            return Err(anyhow!("duplicate backup path {}", file.path));
        }
        let absolute = snapshot.join(relative);
        let metadata = fs::symlink_metadata(&absolute)
            .with_context(|| format!("missing backup file {}", file.path))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!("backup file is not regular: {}", file.path));
        }
        if metadata.len() != file.size || hash_file(&absolute)? != file.sha256 {
            return Err(anyhow!("backup checksum mismatch: {}", file.path));
        }
    }

    let mut actual = Vec::new();
    collect_files(snapshot, snapshot, &mut actual)?;
    let actual: BTreeSet<_> = actual
        .into_iter()
        .filter(|path| path != Path::new(MANIFEST_NAME))
        .collect();
    if actual != declared {
        return Err(anyhow!("backup contains undeclared or missing files"));
    }
    if !declared.contains(Path::new("app.db")) {
        return Err(anyhow!("backup does not contain app.db"));
    }
    verify_library_references(snapshot, &declared)?;
    Ok(manifest)
}

fn verify_library_references(snapshot: &Path, declared: &BTreeSet<PathBuf>) -> Result<()> {
    let library_path = snapshot.join("library.json");
    if !library_path.exists() {
        return Ok(());
    }
    let entries: Vec<crate::library::MediaEntry> =
        serde_json::from_slice(&fs::read(library_path)?)?;
    for entry in entries {
        if !plain_filename(&entry.filename) {
            return Err(anyhow!("unsafe filename in library backup"));
        }
        let directory = if entry.kind == "output" {
            "outputs"
        } else {
            "sources"
        };
        if !declared.contains(&Path::new(directory).join(&entry.filename)) {
            return Err(anyhow!("library backup references a missing media file"));
        }
    }
    Ok(())
}

fn hash_manifest_files(files: &[BackupFile]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"video-editor-backup-v1\0");
    for file in files {
        hash.update(file.path.as_bytes());
        hash.update(b"\0");
        hash.update(file.size.to_be_bytes());
        hash.update(b"\0");
        hash.update(file.sha256.as_bytes());
        hash.update(b"\0");
    }
    format!("{:x}", hash.finalize())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn copy_optional_file(source: &Path, destination: &Path) -> Result<()> {
    match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::copy(source, destination)?;
        }
        Ok(_) => return Err(anyhow!("backup source is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    let source_metadata = fs::symlink_metadata(source)?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
        return Err(anyhow!("backup source tree is not a regular directory"));
    }
    fs::create_dir_all(destination)?;
    let mut entries: Vec<_> = fs::read_dir(source)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(anyhow!("backup refuses symlinks and special files"));
        }
    }
    Ok(())
}

fn collect_files(root: &Path, directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(directory)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &entry.path(), output)?;
        } else if file_type.is_file() {
            output.push(entry.path().strip_prefix(root)?.to_path_buf());
        } else {
            return Err(anyhow!("backup refuses symlinks and special files"));
        }
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(anyhow!("unsafe backup path"));
    }
    Ok(path.to_path_buf())
}

fn path_token(path: &Path) -> Result<String> {
    let token = path
        .to_str()
        .ok_or_else(|| anyhow!("backup path is not UTF-8"))?
        .replace(std::path::MAIN_SEPARATOR, "/");
    safe_relative_path(&token)?;
    Ok(token)
}

fn ensure_empty_target(target: &Path) -> Result<()> {
    if !target.exists() {
        return Ok(());
    }
    if !target.is_dir() || fs::read_dir(target)?.next().transpose()?.is_some() {
        return Err(anyhow!("restore target must be an empty directory"));
    }
    Ok(())
}

fn plain_filename(filename: &str) -> bool {
    !filename.is_empty()
        && Path::new(filename)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(filename)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn backup_verify_delete_and_restore_drill() {
        let source = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(source.path().join("sources"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(source.path().join("staging"))
            .await
            .unwrap();
        tokio::fs::write(source.path().join("sources/video.mp4"), b"media")
            .await
            .unwrap();
        tokio::fs::write(source.path().join("staging/private.upload"), b"secret")
            .await
            .unwrap();
        tokio::fs::write(source.path().join("library.json"), b"[]")
            .await
            .unwrap();
        let db = Db::open(source.path()).await.unwrap();
        db.upsert_project("video", "fixture", &json!({"id":"video"}), &json!({}))
            .await
            .unwrap();

        let backup_root = tempfile::tempdir().unwrap();
        let snapshot = create_snapshot(source.path(), backup_root.path(), &db)
            .await
            .unwrap();
        let manifest = verify_snapshot(&snapshot).await.unwrap();
        assert!(manifest.files.iter().any(|file| file.path == "app.db"));
        assert!(manifest
            .files
            .iter()
            .any(|file| file.path == "sources/video.mp4"));
        assert!(!manifest
            .files
            .iter()
            .any(|file| file.path.contains("staging")));

        let restore_parent = tempfile::tempdir().unwrap();
        let restored = restore_parent.path().join("restored-storage");
        restore_snapshot(&snapshot, &restored).await.unwrap();
        assert_eq!(
            fs::read(restored.join("sources/video.mp4")).unwrap(),
            b"media"
        );
        let restored_db = Db::open(&restored).await.unwrap();
        assert_eq!(restored_db.list_projects().await.unwrap().len(), 1);

        fs::write(snapshot.join("sources/video.mp4"), b"corrupt").unwrap();
        assert!(verify_snapshot(&snapshot).await.is_err());
        let repaired = create_snapshot(source.path(), backup_root.path(), &db)
            .await
            .unwrap();
        assert_eq!(repaired, snapshot);
        verify_snapshot(&repaired).await.unwrap();
    }

    #[test]
    fn restore_paths_cannot_escape_the_target() {
        assert!(safe_relative_path("sources/video.mp4").is_ok());
        for path in ["../app.db", "/tmp/app.db", "sources/../app.db", ""] {
            assert!(safe_relative_path(path).is_err(), "{path:?}");
        }
    }
}
