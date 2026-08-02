use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Connection, Row};
use uuid::Uuid;

use crate::db::Db;
use crate::library::now_secs;
use crate::luts::{parse_cube, MAX_CUBE_SIZE, MAX_LUT_FILE_BYTES};

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotLut {
    id: String,
    filename: String,
    cube_size: u32,
    size: u64,
    sha256: String,
}

pub async fn create_snapshot(storage: &Path, backup_root: &Path, db: &Db) -> Result<PathBuf> {
    tokio::fs::create_dir_all(backup_root).await?;
    let stage = backup_root.join(format!(".snapshot-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&stage).await?;
    if let Err(error) = db.snapshot_to(&stage.join("app.db")).await {
        let _ = tokio::fs::remove_dir_all(&stage).await;
        return Err(error).context("snapshot SQLite database");
    }
    let snapshot_luts = match load_snapshot_luts(&stage.join("app.db")).await {
        Ok(luts) => luts,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(error).context("read LUT references from database snapshot");
        }
    };

    let storage = storage.to_path_buf();
    let stage_for_copy = stage.clone();
    let manifest_result = tokio::task::spawn_blocking(move || {
        copy_optional_file(
            &storage.join("library.json"),
            &stage_for_copy.join("library.json"),
        )?;
        copy_tree(&storage.join("sources"), &stage_for_copy.join("sources"))?;
        copy_tree(&storage.join("outputs"), &stage_for_copy.join("outputs"))?;
        copy_snapshot_luts(
            &storage.join("luts"),
            &stage_for_copy.join("luts"),
            &snapshot_luts,
        )?;
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
    let snapshot_luts = load_snapshot_luts_from_connection(&mut connection).await?;
    verify_lut_references(&snapshot, &manifest, &snapshot_luts)?;
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

async fn load_snapshot_luts(database: &Path) -> Result<Vec<SnapshotLut>> {
    let options = SqliteConnectOptions::new()
        .filename(database)
        .read_only(true)
        .disable_statement_logging();
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    load_snapshot_luts_from_connection(&mut connection).await
}

async fn load_snapshot_luts_from_connection(
    connection: &mut sqlx::SqliteConnection,
) -> Result<Vec<SnapshotLut>> {
    let table_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master \
         WHERE type = 'table' AND name = 'color_luts')",
    )
    .fetch_one(&mut *connection)
    .await?;
    if table_exists == 0 {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT id, filename, cube_size, size_bytes, sha256 \
         FROM color_luts ORDER BY filename",
    )
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SnapshotLut {
                id: row.try_get("id")?,
                filename: row.try_get("filename")?,
                cube_size: u32::try_from(row.try_get::<i64, _>("cube_size")?)
                    .context("invalid LUT cube size in snapshot database")?,
                size: u64::try_from(row.try_get::<i64, _>("size_bytes")?)
                    .context("invalid LUT byte size in snapshot database")?,
                sha256: row.try_get("sha256")?,
            })
        })
        .collect()
}

fn copy_snapshot_luts(source: &Path, destination: &Path, luts: &[SnapshotLut]) -> Result<()> {
    if luts.is_empty() {
        return Ok(());
    }
    let source_metadata = fs::symlink_metadata(source).context("missing LUT storage directory")?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_dir() {
        return Err(anyhow!("LUT storage is not a regular directory"));
    }
    fs::create_dir_all(destination)?;

    let mut copied = BTreeSet::new();
    for lut in luts {
        validate_lut_record(lut)?;
        if !copied.insert(lut.filename.clone()) {
            return Err(anyhow!("duplicate LUT filename in snapshot database"));
        }
        let source_file = source.join(&lut.filename);
        validate_lut_file(&source_file, lut)
            .with_context(|| format!("invalid referenced LUT file {}", lut.filename))?;

        let copied_file = destination.join(&lut.filename);
        fs::copy(&source_file, &copied_file)?;
        validate_lut_file(&copied_file, lut)
            .with_context(|| format!("copied LUT failed verification: {}", lut.filename))?;
    }
    Ok(())
}

