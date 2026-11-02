//! Original correction/feather stage shared with the owned native model runtime.
use std::sync::{Arc,atomic::AtomicBool};
use anyhow::{ensure,Result};
use super::{concat_cutout_clock::ResolvedMask,concat_cutout_import::Plan};
fn owned(mask:&ResolvedMask)->Result<video_kadr_mask_core::Mask>{
 ensure!(mask.width>0&&mask.width<=4096&&mask.height>0&&mask.height<=4096&&mask.width.checked_mul(mask.height)==Some(mask.probabilities.len()),"invalid resolved cutout plane");
 Ok(video_kadr_mask_core::Mask{width:mask.width,height:mask.height,bytes:mask.probabilities.clone()})
}
/// The caller resolves the subject model and source-owned region planes first.
/// None regions retain original smart confidence fallback, not an opaque mask.
pub fn resolve(plan:&Plan,automatic:&ResolvedMask,regions:&[Option<Arc<ResolvedMask>>],source_aspect:f32,cancel:&AtomicBool)->Result<Option<Arc<ResolvedMask>>>{
 plan.correction.validate()?;
 let correction:video_kadr_mask_core::mask_edit::Correction=serde_json::from_value(serde_json::to_value(&plan.correction)?)?;
 let auto=owned(automatic)?;
 let regions=regions.iter().map(|region|region.as_ref().map(|region|owned(region)).transpose()).collect::<Result<Vec<_>>>()?;
 let references:Vec<_>=regions.iter().map(Option::as_ref).collect();
 let mask=video_kadr_mask_core::mask_edit::resolve(&auto,correction.custom,&correction.strokes,&references,source_aspect,correction.feather,cancel)?;
 Ok(mask.map(|mask|Arc::new(ResolvedMask{width:mask.width,height:mask.height,probabilities:mask.bytes})))
}
