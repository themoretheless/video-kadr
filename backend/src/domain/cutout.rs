//! Portable authored cutout correction and source-owned mask references.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    SmartBrush,
    Brush,
    SmartEraser,
    Eraser,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stroke {
    pub tool: Tool,
    pub size: f64,
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionAttachment>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Correction {
    pub custom: bool,
    pub strokes: Vec<Stroke>,
    pub feather: f64,
}
impl Correction {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.feather.is_finite()
                && (0.0..=0.1).contains(&self.feather)
                && self.strokes.len() <= 1024,
            "invalid mask correction"
        );
        let mut points = 0usize;
        for s in &self.strokes {
            if let Some(region) = &s.region {
                region.validate()?;
                ensure!(
                    matches!(s.tool, Tool::SmartBrush | Tool::SmartEraser)
                        && region.points == s.points,
                    "region tool/prompts mismatch"
                );
            }
            points = points
                .checked_add(s.points.len())
                .context("too many brush points")?;
            ensure!(
                points <= 65536
                    && !s.points.is_empty()
                    && s.size.is_finite()
                    && (0.005..=0.5).contains(&s.size)
                    && s.at.is_none_or(|v| v.is_finite() && v >= 0.)
                    && s.points
                        .iter()
                        .all(|p| p.iter().all(|v| v.is_finite() && (-0.5..=1.5).contains(v))),
                "invalid brush stroke"
            );
        }
        Ok(())
    }
    pub fn hash(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    deny_unknown_fields,
    try_from = "AttachmentWire"
)]
pub struct Attachment {
    pub source_id: String,
    pub mask_source_id: String,
    pub source_hash: String,
    pub mask_hash: String,
    pub mask_file_hash: String,
    pub model_hash: String,
    pub runtime_hash: String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub subject: Option<super::concat_cutout_import::Subject>,
    pub correction: Correction,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AttachmentWire {
    source_id: String,
    mask_source_id: String,
    source_hash: String,
    mask_hash: String,
    mask_file_hash: String,
    model_hash: String,
    runtime_hash: String,
    #[serde(default)]
    subject: Option<super::concat_cutout_import::Subject>,
    correction: Correction,
}
impl TryFrom<AttachmentWire> for Attachment {
    type Error = String;
    fn try_from(v: AttachmentWire) -> std::result::Result<Self, String> {
        let value = Self {
            source_id: v.source_id,
            mask_source_id: v.mask_source_id,
            source_hash: v.source_hash,
            mask_hash: v.mask_hash,
            mask_file_hash: v.mask_file_hash,
            model_hash: v.model_hash,
            runtime_hash: v.runtime_hash,
            subject: v.subject,
            correction: v.correction,
        };
        value.validate().map_err(|e| e.to_string())?;
        Ok(value)
    }
}
impl Attachment {
    pub fn validate(&self) -> Result<()> {
        for id in [&self.source_id, &self.mask_source_id] {
            super::composition::SourceId::parse(id.clone())?;
        }
        ensure!(
            self.source_id != self.mask_source_id,
            "cutout mask cannot be its source"
        );
        for hash in [
            &self.source_hash,
            &self.mask_hash,
            &self.mask_file_hash,
            &self.model_hash,
            &self.runtime_hash,
        ] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid cutout content identity"
            );
        }
        self.correction.validate()?;
        for region in self
            .correction
            .strokes
            .iter()
            .filter_map(|s| s.region.as_ref())
        {
            ensure!(
                region.source_id == self.source_id && region.source_hash == self.source_hash,
                "region belongs to another cutout input"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_snapshot_is_owned_bounded_and_roundtrips() {
        let value = serde_json::json!({"sourceId":"original","maskSourceId":"mask","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"modelHash":"d".repeat(64),"runtimeHash":"e".repeat(64),"correction":{"custom":true,"strokes":[{"tool":"brush","size":0.2,"points":[[0.5,0.5]],"at":null}],"feather":0.01}});
        let snapshot: Attachment = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
        for (field, bad) in [
            ("maskSourceId", serde_json::json!("../mask")),
            ("maskHash", serde_json::json!("untrusted")),
        ] {
            let mut changed = value.clone();
            changed[field] = bad;
            assert!(serde_json::from_value::<Attachment>(changed).is_err());
        }
        let mut bad = value;
        bad["correction"]["feather"] = serde_json::json!(0.11);
        assert!(serde_json::from_value::<Attachment>(bad).is_err());
    }
}

/// Portable learned source region; no private job IDs or filesystem paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, try_from = "RegionWire")]
pub struct RegionAttachment {
    pub source_id: String,
    pub mask_source_id: String,
    pub source_hash: String,
    pub mask_hash: String,
    pub mask_file_hash: String,
    pub encoder_hash: String,
    pub decoder_hash: String,
    pub runtime_hash: String,
    pub prompt_hash: String,
    pub points: Vec<[f64; 2]>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegionWire {
    source_id: String,
    mask_source_id: String,
    source_hash: String,
    mask_hash: String,
    mask_file_hash: String,
    encoder_hash: String,
    decoder_hash: String,
    runtime_hash: String,
    prompt_hash: String,
    points: Vec<[f64; 2]>,
}
impl TryFrom<RegionWire> for RegionAttachment {
    type Error = String;
    fn try_from(v: RegionWire) -> std::result::Result<Self, String> {
        let value = Self {
            source_id: v.source_id,
            mask_source_id: v.mask_source_id,
            source_hash: v.source_hash,
            mask_hash: v.mask_hash,
            mask_file_hash: v.mask_file_hash,
            encoder_hash: v.encoder_hash,
            decoder_hash: v.decoder_hash,
            runtime_hash: v.runtime_hash,
            prompt_hash: v.prompt_hash,
            points: v.points,
        };
        value.validate().map_err(|e| e.to_string())?;
        Ok(value)
    }
}
impl RegionAttachment {
    pub fn validate(&self) -> Result<()> {
        for id in [&self.source_id, &self.mask_source_id] {
            super::composition::SourceId::parse(id.clone())?;
        }
        ensure!(
            self.source_id != self.mask_source_id,
            "region mask cannot be its source"
        );
        for hash in [
            &self.source_hash,
            &self.mask_hash,
            &self.mask_file_hash,
            &self.encoder_hash,
            &self.decoder_hash,
            &self.runtime_hash,
            &self.prompt_hash,
        ] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
                "invalid region content identity"
            );
        }
        ensure!(
            !self.points.is_empty()
                && self.points.len() <= 65536
                && self
                    .points
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && (-0.5..=1.5).contains(v)),
            "invalid region prompts"
        );
        ensure!(
            format!("{:x}", Sha256::digest(serde_json::to_vec(&self.points)?)) == self.prompt_hash,
            "region prompt identity mismatch"
        );
        Ok(())
    }
}

pub const SELFIE_MODEL_HASH: &str =
    "3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad";
pub fn mask_size(model_hash: &str) -> u32 {
    if model_hash == SELFIE_MODEL_HASH {
        256
    } else {
        512
    }
}

/// Native RVM uses a longest-edge 512 grid rounded to multiples of 16.
/// Keep square Selfie/Object contracts unchanged.
pub const PERSON_MODEL_HASH:&str="88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828";
pub fn mask_dimensions(model_hash:&str,width:u32,height:u32)->anyhow::Result<(u32,u32)>{
 anyhow::ensure!(width>0&&height>0,"invalid cutout input dimensions");
 if model_hash==PERSON_MODEL_HASH{
  let scale=512./f64::from(width.max(height));
  let edge=|value:u32|(((f64::from(value)*scale/16.).round()as u32)*16).max(64);
  Ok((edge(width),edge(height)))
 }else{let size=mask_size(model_hash);Ok((size,size))}
}
pub fn still_mask_schema(model_hash:&str)->u32{if model_hash==PERSON_MODEL_HASH{8}else if model_hash==SELFIE_MODEL_HASH{6}else{3}}