fn verify_lut_references(
    snapshot: &Path,
    manifest: &BackupManifest,
    luts: &[SnapshotLut],
) -> Result<()> {
    let mut declared = BTreeMap::new();
    let mut declared_luts = BTreeSet::new();
    for file in &manifest.files {
        let path = safe_relative_path(&file.path)?;
        if path.starts_with("luts") {
            declared_luts.insert(path.clone());
        }
        declared.insert(path, file);
    }

    let mut referenced_luts = BTreeSet::new();
    for lut in luts {
        validate_lut_record(lut)?;
        let path = Path::new("luts").join(&lut.filename);
        if !referenced_luts.insert(path.clone()) {
            return Err(anyhow!("duplicate LUT filename in snapshot database"));
        }
        let file = declared.get(&path).ok_or_else(|| {
            anyhow!(
                "snapshot database references missing declared LUT file: {}",
                lut.filename
            )
        })?;
        if file.size != lut.size {
            return Err(anyhow!(
                "LUT size does not match snapshot database: {}",
                lut.filename
            ));
        }
        if file.sha256 != lut.sha256 {
            return Err(anyhow!(
                "LUT checksum does not match snapshot database: {}",
                lut.filename
            ));
        }
        validate_lut_file(&snapshot.join(&path), lut)
            .with_context(|| format!("invalid snapshot LUT file {}", lut.filename))?;
    }
    if declared_luts != referenced_luts {
        return Err(anyhow!("snapshot contains an orphan declared LUT file"));
    }
    Ok(())
}

fn validate_lut_record(lut: &SnapshotLut) -> Result<()> {
    let id = Uuid::parse_str(&lut.id).context("invalid LUT UUID in snapshot database")?;
    if id.to_string() != lut.id {
        return Err(anyhow!("non-canonical LUT UUID in snapshot database"));
    }
    if lut.filename != format!("{}.cube", lut.id) || !plain_filename(&lut.filename) {
        return Err(anyhow!(
            "LUT filename does not match its database ID: {}",
            lut.filename
        ));
    }
    if !(2..=MAX_CUBE_SIZE as u32).contains(&lut.cube_size) {
        return Err(anyhow!("invalid LUT cube size in snapshot database"));
    }
    if lut.size > MAX_LUT_FILE_BYTES as u64 {
        return Err(anyhow!("LUT exceeds snapshot byte limit"));
    }
    Ok(())
}

