//! Project-owned Concat font bytes and a scoped, bounded renderer bank.
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex, OnceLock},
};
pub const MAX_FONT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FONT_JSON_BYTES: usize = MAX_FONT_BYTES.div_ceil(3) * 4 + 32 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontResource {
    pub family: String,
    pub data: String,
}
pub fn family_name(family: &str) -> &str {
    family.trim().trim_matches(['\'', '"'])
}
pub fn bank(resources: &[FontResource]) -> Result<Option<Arc<concat_text::Fonts>>> {
    if resources.is_empty() {
        return Ok(None);
    }
    ensure!(resources.len() <= 32, "Project font limit is 32 files");
    let total = resources.iter().try_fold(0usize, |n, r| {
        n.checked_add(r.data.len())
            .context("font byte count overflow")
    })?;
    ensure!(
        total <= MAX_FONT_JSON_BYTES,
        "Project font bytes exceed 8 MiB decoded budget"
    );
    ensure!(
        resources.iter().all(|r| !family_name(&r.family).is_empty()
            && r.family.len() <= 256
            && !r.family.contains('\0')),
        "Invalid project font family"
    );
    let key = format!("{:x}", Sha256::digest(serde_json::to_vec(resources)?));
    static CACHE: OnceLock<Mutex<VecDeque<(String, Arc<concat_text::Fonts>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    {
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Font cache poisoned"))?;
        if let Some(index) = cache.iter().position(|(hash, _)| hash == &key) {
            let entry = cache.remove(index).unwrap();
            let fonts = entry.1.clone();
            cache.push_back(entry);
            return Ok(Some(fonts));
        }
    }
    let mut bytes = 0usize;
    let mut decoded = Vec::new();
    for resource in resources {
        let data = STANDARD
            .decode(&resource.data)
            .context("Invalid project font base64")?;
        bytes = bytes
            .checked_add(data.len())
            .context("Font size overflow")?;
        ensure!(
            bytes <= MAX_FONT_BYTES,
            "Project font bytes exceed 8 MiB decoded budget"
        );
        let families = concat_text::families_from_data(&data);
        ensure!(
            families
                .iter()
                .any(|family| family.eq_ignore_ascii_case(family_name(&resource.family))),
            "Font file does not contain requested family {}",
            resource.family
        );
        decoded.push(data);
    }
    let mut fonts = concat_text::Fonts::new();
    for data in decoded {
        ensure!(fonts.add_data(data), "Project font could not be loaded");
    }
    let fonts = Arc::new(fonts);
    let mut cache = cache
        .lock()
        .map_err(|_| anyhow::anyhow!("Font cache poisoned"))?;
    if let Some((_, existing)) = cache.iter().find(|(hash, _)| hash == &key) {
        return Ok(Some(existing.clone()));
    }
    while cache.len() >= 4 {
        cache.pop_front();
    }
    cache.push_back((key, fonts.clone()));
    Ok(Some(fonts))
}
thread_local! {static CURRENT:RefCell<Option<Arc<concat_text::Fonts>>>=const{RefCell::new(None)};}
pub fn current() -> Option<Arc<concat_text::Fonts>> {
    CURRENT.with(|slot| slot.borrow().clone())
}
/// A guard cannot cross an await or move to another render thread.
pub struct Scope {
    previous: Option<Arc<concat_text::Fonts>>,
    _thread: PhantomData<Rc<()>>,
}
impl Scope {
    pub fn enter(resources: &[FontResource]) -> Result<Self> {
        let next = bank(resources)?;
        let previous = CURRENT.with(|slot| slot.replace(next));
        Ok(Self {
            previous,
            _thread: PhantomData,
        })
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Add { data: String },
    Remove { index: usize },
}
/// Font files are project assets; title family references remain authored as before.
pub fn edit(
    project: &super::nested_composition::NestedCompositionProject,
    operation: Edit,
) -> Result<super::nested_composition::NestedCompositionProject> {
    project.dependency_order()?;
    let mut changed = project.clone();
    match operation {
        Edit::Add { data } => {
            ensure!(
                data.len() <= MAX_FONT_JSON_BYTES,
                "Font exceeds project byte budget"
            );
            let bytes = STANDARD.decode(data).context("Invalid font base64")?;
            ensure!(bytes.len() <= MAX_FONT_BYTES, "Font exceeds 8 MiB");
            let families = concat_text::families_from_data(&bytes);
            let family = families
                .first()
                .context("Font contains no readable family")?
                .clone();
            let data = STANDARD.encode(bytes);
            if changed.fonts.iter().any(|font| font.data == data) {
                return Ok(changed);
            }
            changed.fonts.push(FontResource { family, data });
        }
        Edit::Remove { index } => {
            ensure!(
                index < changed.fonts.len(),
                "Selected font file is unavailable"
            );
            changed.fonts.remove(index);
        }
    }
    changed.dependency_order()?;
    let document = serde_json::to_value(&changed)?;
    ensure!(
        serde_json::to_vec(&document)?.len()
            <= crate::db::composition_document_byte_limit(&document)?,
        "Font change exceeds saved project budget"
    );
    Ok(changed)
}
