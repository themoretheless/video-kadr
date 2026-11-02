//! Concat v1 output. Unsupported native ownership is diagnosed rather than flattened or discarded.
use super::{nested_composition::NestedCompositionProject,concat_effect_export::export_effect};
use anyhow::{Context,Result,ensure,bail};
use serde_json::{Value,json};
use std::collections::BTreeMap;
fn constant(value:&Value)->Result<f64>{ensure!(value["mode"]=="constant","Native animation needs source-key conversion before Concat export");value["value"].as_f64().context("Missing constant pose value")}
fn source_keys(value:&Value)->Result<Value>{
 if let Some(object)=value.as_object(){let key=object.get("at").and_then(Value::as_f64).is_some()&&object.get("value").and_then(Value::as_f64).is_some();ensure!(!key||object.get("derived")!=Some(&Value::Bool(true)),"Derived source-window keys need a portable authoring conversion");let mut out=serde_json::Map::new();for (name,value) in object{if !key||name!="derived"{out.insert(name.clone(),source_keys(value)?);}}return Ok(Value::Object(out))}
 if let Some(array)=value.as_array(){return array.iter().map(source_keys).collect::<Result<Vec<_>>>().map(Value::Array)}
 Ok(value.clone())
}
fn visual(clip:&Value,width:f64,height:f64,media_kind:Option<&str>)->Result<Value>{
 ensure!(clip["colorAnimation"].is_null(),"Native color animation needs source conversion");
 ensure!(clip["enabled"]!=false,"Disabled clips need an explicit source visibility lane");
 ensure!(clip["animationOffsetUs"].as_u64().unwrap_or(0)==0,"Retained source clock offset needs rewindowing before export");
 let start=clip["timelineStartTick"].as_u64().context("Source-free clip start missing")?;
 let end=clip["timelineEndTick"].as_u64().context("Source-free clip end missing")?;
 let duration=end.checked_sub(start).context("Invalid clip interval")?;
 let start_seconds=start as f64/1e6;let duration_seconds=duration as f64/1e6;ensure!((start_seconds*1e6).round() as u64==start&&(duration_seconds*1e6).round() as u64==duration,"Source floating seconds cannot retain exact native placement");
 let mut out=json!({"id":clip["id"],"start":start_seconds,"duration":duration_seconds});
 if !clip["concatTreatment"].is_null(){
  let treatment=&clip["concatTreatment"];ensure!(treatment["durationTicks"].as_u64()==Some(duration),"Treatment source duration differs from visible duration");
  out["kind"]=json!("layer");out["opacity"]=treatment["strength"].clone();out["fadeIn"]=treatment["fadeIn"].clone();out["fadeOut"]=treatment["fadeOut"].clone();
 }else{
  if !clip["concatTitle"].is_null(){out["kind"]=json!("text");out["text"]=clip["concatTitle"].clone();out["text"]["content"]=clip["text"].clone();}
  else if !clip["concatShape"].is_null(){ensure!(clip["width"].as_f64()==Some(width)&&clip["height"].as_f64()==Some(height),"Shape raster size differs from source canvas");out["kind"]=json!("shape");out["shape"]=clip["concatShape"].clone();}
  else if let Some(kind)=media_kind{out["kind"]=json!(kind);out["mediaId"]=clip["sourceId"].clone();}
  else{bail!("Ordinary native text/graphic has no retained Concat style")}
  let transform=&clip["transform"];
  ensure!(transform["anchorX"].as_f64()==Some(0.5)&&transform["anchorY"].as_f64()==Some(0.5)&&transform["position"].is_null(),"Native anchor/path cannot be represented by source pose fields");
  let animation=&transform["concatAnimation"];
  if !animation.is_null(){
   for channel in ["x","y","scaleX","scaleY","rotationDegrees"]{constant(&transform[channel])?;}constant(&clip["opacity"])?;
   ensure!(animation["durationTicks"].as_u64()==Some(duration),"Source property duration differs from visible duration");
   ensure!(animation["canvas"]==json!([width,height]),"Source property canvas differs from output canvas");
   let rest=&animation["rest"];ensure!(rest["scaleX"]==rest["scaleY"],"Source base scale must be uniform; use stretch for independent axes");
   for (source,native) in [("scale","scaleX"),("offsetX","x"),("offsetY","y"),("rotation","rotationDegrees"),("opacity","opacity")]{out[source]=rest[native].clone();}
   out["stretchX"]=animation["stretch"][0].clone();out["stretchY"]=animation["stretch"][1].clone();
   let mut keys=vec![];for (property,run) in animation["keys"].as_object().context("Source key record missing")?{for key in run.as_array().context("Source key run missing")?{let mut key=source_keys(key)?;key["property"]=json!(property);keys.push(key);}}out["keys"]=json!(keys);
  }else{
   out["scale"]=json!(1.);out["stretchX"]=json!(constant(&transform["scaleX"])?);out["stretchY"]=json!(constant(&transform["scaleY"])?);out["offsetX"]=json!(constant(&transform["x"])?/width);out["offsetY"]=json!(constant(&transform["y"])?/height);out["rotation"]=json!(constant(&transform["rotationDegrees"])?);out["opacity"]=json!(constant(&clip["opacity"])?);
  }
  let preparation=&transform["concatPreparation"];
  if !preparation.is_null(){ensure!(preparation["canvas"]==json!([width as u64,height as u64]),"Source preparation canvas differs from output canvas");out["crop"]=json!({"left":preparation["crop"][0],"top":preparation["crop"][1],"right":preparation["crop"][2],"bottom":preparation["crop"][3]});out["flipH"]=json!(preparation["flipH"]==true);out["flipV"]=json!(preparation["flipV"]==true);}
  out["blend"]=clip.get("concatShapeBlend").or_else(||clip.get("blendMode")).filter(|value|!value.is_null()).cloned().unwrap_or(json!("normal"));
 }
 let mut effects=vec![];for effect in clip["effects"].as_array().into_iter().flatten(){let effect:super::composition::VideoEffect=serde_json::from_value(effect.clone())?;let mut link=serde_json::to_value(export_effect(&effect)?)?;if let Some(keys)=link.get_mut("keys"){*keys=source_keys(keys)?;}effects.push(link);}out["videoEffects"]=json!(effects);
 Ok(out)
}
pub fn export(project:&NestedCompositionProject)->Result<Value>{export_with_paths(project,&BTreeMap::new())}
pub fn export_with_paths(project:&NestedCompositionProject,paths:&BTreeMap<String,String>)->Result<Value>{
 project.dependency_order()?;
 for node in project.compositions.values(){super::concat_lane_groups::groups(&node.content)?;}
 ensure!(project.fonts.is_empty()&&project.luts.is_empty(),"Embedded fonts/LUTs require a resource bundle for Concat export");
 let wire=serde_json::to_value(project)?;
 let mut order=project.timeline_order.clone();if order.is_empty(){order.push(project.root.clone());order.extend(project.compositions.keys().filter(|id|*id!=&project.root).cloned());}
 let mut media=BTreeMap::new();let mut bindings=BTreeMap::new();
 for (node_id,node) in &project.compositions{for (source_id,source) in &node.content.sources{let id=source_id.as_str();
  let path=paths.get(id).with_context(||format!("Media {id}: supply an explicit Concat file path"))?;ensure!(!path.is_empty()&&path.len()<=4096&&!path.contains('\0'),"Invalid source path for {id}");
  if let Some(previous)=bindings.insert(id.to_owned(),source.clone()){ensure!(previous==*source,"Conflicting source metadata for {id}");}
  let source=serde_json::to_value(source)?;media.insert(id.to_owned(),json!({"id":id,"path":path,"name":wire["compositions"][node_id]["content"]["metadata"]["sourceNames"][id].as_str().unwrap_or(id),"kind":source["kind"],"duration":source["durationTicks"].as_u64().unwrap() as f64/1e6,"width":source["width"],"height":source["height"],"hasAudio":source["hasAudio"],"colorSpace":"sdr"}));
 }}
 for (source_id,entry) in &project.project_media {
  let id=source_id.as_str();
  let path=paths.get(id).with_context(||format!("Media {id}: supply an explicit Concat file path"))?;
  ensure!(!path.is_empty()&&path.len()<=4096&&!path.contains('\0'),"Invalid source path for {id}");
  if let Some(previous)=bindings.insert(id.to_owned(),entry.source.clone()){ensure!(previous==entry.source,"Project media descriptor differs from live source {id}");}
  let source=serde_json::to_value(&entry.source)?;
  media.insert(id.to_owned(),json!({"id":id,"path":path,"name":entry.name,"placeholder":entry.placeholder,"kind":source["kind"],"duration":source["durationTicks"].as_u64().unwrap() as f64/1e6,"width":source["width"],"height":source["height"],"hasAudio":source["hasAudio"],"colorSpace":"sdr"}));
  let item=media.get_mut(id).unwrap();for (key,value) in &entry.concat_metadata {item[key]=value.clone();}
 }
 ensure!(paths.keys().all(|id|bindings.contains_key(id)),"File path supplied for unknown source");
 let mut timelines=vec![];
 for id in order{
  let node=&wire["compositions"][&id];let content=&node["content"];let canvas=&content["canvas"];
  ensure!(node["instances"].as_array().is_some_and(Vec::is_empty),"Nested instances require a source timeline-media conversion");

  ensure!(content["captionTracks"].as_array().map_or(true,Vec::is_empty),"Caption tracks need source title conversion");
  ensure!(canvas["background"]==json!({"red":0.,"green":0.,"blue":0.,"alpha":1.})&&canvas["backgroundMode"].as_str().unwrap_or("color")=="color","Concat output currently requires its opaque black source canvas");
  let metadata=&content["metadata"];for (name,value) in metadata.as_object().into_iter().flatten(){if name=="dropFrame"&&value==false||name=="sequenceStartFrame"&&value.as_i64()==Some(0){continue}if !matches!(name.as_str(),"sequenceName"|"clipNames"|"sourceNames"|"concatLaneGroups"){ensure!(value.is_null()||value.as_object().is_some_and(serde_json::Map::is_empty)||value.as_array().is_some_and(Vec::is_empty),"Metadata {name} needs an explicit source mapping");}}
  let width=canvas["width"].as_f64().context("Canvas width missing")?;let height=canvas["height"].as_f64().context("Canvas height missing")?;
  let mut tracks=vec![];let mut clips=vec![];let mut end=0u64;
  for track in content["tracks"].as_array().context("Tracks missing")?.iter().rev(){
   ensure!(matches!(track["kind"].as_str(),Some("text"|"graphic"|"video"|"image"|"audio")),"Unsupported native track kind");
   ensure!(track["locked"]!=true,"Locked lane state needs a source workspace mapping");ensure!(track["solo"]!=true,"Solo lane state needs a source workspace mapping");ensure!(track["kind"]=="video"||track["transitions"].as_array().map_or(true,Vec::is_empty),"Transitions require a source video lane");
   let track_clip_start=clips.len();
   let mut kind=None;
   for clip in track["clips"].as_array().context("Clips missing")?{
    let media_kind=if matches!(track["kind"].as_str(),Some("video"|"image"|"audio")){track["kind"].as_str()}else{None};
    let mut view=clip.clone();
    if media_kind.is_some(){let(start,duration)=media_timing(clip)?;view["timelineStartTick"]=json!(start);view["timelineEndTick"]=json!(start.checked_add(duration).context("Media end overflow")?);}
    let mut converted=if media_kind==Some("audio"){json!({"id":clip["id"],"kind":"audio","mediaId":clip["sourceId"],"start":view["timelineStartTick"].as_u64().unwrap() as f64/1e6,"duration":(view["timelineEndTick"].as_u64().unwrap()-view["timelineStartTick"].as_u64().unwrap()) as f64/1e6})}else{visual(&view,width,height,media_kind).with_context(||format!("clip {}",clip["id"]))?};
    if let Some(kind)=media_kind{media_controls(clip,kind,&mut converted)?;}

    let current=converted["kind"].as_str().unwrap().to_owned();if let Some(kind)=&kind{ensure!(kind==&current,"Mixed retained kinds need separate source lanes");}else{kind=Some(current);}
    converted["trackId"]=track["id"].clone();converted["name"]=metadata["clipNames"].get(clip["id"].as_str().unwrap()).cloned().unwrap_or_else(||clip["id"].clone());
    end=end.max(view["timelineEndTick"].as_u64().unwrap());clips.push(converted);
   }
   for transition in track["transitions"].as_array().into_iter().flatten(){let incoming=clips.iter_mut().find(|clip|clip["id"]==transition["toClipId"]&&clip["trackId"]==track["id"]).context("Source transition incoming clip missing")?;ensure!(incoming.get("transitionIn").is_none(),"Multiple transitions enter one source clip");incoming["transitionIn"]=super::concat_project_transitions::export(transition)?;}
   let transition_order=track["transitions"].as_array().cloned().unwrap_or_default();
   clips[track_clip_start..].sort_by_key(|clip|transition_order.iter().position(|transition|transition["toClipId"]==clip["id"]).map_or(0,|index|index+1));
   tracks.push(json!({"id":track["id"],"name":track["name"],"visible":track["hidden"]!=true,"muted":track["muted"]==true}));
  }
  ensure!(node["durationUs"].as_u64().map_or(true,|duration|duration==end),"Empty timeline tail needs a source duration representation");
  let rate=&canvas["fpsRate"];let (num,den)=if rate.is_object(){(rate["numerator"].clone(),rate["denominator"].clone())}else{(canvas["fpsMilli"].clone(),json!(1000))};
  timelines.push(json!({"id":id,"name":metadata["sequenceName"].as_str().unwrap_or(&id),"video":{"width":width as u32,"height":height as u32,"rateNum":num,"rateDen":den,"colorSpace":"sdr"},"tracks":tracks,"clips":clips}));
 }
 let mut document=json!({"version":1,"media":media.into_values().collect::<Vec<_>>(),"activeTimelineId":project.root,"timelines":timelines});
 ensure!(serde_json::to_vec(&document)?.len()<=4*1024*1024,"Concat JSON exceeds the import budget");
 let report=super::concat_project_import::import(serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}))?)?;
 ensure!(report.status=="converted","Reverse conversion failed native admission: {}",report.diagnostics.join("; "));
 ensure!(report.project.as_ref().context("Missing reconstructed project")?.project_media==project.project_media,"Reverse conversion changes project media properties");
 // Compare normalized media ownership after the existing importer has reconstructed it.
 let restored=serde_json::to_value(report.project.context("Missing reconstructed project")?)?;
 for timeline in document["timelines"].as_array().unwrap(){let restored_id=&report.timeline_ids[timeline["id"].as_str().unwrap()];let original=&wire["compositions"][timeline["id"].as_str().unwrap()]["content"];let rebuilt=&restored["compositions"][restored_id]["content"];
  for (source_index,track) in original["tracks"].as_array().unwrap().iter().rev().enumerate(){if !matches!(track["kind"].as_str(),Some("audio"|"video"|"image")){continue}
   let native_index=original["tracks"].as_array().unwrap().len()-1-source_index;
   let grouped=super::concat_lane_groups::groups(&project.compositions[timeline["id"].as_str().unwrap()].content)?.iter().any(|(video,image)|*video==native_index||*image==native_index);
   if grouped && track["clips"].as_array().is_some_and(Vec::is_empty) && track["transitions"].as_array().map_or(true,Vec::is_empty){continue}
   let rebuilt_track=rebuilt["tracks"].as_array().unwrap().iter().find(|item|item["id"]==format!("lane-{source_index}-{}",track["kind"].as_str().unwrap())).context("Missing reconstructed media lane")?;
   ensure!(track["clips"]==rebuilt_track["clips"],"Media lane {} has native controls that cannot be exported losslessly",track["id"]);
   let normalize=|value:&Value|{let mut values=value.as_array().cloned().unwrap_or_default();for transition in &mut values{transition.as_object_mut().unwrap().remove("id");}values};
   ensure!(normalize(&track["transitions"])==normalize(&rebuilt_track["transitions"]),"Source transition timing/endpoints cannot be exported losslessly");
  }
 }
 // Physical lanes have passed the existing per-clip losslessness check.
 // Rejoin only the explicit source siblings, then qualify the merged result.
 let mut merged=false;
 for timeline in document["timelines"].as_array_mut().unwrap(){
  let id=timeline["id"].as_str().unwrap();let content=&project.compositions[id].content;
  for (video,image) in super::concat_lane_groups::groups(content)? {
   let owner=content.tracks[video].id().as_str();let sibling=content.tracks[image].id().as_str();
   for clip in timeline["clips"].as_array_mut().unwrap(){if clip["trackId"]==sibling{clip["trackId"]=json!(owner);}}
   timeline["tracks"].as_array_mut().unwrap().retain(|track|track["id"]!=sibling);merged=true;
  }
 }
 if merged {
  let report=super::concat_project_import::import(serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}))?)?;
  ensure!(report.status=="converted","Merged Concat lane failed native admission: {}",report.diagnostics.join("; "));
  let restored=serde_json::to_value(report.project.context("Missing merged project")?)?;
  for timeline in document["timelines"].as_array().unwrap(){
   let id=timeline["id"].as_str().unwrap();let original=&wire["compositions"][id]["content"];let rebuilt=&restored["compositions"][&report.timeline_ids[id]]["content"];
   let content=&project.compositions[id].content;let groups=super::concat_lane_groups::groups(content)?;
   for (index,track) in original["tracks"].as_array().unwrap().iter().enumerate(){
    if !matches!(track["kind"].as_str(),Some("audio"|"video"|"image")){continue}
    if groups.iter().any(|(video,image)|*video==index||*image==index) && track["clips"].as_array().is_some_and(Vec::is_empty) && track["transitions"].as_array().map_or(true,Vec::is_empty){continue}
    let owner=groups.iter().find(|(_,image)|*image==index).map_or(track["id"].as_str().unwrap(),|(video,_)|content.tracks[*video].id().as_str());
    let source_index=timeline["tracks"].as_array().unwrap().iter().position(|item|item["id"]==owner).context("Merged source lane missing")?;
    let rebuilt_track=rebuilt["tracks"].as_array().unwrap().iter().find(|item|item["id"]==format!("lane-{source_index}-{}",track["kind"].as_str().unwrap())).context("Merged native lane missing")?;
    ensure!(track["clips"]==rebuilt_track["clips"],"Merged source lane changes native clip controls");
    let normalize=|value:&Value|{let mut values=value.as_array().cloned().unwrap_or_default();for value in &mut values{value.as_object_mut().unwrap().remove("id");}values};
    ensure!(normalize(&track["transitions"])==normalize(&rebuilt_track["transitions"]),"Merged source lane changes transitions");
   }
  }
 }
 Ok(document)
}

