// SPDX-License-Identifier: AGPL-3.0-or-later
//! Original cutout settings translated into the existing owned mask-correction model.
//! Analysis/model bindings must be resolved before this plan can produce a mask.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
use super::cutout::{Correction,Stroke,Tool};
#[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum Subject {#[default]Auto,Person,Object}
#[derive(Debug,Clone,Copy,Deserialize)]
#[serde(rename_all="lowercase")]
enum Mode {Auto,Custom}
#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct SourceStroke {tool:Tool,size:f64,points:Vec<[f64;2]>,#[serde(default)]at:Option<f64>}
#[derive(Debug,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct SourceCutout {mode:Mode,#[serde(default)]subject:Subject,#[serde(default="default_feather")]feather:f64,#[serde(default)]strokes:Vec<SourceStroke>}
fn default_feather()->f64{0.01}
#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Plan {
 pub subject:Subject,
 pub correction:Correction,
 /// Smart strokes painted at source instants need source-owned region results.
 /// Empty for auto mode, or the original legacy confidence fallback without at.
 pub region_source_seconds:Vec<f64>,
}
pub fn plan(value:&serde_json::Value)->Result<Plan>{
 let input:SourceCutout=serde_json::from_value(value.clone())?;
 ensure!(input.strokes.len()<=1024,"too many Concat cutout strokes");
 let count=input.strokes.iter().try_fold(0usize,|sum,stroke|sum.checked_add(stroke.points.len())).ok_or_else(||anyhow::anyhow!("cutout point budget overflow"))?;
 ensure!(count<=65536,"too many Concat cutout points");
 ensure!(input.feather.is_finite(),"nonfinite Concat cutout feather");
 let custom=matches!(input.mode,Mode::Custom);let mut strokes=Vec::new();let mut regions=Vec::new();
 for stroke in input.strokes{
  ensure!(stroke.size.is_finite()&&stroke.at.is_none_or(f64::is_finite)&&stroke.points.iter().flatten().all(|v|v.is_finite()),"nonfinite Concat stroke");
  if stroke.points.is_empty(){continue}
  let at=stroke.at.map(|at|at.max(0.));
  if custom&&matches!(stroke.tool,Tool::SmartBrush|Tool::SmartEraser){if let Some(at)=at{regions.push(at)}}
  strokes.push(Stroke{tool:stroke.tool,size:stroke.size.clamp(0.005,0.5),points:stroke.points.into_iter().map(|p|p.map(|v|v.clamp(-0.5,1.5))).collect(),at,region:None});
 }
 regions.sort_by(f64::total_cmp);regions.dedup();
 let correction=Correction{custom,strokes,feather:input.feather.clamp(0.,0.1)};correction.validate()?;
 Ok(Plan{subject:input.subject,correction,region_source_seconds:regions})
}
