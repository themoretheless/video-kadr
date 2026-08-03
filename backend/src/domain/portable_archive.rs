//! Pure validation boundary for a portable per-project archive manifest.
//! Container decoding/extraction must happen behind separate bounded I/O APIs.

use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::project::ProjectDocument;

pub const PORTABLE_ARCHIVE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct ArchiveLimits {
    pub max_entries: u32,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveEntryKind {
    Project,
    Media,
    Lut,
    Proxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyProvenance {
    pub source_sha256: String,
    pub profile_fingerprint: String,
    pub renderer_compatibility: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableArchiveEntry {
    pub path: String,
    pub kind: ArchiveEntryKind,
    pub size_bytes: u64,
    pub sha256: String,
    pub proxy_provenance: Option<ProxyProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableArchiveManifest {
    pub schema_version: u32,
    pub project: Value,
    pub entries: Vec<PortableArchiveEntry>,
    pub root_hash: String,
}

impl PortableArchiveManifest {
    pub fn validate(&self, limits: ArchiveLimits) -> Result<ProjectDocument> {
        if self.schema_version != PORTABLE_ARCHIVE_SCHEMA_VERSION {
            return Err(anyhow!("unsupported portable archive schema"));
        }
        if limits.max_entries == 0 || limits.max_entry_bytes == 0 || limits.max_total_bytes == 0 {
            return Err(anyhow!("portable archive limits must be positive"));
        }
        if self.entries.len() > limits.max_entries as usize {
            return Err(anyhow!("portable archive has too many entries"));
        }
        let project = ProjectDocument::migrate(self.project.clone())?;
        project.validate()?;

        let mut paths = BTreeSet::new();
        let mut folded_paths = BTreeSet::new();
        let mut total = 0_u64;
        let mut project_entries = 0_u32;
        for entry in &self.entries {
            validate_sha256(&entry.sha256)?;
            validate_entry_path(entry)?;
            if entry.size_bytes == 0 || entry.size_bytes > limits.max_entry_bytes {
                return Err(anyhow!("portable archive entry exceeds byte limit"));
            }
            total = total
                .checked_add(entry.size_bytes)
                .ok_or_else(|| anyhow!("portable archive total size overflow"))?;
            if total > limits.max_total_bytes {
                return Err(anyhow!("portable archive exceeds total byte limit"));
            }
            if !paths.insert(entry.path.clone())
                || !folded_paths.insert(entry.path.to_ascii_lowercase())
            {
                return Err(anyhow!("duplicate portable archive path"));
            }
            if entry.kind == ArchiveEntryKind::Project {
                project_entries += 1;
            }
            validate_provenance(entry)?;
        }
        if project_entries != 1 {
            return Err(anyhow!("portable archive needs exactly one project entry"));
        }
        for path in &paths {
            let mut prefix = String::new();
            for component in path.split('/').take(path.split('/').count() - 1) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(component);
                if paths.contains(&prefix) {
                    return Err(anyhow!("portable archive file/directory path collision"));
                }
            }
        }
        if self.root_hash != root_hash(self.schema_version, &self.project, &self.entries)? {
            return Err(anyhow!("portable archive root hash mismatch"));
        }
        Ok(project)
    }
}

pub fn root_hash(
    schema_version: u32,
    project: &Value,
    entries: &[PortableArchiveEntry],
) -> Result<String> {
    let mut ordered = entries.to_vec();
    ordered.sort_by(|a, b| a.path.cmp(&b.path));
    let project = canonical_json(project)?;
    let entries =
        canonical_json(&serde_json::to_value(&ordered).expect("entry serialization cannot fail"))?;
    let mut hasher = Sha256::new();
    for part in [
        b"portable-project-v1".as_slice(),
        schema_version.to_be_bytes().as_slice(),
        project.as_slice(),
        entries.as_slice(),
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Cross-language stable JSON: object keys are recursively sorted, arrays keep
/// order, and scalar encoding follows JSON's compact representation.
fn canonical_json(value: &Value) -> Result<Vec<u8>> {
    fn write(value: &Value, output: &mut Vec<u8>) -> Result<()> {
        match value {
            Value::Object(object) => {
                output.push(b'{');
                let mut keys: Vec<_> = object.keys().collect();
                keys.sort_by_key(|key| key.encode_utf16().collect::<Vec<_>>());
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    output.extend(
                        serde_json::to_vec(key).expect("JSON key serialization cannot fail"),
                    );
                    output.push(b':');
                    write(&object[key], output)?;
                }
                output.push(b'}');
            }
            Value::Array(values) => {
                output.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    write(value, output)?;
                }
                output.push(b']');
            }
            Value::Number(number) => {
                const MAX_SAFE: u64 = 9_007_199_254_740_991;
                let value = if let Some(value) = number.as_i64() {
                    if value.unsigned_abs() > MAX_SAFE {
                        return Err(anyhow!("portable archive number exceeds JS safe integer"));
                    }
                    value as f64
                } else if let Some(value) = number.as_u64() {
                    if value > MAX_SAFE {
                        return Err(anyhow!("portable archive number exceeds JS safe integer"));
                    }
                    value as f64
                } else {
                    number
                        .as_f64()
                        .ok_or_else(|| anyhow!("invalid portable archive number"))?
                };
                let bits = if value == 0.0 { 0 } else { value.to_bits() };
                output.extend(format!("~{bits:016x}").as_bytes());
            }
            scalar => output
                .extend(serde_json::to_vec(scalar).expect("JSON scalar serialization cannot fail")),
        }
        Ok(())
    }
    let mut output = Vec::new();
    write(value, &mut output)?;
    Ok(output)
}

fn validate_entry_path(entry: &PortableArchiveEntry) -> Result<()> {
    if entry.path.is_empty()
        || entry.path.len() > 512
        || entry.path.contains('\\')
        || entry
            .path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !entry.path.is_ascii()
    {
        return Err(anyhow!("invalid portable archive path"));
    }
    let valid = match entry.kind {
        ArchiveEntryKind::Project => entry.path == "project.json",
        ArchiveEntryKind::Media => content_path(&entry.path, "media", &entry.sha256, None),
        ArchiveEntryKind::Lut => content_path(&entry.path, "luts", &entry.sha256, Some("cube")),
        ArchiveEntryKind::Proxy => {
            let parts: Vec<_> = entry.path.split('/').collect();
            parts.len() == 3
                && parts[0] == "proxies"
                && is_safe_token(parts[1])
                && parts[2].split_once('.').is_some_and(|(stem, extension)| {
                    stem == entry.sha256 && is_safe_extension(extension)
                })
        }
    };
    if valid {
        Ok(())
    } else {
        Err(anyhow!("portable archive path does not match its role"))
    }
}

fn content_path(path: &str, directory: &str, sha256: &str, extension: Option<&str>) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 2 || parts[0] != directory {
        return false;
    }
    match extension {
        Some(extension) => parts[1] == format!("{sha256}.{extension}"),
        None => parts[1]
            .split_once('.')
            .is_some_and(|(stem, ext)| stem == sha256 && is_safe_extension(ext)),
    }
}

fn validate_provenance(entry: &PortableArchiveEntry) -> Result<()> {
    match (&entry.kind, &entry.proxy_provenance) {
        (ArchiveEntryKind::Proxy, Some(provenance)) => {
            validate_sha256(&provenance.source_sha256)?;
            validate_sha256(&provenance.profile_fingerprint)?;
            if !is_safe_token(&provenance.renderer_compatibility) {
                return Err(anyhow!("invalid proxy renderer compatibility"));
            }
            Ok(())
        }
        (ArchiveEntryKind::Proxy, None) => Err(anyhow!("portable proxy lacks provenance")),
        (_, None) => Ok(()),
        (_, Some(_)) => Err(anyhow!("proxy provenance is forbidden for this entry role")),
    }
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(anyhow!("invalid portable archive SHA-256"))
    }
}

fn is_safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn is_safe_extension(value: &str) -> bool {
    !value.is_empty() && value.len() <= 12 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sha(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn project() -> Value {
        json!({
            "schemaVersion": 1,
            "videoId": "video-1",
            "name": "Portable",
            "video": {"id":"video-1","duration":1.0,"width":16,"height":16},
            "edit": {}
        })
    }

    fn manifest() -> PortableArchiveManifest {
        let project = project();
        let entries = vec![
            PortableArchiveEntry {
                path: "project.json".into(),
                kind: ArchiveEntryKind::Project,
                size_bytes: 100,
                sha256: sha(1),
                proxy_provenance: None,
            },
            PortableArchiveEntry {
                path: format!("media/{}.mp4", sha(2)),
                kind: ArchiveEntryKind::Media,
                size_bytes: 200,
                sha256: sha(2),
                proxy_provenance: None,
            },
        ];
        PortableArchiveManifest {
            schema_version: PORTABLE_ARCHIVE_SCHEMA_VERSION,
            root_hash: root_hash(PORTABLE_ARCHIVE_SCHEMA_VERSION, &project, &entries).unwrap(),
            project,
            entries,
        }
    }

    fn limits() -> ArchiveLimits {
        ArchiveLimits {
            max_entries: 8,
            max_entry_bytes: 1_000,
            max_total_bytes: 2_000,
        }
    }

    fn resign(manifest: &mut PortableArchiveManifest) {
        manifest.root_hash = root_hash(
            manifest.schema_version,
            &manifest.project,
            &manifest.entries,
        )
        .unwrap();
    }

    #[test]
    fn root_hash_recursively_canonicalizes_json_object_keys() {
        let left: Value = serde_json::from_str(r#"{"z":{"b":2,"a":1},"a":0}"#).unwrap();
        let right: Value = serde_json::from_str(r#"{"a":0,"z":{"a":1,"b":2}}"#).unwrap();
        assert_eq!(
            root_hash(1, &left, &[]).unwrap(),
            root_hash(1, &right, &[]).unwrap()
        );
    }

    #[test]
    fn numeric_canonicalization_matches_javascript_number_semantics() {
        let integer = json!({"n": 1});
        let float = json!({"n": 1.0});
        let negative_zero: Value = serde_json::from_str(r#"{"n":-0.0}"#).unwrap();
        let zero = json!({"n": 0});
        assert_eq!(
            root_hash(1, &integer, &[]).unwrap(),
            root_hash(1, &float, &[]).unwrap()
        );
        assert_eq!(
            root_hash(1, &negative_zero, &[]).unwrap(),
            root_hash(1, &zero, &[]).unwrap()
        );
        let unsafe_integer = json!({"n": 9_007_199_254_740_992_u64});
        assert!(root_hash(1, &unsafe_integer, &[]).is_err());
    }

    #[test]
    fn valid_manifest_migrates_and_validates_project() {
        let document = manifest().validate(limits()).unwrap();
        assert_eq!(document.name, "Portable");
    }

    #[test]
    fn paths_are_role_bound_and_reject_escape_case_aliases_and_prefix_collisions() {
        for bad in ["../evil", "/absolute", "media\\evil", "media//evil"] {
            let mut value = manifest();
            value.entries[1].path = bad.into();
            resign(&mut value);
            assert!(value.validate(limits()).is_err(), "accepted {bad}");
        }
        let mut value = manifest();
        value.entries.push(PortableArchiveEntry {
            path: "PROJECT.JSON".into(),
            kind: ArchiveEntryKind::Project,
            size_bytes: 1,
            sha256: sha(3),
            proxy_provenance: None,
        });
        resign(&mut value);
        assert!(value.validate(limits()).is_err());
    }

    #[test]
    fn quotas_use_checked_totals_and_root_hash_binds_manifest() {
        let mut value = manifest();
        value.entries[1].size_bytes = u64::MAX;
        assert!(value
            .validate(ArchiveLimits {
                max_entry_bytes: u64::MAX,
                max_total_bytes: u64::MAX,
                ..limits()
            })
            .is_err());
        let mut value = manifest();
        value.entries[1].size_bytes += 1;
        assert!(value.validate(limits()).is_err());
    }

    #[test]
    fn future_project_and_archive_schemas_fail_closed() {
        let mut value = manifest();
        value.schema_version += 1;
        resign(&mut value);
        assert!(value.validate(limits()).is_err());
        let mut value = manifest();
        value.project["schemaVersion"] = json!(999);
        resign(&mut value);
        assert!(value.validate(limits()).is_err());
    }

    #[test]
    fn proxy_requires_complete_content_bound_provenance() {
        let mut value = manifest();
        value.entries.push(PortableArchiveEntry {
            path: format!("proxies/web/{}.mp4", sha(4)),
            kind: ArchiveEntryKind::Proxy,
            size_bytes: 10,
            sha256: sha(4),
            proxy_provenance: None,
        });
        resign(&mut value);
        assert!(value.validate(limits()).is_err());
        value.entries.last_mut().unwrap().proxy_provenance = Some(ProxyProvenance {
            source_sha256: sha(2),
            profile_fingerprint: sha(5),
            renderer_compatibility: "ffmpeg-v1".into(),
        });
        resign(&mut value);
        assert!(value.validate(limits()).is_ok());
    }
}