fn validate_lut_file(path: &Path, lut: &SnapshotLut) -> Result<()> {
    validate_lut_record(lut)?;
    let metadata = fs::symlink_metadata(path).context("missing referenced LUT file")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!("referenced LUT is not a regular file"));
    }
    if metadata.len() != lut.size {
        return Err(anyhow!("referenced LUT size does not match database"));
    }
    let bytes = fs::read(path)?;
    if bytes.len() > MAX_LUT_FILE_BYTES || hash_bytes(&bytes) != lut.sha256 {
        return Err(anyhow!("referenced LUT checksum does not match database"));
    }
    let parsed = parse_cube(&bytes).context("referenced LUT is not a valid 3D CUBE")?;
    if parsed.cube_size != lut.cube_size {
        return Err(anyhow!("referenced LUT cube size does not match database"));
    }
    if parsed.canonical != bytes {
        return Err(anyhow!("referenced LUT is not canonical"));
    }
    if parsed.canonical.len() as u64 != lut.size {
        return Err(anyhow!("canonical LUT size does not match database"));
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

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
    use crate::luts::LutAsset;

    const LUT_ID: &str = "5b0dce8f-9e20-42d8-9eb4-b4f2bd0efcab";
    const LUT_FILENAME: &str = "5b0dce8f-9e20-42d8-9eb4-b4f2bd0efcab.cube";
    const ORPHAN_FILENAME: &str = "07619291-14be-4e9c-a70f-a626c2af11a0.cube";

    fn canonical_cube() -> Vec<u8> {
        parse_cube(
            b"LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n\
              0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n",
        )
        .unwrap()
        .canonical
    }

    fn altered_canonical_cube() -> Vec<u8> {
        let mut bytes = canonical_cube();
        let value = bytes
            .iter_mut()
            .rev()
            .find(|byte| **byte == b'1')
            .expect("fixture has a data value");
        *value = b'0';
        assert_eq!(parse_cube(&bytes).unwrap().canonical, bytes);
        bytes
    }

    async fn add_lut_bytes(
        db: &Db,
        storage: &Path,
        id: &str,
        cube_size: u32,
        bytes: &[u8],
    ) -> LutAsset {
        tokio::fs::create_dir_all(storage.join("luts"))
            .await
            .unwrap();
        let filename = format!("{id}.cube");
        let path = storage.join("luts").join(&filename);
        tokio::fs::write(&path, bytes).await.unwrap();
        let asset = LutAsset::new(
            id.to_owned(),
            filename.to_owned(),
            filename.to_owned(),
            cube_size,
            bytes.len() as u64,
            hash_bytes(bytes),
            1,
        );
        let (stored, created) = db.insert_or_get_lut(&asset).await.unwrap();
        assert!(created);
        assert_eq!(stored, asset);
        asset
    }

    async fn add_lut(db: &Db, storage: &Path) -> LutAsset {
        let bytes = canonical_cube();
        add_lut_bytes(db, storage, LUT_ID, 2, &bytes).await
    }

    async fn source_with_lut() -> (tempfile::TempDir, Db) {
        let source = tempfile::tempdir().unwrap();
        tokio::fs::write(source.path().join("library.json"), b"[]")
            .await
            .unwrap();
        let db = Db::open(source.path()).await.unwrap();
        add_lut(&db, source.path()).await;
        (source, db)
    }

    async fn valid_lut_snapshot() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
        let (source, db) = source_with_lut().await;
        let backup_root = tempfile::tempdir().unwrap();
        let snapshot = create_snapshot(source.path(), backup_root.path(), &db)
            .await
            .unwrap();
        (source, backup_root, snapshot)
    }

    fn rewrite_manifest(snapshot: &Path, update: impl FnOnce(&mut BackupManifest)) {
        let path = snapshot.join(MANIFEST_NAME);
        let mut manifest: BackupManifest =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        update(&mut manifest);
        manifest
            .files
            .sort_by(|left, right| left.path.cmp(&right.path));
        manifest.root_hash = hash_manifest_files(&manifest.files);
        fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    async fn replace_snapshot_lut_and_metadata(snapshot: &Path, bytes: &[u8]) {
        let lut_path = snapshot.join("luts").join(LUT_FILENAME);
        fs::write(&lut_path, bytes).unwrap();
        let lut_hash = hash_bytes(bytes);
        let options = SqliteConnectOptions::new()
            .filename(snapshot.join("app.db"))
            .disable_statement_logging();
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        let updated = sqlx::query("UPDATE color_luts SET size_bytes = ?, sha256 = ? WHERE id = ?")
            .bind(bytes.len() as i64)
            .bind(&lut_hash)
            .bind(LUT_ID)
            .execute(&mut connection)
            .await
            .unwrap();
        assert_eq!(updated.rows_affected(), 1);
        drop(connection);

        let database_path = snapshot.join("app.db");
        let database_size = fs::metadata(&database_path).unwrap().len();
        let database_hash = hash_file(&database_path).unwrap();
        rewrite_manifest(snapshot, |manifest| {
            for file in &mut manifest.files {
                if file.path == "app.db" {
                    file.size = database_size;
                    file.sha256 = database_hash.clone();
                } else if file.path == format!("luts/{LUT_FILENAME}") {
                    file.size = bytes.len() as u64;
                    file.sha256 = lut_hash.clone();
                }
            }
        });
    }

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
        add_lut(&db, source.path()).await;
        tokio::fs::write(
            source.path().join("luts").join(ORPHAN_FILENAME),
            b"not referenced",
        )
        .await
        .unwrap();
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
        assert!(manifest
            .files
            .iter()
            .any(|file| file.path == format!("luts/{LUT_FILENAME}")));
        assert!(!manifest
            .files
            .iter()
            .any(|file| file.path == format!("luts/{ORPHAN_FILENAME}")));
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
        assert_eq!(
            fs::read(restored.join("luts").join(LUT_FILENAME)).unwrap(),
            canonical_cube()
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

    #[tokio::test]
    async fn snapshot_creation_rejects_missing_referenced_lut() {
        let (source, db) = source_with_lut().await;
        tokio::fs::remove_file(source.path().join("luts").join(LUT_FILENAME))
            .await
            .unwrap();
        let backup_root = tempfile::tempdir().unwrap();

        let error = create_snapshot(source.path(), backup_root.path(), &db)
            .await
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("missing referenced LUT file"),
            "{error:#}"
        );
        assert!(fs::read_dir(backup_root.path()).unwrap().next().is_none());
    }

    #[tokio::test]
    async fn snapshot_creation_rejects_corrupt_referenced_lut() {
        let (source, db) = source_with_lut().await;
        let corrupt = altered_canonical_cube();
        tokio::fs::write(source.path().join("luts").join(LUT_FILENAME), corrupt)
            .await
            .unwrap();
        let backup_root = tempfile::tempdir().unwrap();

        let error = create_snapshot(source.path(), backup_root.path(), &db)
            .await
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("checksum does not match database"),
            "{error:#}"
        );
        assert!(fs::read_dir(backup_root.path()).unwrap().next().is_none());
    }

    #[tokio::test]
    async fn snapshot_creation_rejects_semantically_invalid_lut() {
        let source = tempfile::tempdir().unwrap();
        tokio::fs::write(source.path().join("library.json"), b"[]")
            .await
            .unwrap();
        let db = Db::open(source.path()).await.unwrap();
        let malformed = b"LUT_3D_SIZE 2\n0 0 0\n";
        add_lut_bytes(&db, source.path(), LUT_ID, 2, malformed).await;
        let backup_root = tempfile::tempdir().unwrap();

        let error = create_snapshot(source.path(), backup_root.path(), &db)
            .await
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("not a valid 3D CUBE"),
            "{error:#}"
        );
        assert!(fs::read_dir(backup_root.path()).unwrap().next().is_none());
    }

    #[tokio::test]
    async fn verify_rejects_database_lut_missing_from_manifest() {
        let (_source, _backup_root, snapshot) = valid_lut_snapshot().await;
        let lut_manifest_path = format!("luts/{LUT_FILENAME}");
        fs::remove_file(snapshot.join("luts").join(LUT_FILENAME)).unwrap();
        rewrite_manifest(&snapshot, |manifest| {
            manifest.files.retain(|file| file.path != lut_manifest_path);
        });

        let error = verify_snapshot(&snapshot).await.unwrap_err();

        assert!(
            error
                .to_string()
                .contains("database references missing declared LUT file"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn verify_rejects_corrupt_lut_even_when_manifest_matches_it() {
        let (_source, _backup_root, snapshot) = valid_lut_snapshot().await;
        let lut_path = snapshot.join("luts").join(LUT_FILENAME);
        fs::write(&lut_path, altered_canonical_cube()).unwrap();
        let corrupt_size = fs::metadata(&lut_path).unwrap().len();
        let corrupt_hash = hash_file(&lut_path).unwrap();
        rewrite_manifest(&snapshot, |manifest| {
            let lut = manifest
                .files
                .iter_mut()
                .find(|file| file.path == format!("luts/{LUT_FILENAME}"))
                .unwrap();
            lut.size = corrupt_size;
            lut.sha256 = corrupt_hash;
        });

        let error = verify_snapshot(&snapshot).await.unwrap_err();

        assert!(
            error
                .to_string()
                .contains("checksum does not match snapshot database"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn verify_rejects_semantically_invalid_lut_with_matching_metadata() {
        let (_source, _backup_root, snapshot) = valid_lut_snapshot().await;
        replace_snapshot_lut_and_metadata(&snapshot, b"LUT_3D_SIZE 2\n0 0 0\n").await;

        let error = verify_snapshot(&snapshot).await.unwrap_err();

        assert!(
            format!("{error:#}").contains("not a valid 3D CUBE"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn verify_rejects_orphan_declared_lut() {
        let (_source, _backup_root, snapshot) = valid_lut_snapshot().await;
        let orphan_path = snapshot.join("luts").join(ORPHAN_FILENAME);
        fs::write(&orphan_path, b"orphan lut").unwrap();
        let orphan = BackupFile {
            path: format!("luts/{ORPHAN_FILENAME}"),
            size: fs::metadata(&orphan_path).unwrap().len(),
            sha256: hash_file(&orphan_path).unwrap(),
        };
        rewrite_manifest(&snapshot, |manifest| manifest.files.push(orphan));

        let error = verify_snapshot(&snapshot).await.unwrap_err();

        assert!(
            error.to_string().contains("orphan declared LUT file"),
            "{error:#}"
        );
    }

    #[test]
    fn restore_paths_cannot_escape_the_target() {
        assert!(safe_relative_path("sources/video.mp4").is_ok());
        for path in ["../app.db", "/tmp/app.db", "sources/../app.db", ""] {
            assert!(safe_relative_path(path).is_err(), "{path:?}");
        }
    }
}
