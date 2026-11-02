//! FilmCraft generated builtins; pinned source provenance and MIT notice docs/licenses/filmcraft-MIT.txt.
use super::lut_tables::{Lut, Lut3d};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuiltinInfo {
    pub id: String,
    pub label: String,
    pub input: bool,
    pub size: usize,
}
#[derive(Deserialize)]
struct Catalog {
    commit: String,
    entries: Vec<BuiltinInfo>,
}
pub fn catalog() -> Result<Vec<BuiltinInfo>> {
    let catalog: Catalog =
        serde_json::from_str(include_str!("assets/filmcraft-builtin-luts.json"))?;
    ensure!(
        catalog.commit == "5231852443363f001c3f6b396dd9b1e6461ae2be",
        "builtin provenance mismatch"
    );
    ensure!(
        (17..=64).contains(&catalog.entries.len()),
        "invalid builtin catalog"
    );
    Ok(catalog.entries)
}
pub fn resolve(id: &str) -> Result<Arc<Lut>> {
    static TABLES: OnceLock<Result<Vec<(String, Arc<Lut>)>, String>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| load().map_err(|e| e.to_string()));
    let tables = tables.as_ref().map_err(|e| anyhow::anyhow!(e.clone()))?;
    tables
        .iter()
        .find(|(key, _)| key == id)
        .map(|(_, lut)| lut.clone())
        .ok_or_else(|| anyhow::anyhow!("unknown builtin LUT"))
}
fn load() -> Result<Vec<(String, Arc<Lut>)>> {
    let entries = catalog()?;
    let bytes = include_bytes!("assets/filmcraft-builtin-luts.bin");
    let stride = 33usize.pow(3) * 12;
    ensure!(
        bytes.len() == entries.len() * stride,
        "invalid builtin table length"
    );
    let mut tables = Vec::new();
    for (entry, block) in entries.into_iter().zip(bytes.chunks_exact(stride)) {
        ensure!(
            entry.size == 33 && !tables.iter().any(|(id, _)| id == &entry.id),
            "invalid builtin table metadata"
        );
        let mut data = Vec::with_capacity(33usize.pow(3));
        for sample in block.chunks_exact(12) {
            let rgb = std::array::from_fn(|c| {
                f32::from_le_bytes([
                    sample[c * 4],
                    sample[c * 4 + 1],
                    sample[c * 4 + 2],
                    sample[c * 4 + 3],
                ])
            });
            ensure!(
                rgb.iter().all(|q| q.is_finite() && (0.0..=1.0).contains(q)),
                "invalid builtin sample"
            );
            data.push(rgb);
        }
        let lut = Lut::from_cube(Lut3d {
            title: format!("FilmCraft {}", entry.label),
            size: 33,
            domain_min: [0.0; 3],
            domain_max: [1.0; 3],
            data,
        });
        tables.push((entry.id, Arc::new(lut)));
    }
    Ok(tables)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sony_middle_gray_and_all_tables_are_valid() {
        let entries = catalog().unwrap();
        assert_eq!(entries.len(), 17);
        for entry in entries {
            let table = resolve(&entry.id).unwrap();
            assert_eq!(table.cube.as_ref().unwrap().size, 33);
        }
        let table = resolve("slog3-sgamut3cine-to-rec709").unwrap();
        let gray = table.apply([(420.0 - 64.0) / 876.0; 3]);
        for c in gray {
            assert!((super::super::basic_grade::linear(c) - 0.18).abs() < 0.01);
        }
        assert!(resolve("missing").is_err());
    }
}
