//! Versioned project-document migration and forward-compatibility policy.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

pub const LATEST_PROJECT_VERSION: u64 = 3;

const KNOWN_OPERATIONS: &[&str] = &[
    "clip.add",
    "clip.move",
    "clip.remove",
    "effect.set",
    "track.add",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub document: Value,
    pub preserved_optional_operations: BTreeSet<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProjectMigrationError {
    InvalidRoot,
    InvalidVersion,
    UnsupportedVersion(u64),
    InvalidOperation(usize),
    UnknownRequiredOperation(String),
}

impl std::fmt::Display for ProjectMigrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRoot => formatter.write_str("project document must be a JSON object"),
            Self::InvalidVersion => {
                formatter.write_str("project schemaVersion is missing or invalid")
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "project schemaVersion {version} is not supported"
            ),
            Self::InvalidOperation(index) => write!(
                formatter,
                "operation at index {index} must be an object with a type"
            ),
            Self::UnknownRequiredOperation(kind) => {
                write!(formatter, "unknown required operation: {kind}")
            }
        }
    }
}

impl std::error::Error for ProjectMigrationError {}

/// Migrates a persisted project to the canonical latest representation.
///
/// Unknown optional operations are retained byte-for-byte so a newer editor
/// can still recover them. Unknown required operations fail closed: silently
/// skipping them could change the rendered result.
pub fn migrate_to_latest(document: Value) -> Result<MigrationReport, ProjectMigrationError> {
    let mut root = document
        .as_object()
        .cloned()
        .ok_or(ProjectMigrationError::InvalidRoot)?;
    let version = root
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(ProjectMigrationError::InvalidVersion)?;
    if version == 0 || version > LATEST_PROJECT_VERSION {
        return Err(ProjectMigrationError::UnsupportedVersion(version));
    }

    if version == 1 {
        let clips = root.remove("clips").unwrap_or_else(|| json!([]));
        root.insert("timeline".into(), json!({ "tracks": [{ "clips": clips }] }));
        root.insert("schemaVersion".into(), json!(2));
    }
    if version <= 2 {
        root.entry("operations")
            .or_insert_with(|| Value::Array(Vec::new()));
        root.insert("schemaVersion".into(), json!(LATEST_PROJECT_VERSION));
    }

    let preserved_optional_operations = validate_operations(&root)?;
    canonicalize(&mut root);
    Ok(MigrationReport {
        document: Value::Object(root),
        preserved_optional_operations,
    })
}

fn validate_operations(
    root: &Map<String, Value>,
) -> Result<BTreeSet<String>, ProjectMigrationError> {
    let Some(operations) = root.get("operations") else {
        return Ok(BTreeSet::new());
    };
    let operations = operations
        .as_array()
        .ok_or(ProjectMigrationError::InvalidOperation(0))?;
    let mut preserved = BTreeSet::new();
    for (index, operation) in operations.iter().enumerate() {
        let object = operation
            .as_object()
            .ok_or(ProjectMigrationError::InvalidOperation(index))?;
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or(ProjectMigrationError::InvalidOperation(index))?;
        if !KNOWN_OPERATIONS.contains(&kind) {
            let optional = object
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !optional {
                return Err(ProjectMigrationError::UnknownRequiredOperation(kind.into()));
            }
            preserved.insert(kind.into());
        }
    }
    Ok(preserved)
}

fn canonicalize(root: &mut Map<String, Value>) {
    root.entry("metadata").or_insert_with(|| json!({}));
    root.entry("timeline")
        .or_insert_with(|| json!({ "tracks": [] }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let source = match name {
            "v1" => include_str!("../../tests/fixtures/projects/v1.json"),
            "v2" => include_str!("../../tests/fixtures/projects/v2.json"),
            "latest" => include_str!("../../tests/fixtures/projects/latest.json"),
            "unknown-required" => {
                include_str!("../../tests/fixtures/projects/unknown-required.json")
            }
            "unknown-optional" => {
                include_str!("../../tests/fixtures/projects/unknown-optional.json")
            }
            _ => panic!("unknown fixture"),
        };
        serde_json::from_str(source).unwrap()
    }

    #[test]
    fn golden_versions_migrate_to_latest() {
        let expected = fixture("latest");
        for version in ["v1", "v2", "latest"] {
            assert_eq!(
                migrate_to_latest(fixture(version)).unwrap().document,
                expected
            );
        }
    }

    #[test]
    fn unknown_required_operations_fail_closed() {
        assert_eq!(
            migrate_to_latest(fixture("unknown-required")).unwrap_err(),
            ProjectMigrationError::UnknownRequiredOperation("future.magic".into())
        );
    }

    #[test]
    fn unknown_optional_operations_are_preserved() {
        let source = fixture("unknown-optional");
        let report = migrate_to_latest(source.clone()).unwrap();
        assert_eq!(report.document, source);
        assert_eq!(
            report.preserved_optional_operations,
            BTreeSet::from(["vendor.annotation".into()])
        );
    }
}
