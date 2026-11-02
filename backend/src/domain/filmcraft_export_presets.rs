//! Preset-library semantics adapted from Filmcraft export_tools.rs (MIT),
//! commit 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! Settings remain source JSON here; execution must resolve and validate them.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FORMAT: &str = "filmcraft.export-presets";
pub const DEFAULT_PRESET: &str = "Match Source – Adaptive High Bitrate";

pub fn key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    pub settings: Value,
    #[serde(skip)]
    pub builtin: bool,
}

pub fn builtins() -> Vec<Preset> {
    let mut entries: Vec<Preset> =
        serde_json::from_str(include_str!("filmcraft_export_presets_builtin.json"))
            .expect("pinned built-in preset catalog must be valid");
    for entry in &mut entries {
        entry.builtin = true;
    }
    entries
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Library {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub favorites: Vec<String>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            format: FORMAT.into(),
            version: 1,
            presets: vec![],
            favorites: vec![],
        }
    }
}

impl Library {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut library: Self = serde_json::from_slice(bytes)?;
        ensure!(library.format == FORMAT, "not an export preset file");
        ensure!(library.version <= 1, "unsupported export preset version");
        for preset in &mut library.presets {
            ensure!(!preset.name.trim().is_empty(), "a preset has no name");
            ensure!(
                preset.settings.is_object(),
                "preset settings must be an object"
            );
            preset.settings =
                super::filmcraft_export_preset_settings::normalize(preset.settings.clone(), false)?;
            preset.builtin = false;
        }
        Ok(library)
    }

    pub fn all(&self) -> Vec<Preset> {
        let mut result: Vec<_> = builtins()
            .into_iter()
            .filter(|b| !self.presets.iter().any(|p| key(&p.name) == key(&b.name)))
            .collect();
        result.extend(self.presets.clone());
        result
    }

    pub fn find(&self, name: &str) -> Option<Preset> {
        let wanted = key(name);
        self.presets
            .iter()
            .find(|p| key(&p.name) == wanted)
            .cloned()
            .or_else(|| builtins().into_iter().find(|p| key(&p.name) == wanted))
    }

    pub fn is_favorite(&self, name: &str) -> bool {
        self.favorites.iter().any(|p| key(p) == key(name))
    }

    /// The caller supplies resolved settings; no output path/range is retained.
    pub fn save(&mut self, mut preset: Preset, overwrite: bool) -> Result<()> {
        preset.settings =
            super::filmcraft_export_preset_settings::normalize(preset.settings, true)?;
        preset.name = preset.name.trim().into();
        ensure!(!preset.name.is_empty(), "need a name");
        ensure!(
            preset.settings.is_object(),
            "preset settings must be an object"
        );
        ensure!(
            !builtins().iter().any(|p| key(&p.name) == key(&preset.name)),
            "built-in name is reserved"
        );
        let existing = self
            .presets
            .iter()
            .position(|p| key(&p.name) == key(&preset.name));
        ensure!(existing.is_none() || overwrite, "preset already exists");
        preset.settings["path"] = Value::String(String::new());
        preset.settings["range"] = Value::Null;
        preset.builtin = false;
        match existing {
            Some(i) => self.presets[i] = preset,
            None => self.presets.push(preset),
        }
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        let index = self.presets.iter().position(|p| key(&p.name) == key(name));
        let Some(index) = index else {
            bail!("no user preset named {name}")
        };
        self.presets.remove(index);
        self.favorites.retain(|p| key(p) != key(name));
        Ok(())
    }

    pub fn favorite(&mut self, name: &str, enabled: Option<bool>) -> Result<bool> {
        let preset = self
            .find(name)
            .ok_or_else(|| anyhow::anyhow!("unknown preset"))?;
        let enabled = enabled.unwrap_or(!self.is_favorite(name));
        self.favorites.retain(|p| key(p) != key(name));
        if enabled {
            self.favorites.push(preset.name);
        }
        Ok(enabled)
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        let imported = Self::parse(bytes)?;
        let builtins = builtins();
        let mut names = vec![];
        for mut preset in imported.presets {
            if builtins.iter().any(|p| key(&p.name) == key(&preset.name)) {
                preset.name = format!("{} (imported)", preset.name);
            }
            self.presets.retain(|p| key(&p.name) != key(&preset.name));
            names.push(preset.name.clone());
            self.presets.push(preset);
        }
        // Upstream import deliberately does not apply the file's favorites.
        Ok(names)
    }

    pub fn export(&self, names: Option<&[String]>) -> Result<Self> {
        let presets = match names {
            Some(names) => names
                .iter()
                .map(|n| {
                    self.find(n)
                        .ok_or_else(|| anyhow::anyhow!("unknown preset: {n}"))
                })
                .collect::<Result<Vec<_>>>()?,
            None => self.presets.clone(),
        };
        let favorites = self
            .favorites
            .iter()
            .filter(|f| presets.iter().any(|p| key(&p.name) == key(f)))
            .cloned()
            .collect();
        Ok(Self {
            presets,
            favorites,
            ..Self::default()
        })
    }

    pub fn list(
        &self,
        query: &str,
        category: Option<&str>,
        format: Option<&str>,
        favorites: bool,
    ) -> Vec<Preset> {
        let query = query.to_ascii_lowercase();
        let format = format.and_then(super::filmcraft_export_preset_overrides::format_id);
        self.all()
            .into_iter()
            .filter(|p| {
                (query.is_empty()
                    || [&p.name, &p.description, &p.category]
                        .iter()
                        .any(|v| v.to_ascii_lowercase().contains(&query)))
                    && category.is_none_or(|c| p.category.eq_ignore_ascii_case(c))
                    && format.is_none_or(|f| {
                        p.settings["format"]
                            .as_str()
                            .is_some_and(|v| v.eq_ignore_ascii_case(f))
                    })
                    && (!favorites || self.is_favorite(&p.name))
            })
            .collect()
    }
}