fn media_timing(clip:&Value)->Result<(u64,u64)>{
 if clip["placement"].is_object(){let placement:super::composition::ClipPlacement=serde_json::from_value(clip["placement"].clone())?;ensure!(placement.speed_ramp.is_none()&&placement.source_timeline_offset_tick.is_none()&&placement.source_head_timeline_tick.is_none(),"Source speed curves and retained clock windows need conversion");Ok((placement.timeline_start_tick,placement.timeline_duration_ticks()?))}
 else{Ok((clip["timelineStartTick"].as_u64().context("Image start missing")?,clip["durationTicks"].as_u64().context("Image duration missing")?))}
}
fn media_controls(clip:&Value,kind:&str,out:&mut Value)->Result<()>{
 if kind=="image"{
  if let Some(value)=clip.get("concatSourceTiming") {
   let timing:super::composition::ImageSourceTiming=serde_json::from_value(value.clone())?;timing.validate()?;
   out["sourceStart"]=json!(timing.source_start);out["speed"]=json!(timing.speed);
   if let Some(points)=timing.speed_curve {out["speedCurve"]=serde_json::to_value(points)?;}
  }
  return Ok(())
 }
 let placement=&clip["placement"];out["sourceStart"]=json!(placement["sourceInTick"].as_u64().context("Media source start missing")? as f64/1e6);out["speed"]=placement["speed"].clone();out["preservePitch"]=json!(true);
 let audio=kind=="audio";out["volume"]=json!(constant(&clip[if audio{"gain"}else{"audioGain"}])?);out["muted"]=json!(if audio{clip["enabled"]==false}else{clip["sourceAudioEnabled"]==false});
 out["fadeIn"]=json!(clip[if audio{"fadeInTicks"}else{"audioFadeInTicks"}].as_u64().unwrap_or(0) as f64/1e6);out["fadeOut"]=json!(clip[if audio{"fadeOutTicks"}else{"audioFadeOutTicks"}].as_u64().unwrap_or(0) as f64/1e6);
 if let Some(stream)=clip.get("audioStream"){out["audioStream"]=stream.clone();}if let Some(origin)=clip.get("detachedFrom"){out["detachedFrom"]=origin.clone();}
 let keys=out.as_object_mut().unwrap().entry("keys").or_insert(json!([])).as_array_mut().context("Invalid property keys")?;
 for key in clip["concatGainKeys"].as_array().into_iter().flatten(){let mut key=source_keys(key)?;key["property"]=json!("volume");keys.push(key);}
 let specs:Vec<super::filmcraft_dsp::EffectSpec>=serde_json::from_value(clip.get("dspEffects").cloned().unwrap_or(json!([])))?;
 let links=specs.iter().map(|spec|super::concat_effect_upgrade::AppliedFilter{id:spec.id.clone(),params:spec.params.iter().map(|(name,value)|(name.clone(),*value as f64)).collect(),enabled:!spec.bypass,keys:spec.concat_source_keys.clone()}).collect::<Vec<_>>();
 ensure!(super::concat_audio_import::convert(&links)?==specs,"Native DSP animation cannot be exported losslessly");let mut filters=serde_json::to_value(links)?;for link in filters.as_array_mut().unwrap(){if let Some(keys)=link.get_mut("keys"){*keys=source_keys(keys)?;}}out["filters"]=filters;Ok(())
}
