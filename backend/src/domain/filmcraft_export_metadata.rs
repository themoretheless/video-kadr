//! Filmcraft export metadata (MIT reference: settings.rs, commit
//! 5231852443363f001c3f6b396dd9b1e6461ae2be). User values are trimmed only at mux time.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExportMetadata {
    pub title: String,
    pub creator: String,
    pub copyright: String,
    pub description: String,
    pub comment: String,
}
impl ExportMetadata {
    fn fields(&self) -> [(&'static str, &str); 5] {
        [("title", &self.title), ("artist", &self.creator), ("copyright", &self.copyright), ("description", &self.description), ("comment", &self.comment)]
    }
    pub fn validate(&self) -> Result<()> {
        let fields=self.fields();
        ensure!(fields.iter().all(|(_,v)|v.len()<=16*1024&&!v.contains('\0')), "invalid or oversized export metadata field");
        ensure!(fields.iter().map(|(_,v)|v.len()).sum::<usize>()<=64*1024, "export metadata exceeds total budget");
        Ok(())
    }
    pub fn ffmpeg_tags(&self) -> Vec<(String,String)> {
        self.fields().into_iter().filter(|(_,v)|!v.trim().is_empty()).map(|(k,v)|(k.to_owned(),v.trim().to_owned())).collect()
    }
}
