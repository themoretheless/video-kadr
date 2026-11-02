//! Lossless Filmcraft source-file decoding and schema migration.
//! This retains the authoring document; it does not admit it as a renderable
//! native Composition. Sequence/media/effect conversion is a separate stage.
//! Pinned original: 5231852443363f001c3f6b396dd9b1e6461ae2be (MIT).
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

pub const FORMAT_ID: &str = "filmcraft.project";
pub const SCHEMA_VERSION: u32 = 12;

#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    items: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    root: Option<serde::de::IgnoredAny>,
}

pub fn detect_schema(document: &Value) -> Result<u32> {
    let probe: Probe = serde_json::from_value(document.clone())?;
    if let Some(version) = probe.schema_version {
        ensure!(
            probe
                .format
                .as_deref()
                .is_none_or(|format| format == FORMAT_ID),
            "not a Filmcraft project format"
        );
        return Ok(version);
    }
    if probe.items.is_some() && probe.root.is_some() {
        return Ok(match probe.version.unwrap_or(1) {
            0..=2 => 1,
            version => version,
        });
    }
    bail!("no schema_version and not a legacy Filmcraft project")
}

/// Preserve every project, view and extension field through source migrations.
/// Future schemas are refused, never overwritten with a partial interpretation.
pub fn migrate_document(mut document: Value) -> Result<(u32, Value)> {
    let found = detect_schema(&document)?;
    ensure!(found >= 1, "Filmcraft project schema is too old");
    ensure!(
        found <= SCHEMA_VERSION,
        "Filmcraft project schema is too new"
    );
    if found == 1 {
        let project = document
            .as_object_mut()
            .context("expected legacy project object")?;
        project.remove("version");
        document = serde_json::json!({"format":FORMAT_ID,"schema_version":2,
            "generator":"FilmCraft (schema v1)","project":document});
    }
    // Original v2..v12 migrations are no-ops except for the envelope stamp.
    if found < SCHEMA_VERSION {
        document
            .as_object_mut()
            .context("expected Filmcraft project envelope")?
            .insert("schema_version".into(), SCHEMA_VERSION.into());
    }
    Ok((found, document))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceProjectDocument {
    pub original_schema: u32,
    /// Migrated source JSON, including opaque fields not supported by the editor.
    pub envelope: Value,
}
impl SourceProjectDocument {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let raw: Value = serde_json::from_slice(bytes).context("damaged Filmcraft project JSON")?;
        let (original_schema, envelope) = migrate_document(raw)?;
        ensure!(
            envelope.get("project").is_some_and(Value::is_object),
            "missing Filmcraft project object"
        );
        ensure!(
            envelope
                .get("generator")
                .is_none_or(|v| v.is_null() || v.is_string()),
            "invalid Filmcraft generator"
        );
        Ok(Self {
            original_schema,
            envelope,
        })
    }
    pub fn project(&self) -> &Value {
        &self.envelope["project"]
    }
    pub fn migrated(&self) -> bool {
        self.original_schema < SCHEMA_VERSION
    }
    pub fn encode_preserved(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&self.envelope)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_version_detection_and_migrations_preserve_every_field() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-project-file.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 31);
        for (i, case) in cases.iter().enumerate() {
            let raw = &case["document"];
            if case.get("found").is_some() {
                assert_eq!(
                    detect_schema(raw).unwrap(),
                    case["found"].as_u64().unwrap() as u32,
                    "probe {i}"
                );
            } else {
                assert!(detect_schema(raw).is_err(), "probe {i}");
            }
            if let Some(expected) = case.get("migrated") {
                let (found, migrated) = migrate_document(raw.clone()).unwrap();
                assert_eq!(migrated, *expected, "migration {i}");
                if !expected.get("project").is_some_and(Value::is_object) {
                    assert!(
                        SourceProjectDocument::read(&serde_json::to_vec(raw).unwrap()).is_err()
                    );
                    continue;
                }
                let loaded =
                    SourceProjectDocument::read(&serde_json::to_vec(raw).unwrap()).unwrap();
                assert_eq!(loaded.original_schema, found);
                assert_eq!(loaded.envelope, migrated);
                let restored =
                    SourceProjectDocument::read(&loaded.encode_preserved().unwrap()).unwrap();
                assert_eq!(restored.envelope, loaded.envelope);
            } else {
                assert!(migrate_document(raw.clone()).is_err(), "migration {i}");
            }
        }
        assert!(SourceProjectDocument::read(b"{broken").is_err());
        assert!(SourceProjectDocument::read(br#"{"schema_version":12,"project":null}"#).is_err());
        assert!(SourceProjectDocument::read(
            br#"{"schema_version":12,"project":{},"generator":7}"#
        )
        .is_err());
    }
}
