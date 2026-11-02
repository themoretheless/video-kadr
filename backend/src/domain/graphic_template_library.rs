//! Persistent user .fcgt library. Documents retain original animation/font resources.
use super::graphic_template_file::GraphicTemplateFile;
use anyhow::{ensure, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub struct GraphicTemplateLibrary {
    root: PathBuf,
}
impl GraphicTemplateLibrary {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn list(&self) -> Result<Vec<(String, GraphicTemplateFile)>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut paths = fs::read_dir(&self.root)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("fcgt"))
            })
            .collect::<Vec<_>>();
        paths.sort();
        let mut entries = vec![];
        for path in paths {
            if path.symlink_metadata()?.file_type().is_symlink() {
                continue;
            }
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(file) = GraphicTemplateFile::read(&bytes) {
                    entries.push((
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        file,
                    ));
                }
            }
        }
        Ok(entries)
    }
    pub fn install(&self, bytes: &[u8]) -> Result<String> {
        let file = GraphicTemplateFile::read(bytes)?;
        let name = file.value()["name"].as_str().unwrap();
        let mut slug = String::new();
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase())
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-')
            }
        }
        let slug = slug.trim_end_matches('-');
        let slug = if slug.is_empty() { "template" } else { slug };
        fs::create_dir_all(&self.root)?;
        let bytes = file.write()?;
        // create_new never overwrites another template, even across concurrent installers.
        for suffix in 0..10000 {
            let name = if suffix == 0 {
                format!("{slug}.fcgt")
            } else {
                format!("{slug}-{suffix}.fcgt")
            };
            let path = self.root.join(&name);
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut output) => {
                    if let Err(error) = output.write_all(&bytes).and_then(|_| output.sync_all()) {
                        let _ = fs::remove_file(path);
                        return Err(error.into());
                    }
                    return Ok(name);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!("too many templates with same name")
    }
    /// Filmcraft installation semantics: stable filename, user identity, atomic replacement.
    pub fn install_replacing(&self, bytes: &[u8]) -> Result<String> {
        let file = GraphicTemplateFile::read(bytes)?;
        let mut document = file.value().clone();
        let mut slug = String::new();
        for ch in document["name"].as_str().unwrap().chars() {
            if ch.is_ascii_alphanumeric() { slug.push(ch.to_ascii_lowercase()); }
            else if !slug.is_empty() && !slug.ends_with('-') { slug.push('-'); }
        }
        let slug = slug.trim_end_matches('-');
        let slug = if slug.is_empty() { "template" } else { slug };
        if !document["id"].as_str().is_some_and(|id| id.starts_with("user:")) {
            document["id"] = serde_json::json!(format!("user:{slug}"));
        }
        let bytes = GraphicTemplateFile::read(&serde_json::to_vec(&document)?)?.write()?;
        fs::create_dir_all(&self.root)?;
        let name = format!("{slug}.fcgt");
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let temporary = self.root.join(format!(".install-{}-{sequence}.tmp", std::process::id()));
            let mut output = match fs::OpenOptions::new().write(true).create_new(true).open(&temporary) {
                Ok(output) => output,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            };
            let result = (|| -> std::io::Result<()> {
                output.write_all(&bytes)?;
                output.sync_all()?;
                drop(output);
                fs::rename(&temporary, self.root.join(&name))
            })();
            if let Err(error) = result { let _ = fs::remove_file(&temporary); return Err(error.into()); }
            return Ok(name);
        }
    }
    pub fn remove(&self, name: &str) -> Result<()> {
        ensure!(
            Path::new(name).file_name().and_then(|s| s.to_str()) == Some(name)
                && !name.contains(['/', '\\'])
                && name.ends_with(".fcgt"),
            "invalid user template name"
        );
        let path = self.root.join(name);
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "template is a symlink"
        );
        fs::remove_file(path)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_normalizes_identity_preserves_resources_and_rejects_invalid_update() {
        let dir = tempfile::tempdir().unwrap();
        let library = GraphicTemplateLibrary::new(dir.path());
        let catalog: serde_json::Value = serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
        let mut document = catalog["cases"][0].clone();
        document["name"] = serde_json::json!("Hello / World!");
        let original = document.clone();
        let name = library.install_replacing(&serde_json::to_vec(&document).unwrap()).unwrap();
        assert_eq!(name, "hello-world.fcgt");
        document["id"] = serde_json::json!("user:hello-world");
        assert_eq!(library.list().unwrap()[0].1.value(), &document);
        assert_ne!(original["id"], document["id"]);
        document["id"] = serde_json::json!("user:custom-id");
        document["description"] = serde_json::json!("updated");
        assert_eq!(library.install_replacing(&serde_json::to_vec(&document).unwrap()).unwrap(), name);
        let entries = library.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.value(), &document);
        assert!(library.install_replacing(b"broken").is_err());
        assert_eq!(library.list().unwrap()[0].1.value(), &document);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn install_restart_list_remove_preserves_documents_and_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let library = GraphicTemplateLibrary::new(dir.path());
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
        let original = &catalog["cases"][0];
        let bytes = serde_json::to_vec(original).unwrap();
        let first = library.install(&bytes).unwrap();
        let second = library.install(&bytes).unwrap();
        assert_ne!(first, second);
        fs::write(dir.path().join("broken.fcgt"), b"broken").unwrap();
        let restarted = GraphicTemplateLibrary::new(dir.path());
        let entries = restarted.list().unwrap();
        assert_eq!(entries.len(), 2);
        for (_, file) in entries {
            assert_eq!(file.value(), original)
        }
        assert!(restarted.remove("../outside.fcgt").is_err());
        assert!(restarted.remove("builtin:lower-third-slab").is_err());
        restarted.remove(&first).unwrap();
        assert_eq!(restarted.list().unwrap().len(), 1);
        assert!(restarted.install(b"PK\x03\x04rest").is_err());
        assert_eq!(restarted.list().unwrap().len(), 1);
    }
}
