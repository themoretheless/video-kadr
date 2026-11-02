//! Portable imported LUT library shared by every composition in a native project.
use super::color_lut::ColorLut;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Filmcraft references select either a builtin or a portable project table.
/// The parsed table is shared through the existing content-addressed cache.
pub fn resolve_reference(
    entries: &[ProjectLut],
    reference: &str,
) -> Result<Option<Arc<super::lut_tables::Lut>>> {
    ensure!(reference.len() <= 136, "LUT reference too long");
    if reference.is_empty() {
        return Ok(None);
    }
    if let Some(id) = reference.strip_prefix("builtin:") {
        return super::builtin_luts::resolve(id).map(Some);
    }
    if let Some(id) = reference.strip_prefix("lib:") {
        ensure!(!id.is_empty(), "empty project LUT reference");
        let mut matches = entries.iter().filter(|entry| entry.id == id);
        let entry = matches
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing project LUT: {id}"))?;
        ensure!(matches.next().is_none(), "ambiguous project LUT reference");
        ensure!(
            entry.lut.intensity == 100.0,
            "project LUT intensity belongs to effect"
        );
        return entry.lut.table().map(Some);
    }
    anyhow::bail!("unknown LUT reference namespace")
}
/// Effects retain orphan IDs after library removal; Filmcraft bypasses missing tables.
pub fn resolve_effect_reference(
    entries: &[ProjectLut],
    reference: &str,
) -> Result<Option<Arc<super::lut_tables::Lut>>> {
    if let Some(id) = reference.strip_prefix("lib:") {
        ensure!(
            !id.trim().is_empty() && id.len() <= 128,
            "invalid project LUT reference"
        );
        if entries.iter().all(|entry| entry.id != id) {
            return Ok(None);
        }
    }
    resolve_reference(entries, reference)
}
pub fn remove(entries: &mut Vec<ProjectLut>, id: &str) -> Result<()> {
    validate(entries)?;
    let id = id.strip_prefix("lib:").unwrap_or(id);
    ensure!(
        entries.iter().any(|entry| entry.id == id),
        "missing project LUT"
    );
    entries.retain(|entry| entry.id != id);
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectLut {
    pub id: String,
    pub name: String,
    pub lut: ColorLut,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}
/// Import identical text once, matching Filmcraft's project-library behavior.
pub fn import(
    entries: &mut Vec<ProjectLut>,
    lut: ColorLut,
    name: String,
    source_path: Option<String>,
) -> Result<(String, bool)> {
    import_reserving(entries, lut, name, source_path, &BTreeSet::new())
}
pub fn import_reserving(
    entries: &mut Vec<ProjectLut>,
    lut: ColorLut,
    name: String,
    source_path: Option<String>,
    reserved: &BTreeSet<String>,
) -> Result<(String, bool)> {
    validate(entries)?;
    let candidate = ProjectLut {
        id: "candidate".into(),
        name,
        lut,
        source_path,
    };
    validate(std::slice::from_ref(&candidate))?;
    if let Some(existing) = entries
        .iter()
        .find(|entry| entry.lut.text == candidate.lut.text)
    {
        return Ok((existing.id.clone(), true));
    }
    ensure!(entries.len() < 64, "too many project LUTs");
    let id = (1..=entries.len() + reserved.len() + 1)
        .map(|n| format!("lut{n}"))
        .find(|id| !reserved.contains(id) && entries.iter().all(|entry| &entry.id != id))
        .ok_or_else(|| anyhow::anyhow!("LUT id allocation failed"))?;
    let mut updated = entries.clone();
    updated.push(ProjectLut {
        id: id.clone(),
        ..candidate
    });
    validate(&updated)?;
    *entries = updated;
    Ok((id, false))
}
pub fn validate(entries: &[ProjectLut]) -> Result<()> {
    ensure!(entries.len() <= 64, "too many project LUTs");
    let mut ids = BTreeSet::new();
    let mut bytes = 0usize;
    for entry in entries {
        ensure!(
            !entry.id.trim().is_empty() && entry.id.len() <= 128 && ids.insert(&entry.id),
            "invalid or duplicate project LUT id"
        );
        ensure!(
            !entry.name.trim().is_empty() && entry.name.len() <= 1024,
            "invalid project LUT name"
        );
        ensure!(
            entry.source_path.as_ref().is_none_or(|p| p.len() <= 4096),
            "invalid LUT source path"
        );
        bytes = bytes
            .checked_add(entry.lut.text.len())
            .ok_or_else(|| anyhow::anyhow!("LUT library size overflow"))?;
        ensure!(
            bytes <= 64 * 1024 * 1024,
            "project LUT library exceeds64MiB"
        );
        ensure!(
            entry.lut.intensity == 100.0,
            "project LUT intensity belongs to effect"
        );
        entry.lut.validate()?;
    }
    Ok(())
}
pub fn update(
    entries: &mut Vec<ProjectLut>,
    id: &str,
    name: Option<String>,
    lut: Option<ColorLut>,
    source_path: Option<String>,
) -> Result<()> {
    validate(entries)?;
    ensure!(name.is_some() || lut.is_some(), "empty LUT update");
    ensure!(
        lut.is_some() || source_path.is_none(),
        "source path requires table replacement"
    );
    let mut next = entries.clone();
    let entry = next
        .iter_mut()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow::anyhow!("missing project LUT"))?;
    if let Some(name) = name {
        entry.name = name;
    }
    if let Some(lut) = lut {
        entry.lut = lut;
        entry.source_path = source_path;
    }
    validate(&next)?;
    *entries = next;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updating_a_shared_table_preserves_its_id_and_is_atomic() {
        let mut entries = vec![];
        let lut = ColorLut {
            format: super::super::color_lut::TableFormat::Cube,
            text: "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n".into(),
            intensity: 100.0,
        };
        let (id, _) = import(
            &mut entries,
            lut.clone(),
            "Identity".into(),
            Some("identity.cube".into()),
        )
        .unwrap();
        let first = resolve_reference(&entries, &format!("lib:{id}"))
            .unwrap()
            .unwrap();
        update(&mut entries, &id, Some("Renamed".into()), None, None).unwrap();
        assert_eq!(entries[0].id, id);
        assert_eq!(entries[0].source_path.as_deref(), Some("identity.cube"));
        assert!(Arc::ptr_eq(
            &first,
            &resolve_reference(&entries, &format!("lib:{id}"))
                .unwrap()
                .unwrap()
        ));
        let before = entries.clone();
        assert!(update(&mut entries, &id, Some("".into()), None, None).is_err());
        assert_eq!(entries, before);
        assert!(update(&mut entries, "missing", Some("Name".into()), None, None).is_err());
        let inverse = ColorLut {
            text: "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n".into(),
            ..lut
        };
        update(
            &mut entries,
            &id,
            None,
            Some(inverse),
            Some("inverse.cube".into()),
        )
        .unwrap();
        assert_eq!(entries[0].id, id);
        assert_eq!(entries[0].name, "Renamed");
        assert_eq!(
            resolve_reference(&entries, &format!("lib:{id}"))
                .unwrap()
                .unwrap()
                .apply([0.25, 0.5, 0.75]),
            [0.75, 0.5, 0.25]
        );
        assert_eq!(first.apply([0.25, 0.5, 0.75]), [0.25, 0.5, 0.75]);
    }
    #[test]
    fn import_reuses_identical_text_and_rejects_bad_input_without_mutation() {
        let lut = ColorLut {
            format: super::super::color_lut::TableFormat::Cube,
            text: "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n".into(),
            intensity: 100.0,
        };
        let mut library = vec![];
        assert_eq!(
            import(
                &mut library,
                lut.clone(),
                "Identity".into(),
                Some("first.cube".into())
            )
            .unwrap(),
            ("lut1".into(), false)
        );
        assert_eq!(
            import(
                &mut library,
                lut.clone(),
                "Renamed".into(),
                Some("second.cube".into())
            )
            .unwrap(),
            ("lut1".into(), true)
        );
        assert_eq!(library.len(), 1);
        assert_eq!(library[0].name, "Identity");
        let before = library.clone();
        assert!(import(&mut library, lut.clone(), "".into(), None).is_err());
        assert_eq!(library, before);
        let mut bad = lut.clone();
        bad.text = "invalid".into();
        assert!(import(&mut library, bad, "Invalid".into(), None).is_err());
        assert_eq!(library, before);
        library[0].id = "lut2".into();
        let mut other = lut;
        other.text = "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n".into();
        assert_eq!(
            import(&mut library, other, "Invert".into(), None).unwrap(),
            ("lut1".into(), false)
        );
    }
    #[test]
    fn references_share_tables_and_follow_library_updates() {
        let mut entries = vec![ProjectLut {
            id: "shared".into(),
            name: "Shared".into(),
            lut: ColorLut {
                format: super::super::color_lut::TableFormat::Cube,
                text: "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n".into(),
                intensity: 100.0,
            },
            source_path: None,
        }];
        let first = resolve_reference(&entries, "lib:shared").unwrap().unwrap();
        let second = resolve_reference(&entries, "lib:shared").unwrap().unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.apply([0.25, 0.5, 0.75]), [0.25, 0.5, 0.75]);
        entries[0].lut.text = "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n".into();
        let updated = resolve_reference(&entries, "lib:shared").unwrap().unwrap();
        assert!(!Arc::ptr_eq(&first, &updated));
        assert_eq!(updated.apply([0.25, 0.5, 0.75]), [0.75, 0.5, 0.25]);
        assert!(resolve_reference(&entries, "").unwrap().is_none());
        assert!(resolve_reference(&entries, "lib:missing").is_err());
        assert!(resolve_reference(&entries, "lib:").is_err());
        assert!(resolve_reference(&entries, "shared").is_err());
        entries.push(entries[0].clone());
        assert!(resolve_reference(&entries, "lib:shared").is_err());
        let builtin = resolve_reference(&[], "builtin:slog3-sgamut3cine-to-rec709")
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(
            &builtin,
            &super::super::builtin_luts::resolve("slog3-sgamut3cine-to-rec709").unwrap()
        ));
    }
    #[test]
    fn portable_library_rejects_duplicate_ids_and_invalid_tables() {
        let entry = ProjectLut {
            id: "lut-1".into(),
            name: "Identity".into(),
            lut: ColorLut {
                format: super::super::color_lut::TableFormat::Cube,
                text: "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n".into(),
                intensity: 100.0,
            },
            source_path: Some("identity.cube".into()),
        };
        validate(&[entry.clone()]).unwrap();
        assert!(validate(&[entry.clone(), entry.clone()]).is_err());
        let text = serde_json::to_string(&entry).unwrap();
        assert_eq!(serde_json::from_str::<ProjectLut>(&text).unwrap(), entry);
        let mut bad = entry;
        bad.lut.text = "LUT_3D_SIZE 999".into();
        assert!(validate(&[bad]).is_err());
    }
}
