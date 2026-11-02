//! Immutable author bundle inventory used by managed Kokoro installation.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Component, Path},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCatalog {
    #[serde(default = "compact_repository")]
    pub repository: String,
    #[serde(default)]
    pub family: super::local_tts::TtsFamily,
    pub id: String,
    pub name: String,
    pub revision: String,
    pub bytes: u64,
    pub files: Vec<ModelFile>,
}
fn digest_valid(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn compact_repository() -> String {
    "csukuangfj/kokoro-int8-multi-lang-v1_0".into()
}
impl ModelCatalog {
    pub fn all() -> Result<Vec<Self>> {
        let compact = Self::bundled()?;
        let full: Self = serde_json::from_str(include_str!("tts-model-catalog-full.json"))?;
        full.validate()?;
        ensure!(compact.id != full.id, "duplicate model identity");
        let pocket: Self = serde_json::from_str(include_str!("tts-model-catalog-pocket.json"))?;
        pocket.validate()?;
        ensure!(
            pocket.id != compact.id && pocket.id != full.id,
            "duplicate model identity"
        );
        let chatterbox: Self =
            serde_json::from_str(include_str!("tts-model-catalog-chatterbox.json"))?;
        chatterbox.validate()?;
        ensure!(
            ![&compact.id, &full.id, &pocket.id].contains(&&chatterbox.id),
            "duplicate model identity"
        );
        Ok(vec![compact, full, pocket, chatterbox])
    }
    pub fn find(id: &str) -> Result<Option<Self>> {
        Ok(Self::all()?.into_iter().find(|m| m.id == id))
    }

    pub fn bundled() -> Result<Self> {
        let value: Self = serde_json::from_str(include_str!("tts-model-catalog.json"))?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(
                self.repository.as_str(),
                "csukuangfj/kokoro-int8-multi-lang-v1_0"
                    | "csukuangfj/kokoro-multi-lang-v1_0"
                    | "csukuangfj2/sherpa-onnx-pocket-tts-int8-2026-01-26"
                    | "ResembleAI/chatterbox-turbo-ONNX"
            ),
            "unsupported model repository"
        );
        ensure!(
            match self.family {
                super::local_tts::TtsFamily::Chatterbox =>
                    self.repository == "ResembleAI/chatterbox-turbo-ONNX",
                super::local_tts::TtsFamily::Pocket =>
                    self.repository == "csukuangfj2/sherpa-onnx-pocket-tts-int8-2026-01-26",
                super::local_tts::TtsFamily::Kokoro => self.repository.starts_with("csukuangfj/"),
            },
            "model family/repository mismatch"
        );
        ensure!(
            digest_valid(&self.id, 64) && digest_valid(&self.revision, 40),
            "invalid model identity"
        );
        ensure!(
            !self.files.is_empty() && self.files.len() <= 2048,
            "invalid model inventory size"
        );
        let mut paths = BTreeSet::new();
        let mut total = 0u64;
        for file in &self.files {
            let path = Path::new(&file.path);
            ensure!(
                !file.path.is_empty()
                    && !file.path.contains('\\')
                    && !file.path.contains('\0')
                    && !file.path.contains(':')
                    && !path.is_absolute(),
                "invalid model path"
            );
            ensure!(
                path.components().all(|c| matches!(c, Component::Normal(_)))
                    && file
                        .path
                        .split('/')
                        .all(|s| !s.is_empty() && s != "." && s != ".."),
                "invalid model path components"
            );
            ensure!(
                paths.insert(&file.path) && digest_valid(&file.sha256, 64),
                "duplicate path or invalid checksum"
            );
            total = total
                .checked_add(file.bytes)
                .context("model size overflow")?;
        }
        ensure!(
            total == self.bytes && total <= 2_000_000_000,
            "invalid bundle size"
        );
        Ok(())
    }
    /// Validate every file and reject unlisted files, special files and symlinks.
    /// Call on an exclusively owned staging directory before publishing it.
    pub fn verify_directory(&self, root: &Path) -> Result<()> {
        self.validate()?;
        ensure!(
            std::fs::symlink_metadata(root)?.file_type().is_dir(),
            "model root must be a directory"
        );
        let expected: BTreeSet<_> = self.files.iter().map(|f| root.join(&f.path)).collect();
        let mut found = BTreeSet::new();
        let mut pending = vec![root.to_path_buf()];
        let mut entries = 0;
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                entries += 1;
                ensure!(entries <= 2048, "too many model entries");
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    pending.push(entry.path());
                } else {
                    ensure!(
                        kind.is_file() && expected.contains(&entry.path()),
                        "unexpected model entry"
                    );
                    found.insert(entry.path());
                }
            }
        }
        ensure!(found == expected, "incomplete model bundle");
        let mut buffer = [0u8; 65536];
        for entry in &self.files {
            let mut file = std::fs::File::open(root.join(&entry.path))?;
            ensure!(
                file.metadata()?.len() == entry.bytes,
                "model file size mismatch"
            );
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                ensure!(bytes <= entry.bytes, "model file grew during verification");
                hash.update(&buffer[..count]);
            }
            ensure!(
                bytes == entry.bytes && format!("{:x}", hash.finalize()) == entry.sha256,
                "model file checksum mismatch"
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chatterbox_inventory_is_pinned_and_family_bound() {
        let catalog =
            ModelCatalog::find("3d500b0e333d1661a40e854cbdbc93d8f3e9f01942b6a4e1cea3f6b4aa8c9521")
                .unwrap()
                .unwrap();
        assert_eq!(
            catalog.family,
            super::super::local_tts::TtsFamily::Chatterbox
        );
        assert_eq!(catalog.bytes, 1123749449);
        assert_eq!(catalog.files.len(), 10);
        assert!(catalog
            .files
            .iter()
            .any(|f| f.path == "onnx/language_model_quantized.onnx_data"
                && f.sha256 == "ec9945df36cb5d131d46688f2609fd715fbfcb0b8ee9681af5c84118de2d55a2"));
        let mut wrong = catalog;
        wrong.family = super::super::local_tts::TtsFamily::Pocket;
        assert!(wrong.validate().is_err());
    }
    #[test]
    fn both_kokoro_variants_have_independent_pinned_inventories() {
        let catalogs = ModelCatalog::all().unwrap();
        assert_eq!(catalogs.len(), 4);
        assert_eq!(
            catalogs[0].id,
            "8d94ed9c7b2aaeedd8be9ed61b484aba9035912fb64ba323556f8d249ec059f1"
        );
        assert_ne!(catalogs[0].id, catalogs[1].id);
        assert!(catalogs[0]
            .files
            .iter()
            .any(|f| f.path == "model.int8.onnx"));
        assert!(!catalogs[1]
            .files
            .iter()
            .any(|f| f.path == "model.int8.onnx"));
        assert!(catalogs[1].files.iter().any(|f| f.path == "model.onnx"));
        assert_eq!(catalogs[1].files.len(), 377);
        assert_eq!(catalogs[1].bytes, 401239297);
        let mut invalid = catalogs[1].clone();
        invalid.repository = "attacker/model".into();
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn catalog_rejects_paths_and_size_corruption() {
        let mut model = ModelCatalog::bundled().unwrap();
        assert_eq!(model.files.len(), 377);
        for path in [
            "../escape",
            "/absolute",
            "a/../b",
            "a//b",
            "a/./b",
            "C:drive",
            "a\\b",
        ] {
            let old = std::mem::replace(&mut model.files[0].path, path.into());
            assert!(model.validate().is_err(), "{path}");
            model.files[0].path = old;
        }
        model.bytes += 1;
        assert!(model.validate().is_err());
    }
    #[test]
    fn verifier_rejects_modified_and_extra_files() {
        let root = std::env::temp_dir().join(format!("tts-catalog-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("model"), b"abc").unwrap();
        let mut model = ModelCatalog::bundled().unwrap();
        model.files = vec![ModelFile {
            path: "model".into(),
            bytes: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        }];
        model.bytes = 3;
        model.verify_directory(&root).unwrap();
        std::fs::write(root.join("model"), b"abd").unwrap();
        assert!(model.verify_directory(&root).is_err());
        std::fs::write(root.join("model"), b"abc").unwrap();
        std::fs::write(root.join("extra"), b"").unwrap();
        assert!(model.verify_directory(&root).is_err());
        std::fs::remove_file(root.join("extra")).unwrap();
        std::fs::remove_file(root.join("model")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/dev/null", root.join("model")).unwrap();
            assert!(model.verify_directory(&root).is_err());
            std::fs::remove_file(root.join("model")).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    #[ignore = "requires pinned public Kokoro qualification bundle"]
    fn actual_author_bundle() {
        let root = std::env::var_os("KOKORO_CATALOG_TEST_DIR").expect("KOKORO_CATALOG_TEST_DIR");
        ModelCatalog::bundled()
            .unwrap()
            .verify_directory(Path::new(&root))
            .unwrap();
    }
}
