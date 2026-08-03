//! Streaming VKADR v1 container codec.
//!
//! Layout: 8-byte magic, big-endian u32 manifest length, manifest JSON, then
//! entry payloads concatenated in manifest order. There are no filenames in
//! the byte stream outside the validated manifest and no compression layer.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, ensure, Context, Result};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::portable_archive::{ArchiveEntryKind, ArchiveLimits, PortableArchiveManifest};

pub const VKADR_MAGIC: &[u8; 8] = b"VKADRv1\n";
pub const MAX_VKADR_MANIFEST_BYTES: u32 = 16 * 1024 * 1024;

/// Write a container from an already validated manifest and confined payload
/// root. Files are opened one-by-one and copied without loading media in RAM.
pub fn write_vkadr(
    mut output: impl Write,
    manifest: &PortableArchiveManifest,
    payload_root: &Path,
    limits: ArchiveLimits,
) -> Result<()> {
    manifest.validate(limits)?;
    let manifest_bytes = serde_json::to_vec(manifest)?;
    ensure!(
        manifest_bytes.len() <= MAX_VKADR_MANIFEST_BYTES as usize,
        "VKADR manifest is too large"
    );
    output.write_all(VKADR_MAGIC)?;
    output.write_all(&(manifest_bytes.len() as u32).to_be_bytes())?;
    output.write_all(&manifest_bytes)?;
    for entry in &manifest.entries {
        let path = payload_root.join(validated_relative(&entry.path)?);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "VKADR payload is not a regular file"
        );
        ensure!(
            metadata.len() == entry.size_bytes,
            "VKADR source payload size mismatch"
        );
        let mut source = File::open(path)?;
        copy_exact_hash(&mut source, &mut output, entry.size_bytes, &entry.sha256)?;
        let mut extra = [0_u8; 1];
        ensure!(
            source.read(&mut extra)? == 0,
            "VKADR source payload grew during export"
        );
    }
    Ok(())
}

/// Decode into a fresh sibling staging directory, verify every byte, then
/// atomically rename it to `destination`. Destination must not exist.
pub fn stage_vkadr(
    mut input: impl Read,
    destination: &Path,
    limits: ArchiveLimits,
) -> Result<PortableArchiveManifest> {
    ensure!(!destination.exists(), "VKADR destination already exists");
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("VKADR destination needs a parent"))?;
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(".vkadr-import-{}", Uuid::new_v4()));
    fs::create_dir(&stage)?;
    let result = (|| -> Result<PortableArchiveManifest> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        ensure!(&magic == VKADR_MAGIC, "invalid VKADR magic");
        let mut length = [0_u8; 4];
        input.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length);
        ensure!(
            length > 0 && length <= MAX_VKADR_MANIFEST_BYTES,
            "invalid VKADR manifest length"
        );
        let mut bytes = vec![0_u8; length as usize];
        input.read_exact(&mut bytes)?;
        let manifest: PortableArchiveManifest = serde_json::from_slice(&bytes)?;
        manifest.validate(limits)?;
        for entry in &manifest.entries {
            let relative = validated_relative(&entry.path)?;
            let path = stage.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            copy_exact_hash(&mut input, &mut file, entry.size_bytes, &entry.sha256)
                .with_context(|| format!("verify VKADR entry {}", entry.path))?;
            file.sync_all()?;
        }
        let mut trailing = [0_u8; 1];
        ensure!(
            input.read(&mut trailing)? == 0,
            "VKADR contains trailing undeclared bytes"
        );
        let project = manifest
            .entries
            .iter()
            .find(|entry| entry.kind == ArchiveEntryKind::Project)
            .unwrap();
        let project_bytes = fs::read(stage.join(validated_relative(&project.path)?))?;
        let project_value: serde_json::Value = serde_json::from_slice(&project_bytes)?;
        ensure!(
            project_value == manifest.project,
            "VKADR project payload differs from manifest project"
        );
        Ok(manifest)
    })();
    match result {
        Ok(manifest) => {
            fs::rename(&stage, destination).context("publish VKADR staging directory")?;
            Ok(manifest)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&stage);
            Err(error)
        }
    }
}

fn copy_exact_hash(
    input: &mut impl Read,
    output: &mut impl Write,
    size: u64,
    expected: &str,
) -> Result<()> {
    let mut remaining = size;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let chunk = remaining.min(buffer.len() as u64) as usize;
        let count = input.read(&mut buffer[..chunk])?;
        ensure!(count > 0, "truncated VKADR payload");
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    ensure!(
        format!("{:x}", hash.finalize()) == expected,
        "VKADR payload checksum mismatch"
    );
    Ok(())
}

fn validated_relative(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    ensure!(
        !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "unsafe VKADR path"
    );
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::portable_archive::{
        root_hash, ArchiveEntryKind, PortableArchiveEntry, PORTABLE_ARCHIVE_SCHEMA_VERSION,
    };
    use serde_json::json;
    use std::io::Cursor;

    fn limits() -> ArchiveLimits {
        ArchiveLimits {
            max_entries: 4,
            max_entry_bytes: 4096,
            max_total_bytes: 8192,
        }
    }
    fn fixture() -> (PortableArchiveManifest, Vec<u8>) {
        let project =
            json!({"schemaVersion":1,"videoId":"v","video":{"id":"v","duration":1},"edit":{}});
        let bytes = serde_json::to_vec(&project).unwrap();
        let entry = PortableArchiveEntry {
            path: "project.json".into(),
            kind: ArchiveEntryKind::Project,
            size_bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            proxy_provenance: None,
        };
        let entries = vec![entry];
        let manifest = PortableArchiveManifest {
            schema_version: PORTABLE_ARCHIVE_SCHEMA_VERSION,
            root_hash: root_hash(PORTABLE_ARCHIVE_SCHEMA_VERSION, &project, &entries).unwrap(),
            project,
            entries,
        };
        (manifest, bytes)
    }

    #[test]
    fn round_trip_and_reject_corruption_trailing_and_quota() {
        let (manifest, project) = fixture();
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("project.json"), project).unwrap();
        let mut archive = Vec::new();
        write_vkadr(&mut archive, &manifest, source.path(), limits()).unwrap();
        let target_root = tempfile::tempdir().unwrap();
        let target = target_root.path().join("imported");
        assert_eq!(
            stage_vkadr(Cursor::new(&archive), &target, limits())
                .unwrap()
                .root_hash,
            manifest.root_hash
        );

        let mut corrupt = archive.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(stage_vkadr(
            Cursor::new(corrupt),
            &target_root.path().join("bad"),
            limits()
        )
        .is_err());
        let mut trailing = archive.clone();
        trailing.push(0);
        assert!(stage_vkadr(
            Cursor::new(trailing),
            &target_root.path().join("trailing"),
            limits()
        )
        .is_err());
        assert!(stage_vkadr(
            Cursor::new(archive),
            &target_root.path().join("quota"),
            ArchiveLimits {
                max_total_bytes: 1,
                ..limits()
            }
        )
        .is_err());
    }

    #[test]
    fn exact_vkadr_v1_bytes_match_shared_interop_golden() {
        let (manifest, project) = fixture();
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("project.json"), project).unwrap();
        let mut archive = Vec::new();
        write_vkadr(&mut archive, &manifest, source.path(), limits()).unwrap();
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/portable-archive/v1-golden.json"
        ))
        .unwrap();
        let hex = archive
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(manifest.project, golden["project"]);
        assert_eq!(manifest.root_hash, golden["rootHash"]);
        assert_eq!(hex, golden["vkadrHex"]);
    }
}
