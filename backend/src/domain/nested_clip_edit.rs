//! Atomic native-wire clip relocation; opaque authored fields remain intact.
use super::{composition::ClipPlacement, nested_composition::NestedCompositionProject};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct ClipMove {
    pub from_composition: String,
    pub from_track_id: String,
    pub clip_id: String,
    pub to_composition: String,
    pub to_track_id: String,
    pub start_us: u64,
    #[serde(default)] pub create_track: bool,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ClipMoveResult {
    pub project: NestedCompositionProject,
    pub changed: bool,
    pub affected_compositions: Vec<String>,
    pub moved_clip_ids: Vec<String>,
}
struct Prepared {
    request: ClipMove,
    clip: Value,
    template: Value,
    kind: String,
    old_start: u64,
    end: u64,
    sources: BTreeMap<String, Value>,
    source_names: BTreeMap<String, Value>,
    name: Option<Value>,
    versions: Option<Value>,
}
fn track<'a>(project: &'a Value, composition: &str, id: &str) -> Result<&'a Value> {
    project["compositions"][composition]["content"]["tracks"].as_array().context("Композиция не найдена")?.iter().find(|track| track["id"] == id).context("Дорожка не найдена")
}
fn track_mut<'a>(project: &'a mut Value, composition: &str, id: &str) -> Result<&'a mut Value> {
    project["compositions"][composition]["content"]["tracks"].as_array_mut().context("Композиция не найдена")?.iter_mut().find(|track| track["id"] == id).context("Дорожка не найдена")
}
fn span(clip: &Value, kind: &str) -> Result<(u64, u64)> {
    if matches!(kind, "video" | "audio") {
        let placement: ClipPlacement = serde_json::from_value(clip["placement"].clone())?;
        Ok((placement.timeline_start_tick, placement.timeline_end_tick()?))
    } else {
        let start = clip["timelineStartTick"].as_u64().context("Не задано начало клипа")?;
        let end = if kind == "image" { start.checked_add(clip["durationTicks"].as_u64().context("Не задана длительность изображения")?).context("Переполнение конца клипа")? } else { clip["timelineEndTick"].as_u64().context("Не задан конец клипа")? };
        Ok((start, end))
    }
}
fn same_motion(a: &Prepared, b: &Prepared) -> bool {
    a.request.to_composition == b.request.to_composition && a.request.to_track_id == b.request.to_track_id && i128::from(a.request.start_us) - i128::from(a.old_start) == i128::from(b.request.start_us) - i128::from(b.old_start)
}
fn plan<'a>(moves: &'a [Prepared], composition: &str, track: &str, clip: &str) -> Option<&'a Prepared> {
    moves.iter().find(|item| item.request.from_composition == composition && item.request.from_track_id == track && item.request.clip_id == clip)
}

// Treatment native parameter tracks are absolute; normalized Concat keys and the
// retained local clock stay attached to the clip without a coordinate change.
fn shift_treatment_effect_keys(value: &mut Value, delta_us: i128) -> Result<()> {
    match value {
        Value::Object(object) if object.get("mode").and_then(Value::as_str)==Some("keyframes") => {
            let track=object.get_mut("track").context("Не задан трек ключей эффекта")?;
            let base=track["timeBase"].as_u64().context("Не задана шкала ключей эффекта")?;
            let delta=(delta_us*i128::from(base)+500_000).div_euclid(1_000_000);
            for key in track["keyframes"].as_array_mut().context("Не заданы ключи эффекта")? {
                let tick=i128::from(key["tick"].as_i64().context("Некорректное время ключа эффекта")?)+delta;
                ensure!(tick.abs()<=9_007_199_254_740_991,"Время ключа эффекта выходит за точный диапазон");
                key["tick"]=json!(i64::try_from(tick)?);
            }
        }
        Value::Object(object) => for child in object.values_mut() { shift_treatment_effect_keys(child,delta_us)?; },
        Value::Array(values) => for child in values { shift_treatment_effect_keys(child,delta_us)?; },
        _ => {}
    }
    Ok(())
}

pub fn move_clips(project: &NestedCompositionProject, requests: &[ClipMove]) -> Result<ClipMoveResult> {
    project.dependency_order()?;
    ensure!(!requests.is_empty() && requests.len() <= 512, "Выберите от 1 до 512 клипов");
    let original = serde_json::to_value(project)?;
    let mut prepared = Vec::new();
    let mut subjects = BTreeSet::new();
    for request in requests {
        ensure!(subjects.insert((&request.from_composition, &request.from_track_id, &request.clip_id)), "Клип выбран для переноса несколько раз");
        ensure!(request.start_us <= 9_007_199_254_740_991, "Позиция переноса выходит за точный диапазон");
        super::composition::TrackId::parse(request.to_track_id.clone())?;
        let source_track = track(&original, &request.from_composition, &request.from_track_id)?;
        ensure!(source_track["locked"] != true, "Исходная дорожка заблокирована");
        let kind = source_track["kind"].as_str().context("Не задан тип дорожки")?;
        let clip = source_track["clips"].as_array().unwrap().iter().find(|clip| clip["id"] == request.clip_id).context("Клип не найден")?;
        let target = original["compositions"].get(&request.to_composition).context("Целевая композиция не найдена")?;
        let target_track = target["content"]["tracks"].as_array().unwrap().iter().find(|track| track["id"] == request.to_track_id);
        if let Some(target_track) = target_track { ensure!(target_track["locked"] != true, "Целевая дорожка заблокирована"); ensure!(target_track["kind"] == kind, "Тип целевой дорожки не совпадает"); }
        else { ensure!(request.create_track, "Целевая дорожка не найдена"); }
        let (old_start, old_end) = span(clip, kind)?;
        if request.from_composition == request.to_composition && request.from_track_id == request.to_track_id && request.start_us == old_start { continue; }
        let end = request.start_us.checked_add(old_end.checked_sub(old_start).context("Некорректный исходный диапазон")?).context("Переполнение позиции переноса")?;
        ensure!(end <= 9_007_199_254_740_991, "Конец переноса выходит за точный диапазон");
        let metadata = &original["compositions"][&request.from_composition]["content"]["metadata"];
        let versions = metadata["mediaVersions"].get(&request.clip_id).filter(|value| !value.is_null()).cloned();
        let mut source_ids = BTreeSet::new();
        if let Some(id) = clip["sourceId"].as_str() { source_ids.insert(id.to_owned()); }
        if let Some(pair) = &versions { for key in ["originalSourceId", "copySourceId"] { source_ids.insert(pair[key].as_str().context("Некорректная ссылка на версию медиа")?.to_owned()); } }
        if let Some(cutout)=versions.as_ref().and_then(|pair|pair.get("cutout")){for key in ["sourceId","maskSourceId"]{source_ids.insert(cutout[key].as_str().context("Некорректная ссылка на маску")?.to_owned());}if let Some(strokes)=cutout["correction"]["strokes"].as_array(){for region in strokes.iter().filter_map(|s|s.get("region")){for key in ["sourceId","maskSourceId"]{source_ids.insert(region[key].as_str().context("Некорректная ссылка на область")?.to_owned());}}}}
        let mut sources = BTreeMap::new(); let mut source_names = BTreeMap::new();
        for id in source_ids {
            let source = original["compositions"][&request.from_composition]["content"]["sources"].get(&id).context("Не найден источник или сохранённая версия медиа")?;
            if let Some(existing) = target["content"]["sources"].get(&id) { ensure!(existing == source, "Метаданные источника в целевом таймлайне отличаются"); }
            sources.insert(id.clone(), source.clone());
            if let Some(name) = metadata["sourceNames"].get(&id) { source_names.insert(id, name.clone()); }
        }
        let mut moved = clip.clone();
        if kind == "graphic" && !moved["concatTreatment"].is_null() {
            shift_treatment_effect_keys(&mut moved["effects"], i128::from(request.start_us)-i128::from(old_start))?;
        }
        if matches!(kind, "video" | "audio") {
            let mut placement: ClipPlacement = serde_json::from_value(moved["placement"].clone())?;
            if let Some(retained) = &placement.concat_retime {
                retained.validate_placement(&placement)?;
                let rate = &target["content"]["canvas"];
                let num = rate["fpsRate"]["numerator"].as_u64().unwrap_or_else(|| rate["fpsMilli"].as_u64().unwrap_or(0));
                let den = rate["fpsRate"]["denominator"].as_u64().unwrap_or(1000);
                ensure!(u128::from(retained.rate_num)*u128::from(den) == u128::from(retained.rate_den)*u128::from(num), "source move requires unchanged frame rate");
                let raw = retained.document_clock.as_ref().context("source move requires original document clock")?.relocate(request.start_us)?;
                placement.concat_retime.as_mut().unwrap().document_clock = Some(raw);
            }
            placement.timeline_start_tick = request.start_us;
            if let Some(retained) = &placement.concat_retime { retained.validate_placement(&placement)?; }
            moved["placement"] = serde_json::to_value(placement)?;
        }
        else { moved["timelineStartTick"] = json!(request.start_us); if kind != "image" { moved["timelineEndTick"] = json!(end); } }
        let mut template = source_track.clone(); template["id"] = json!(request.to_track_id); template["clips"] = json!([]);
        if kind == "video" { template["transitions"] = json!([]); }
        prepared.push(Prepared { request: request.clone(), clip: moved, template, kind: kind.into(), old_start, end, sources, source_names, name: metadata["clipNames"].get(&request.clip_id).cloned(), versions });
    }
    if prepared.is_empty() { return Ok(ClipMoveResult { project: project.clone(), changed: false, affected_compositions: vec![], moved_clip_ids: vec![] }); }
    let mut transitions = Vec::new();
    for (composition_id, node) in original["compositions"].as_object().unwrap() {
        for source_track in node["content"]["tracks"].as_array().unwrap() {
            let track_id = source_track["id"].as_str().unwrap();
            for transition in source_track["transitions"].as_array().into_iter().flatten() {
                let endpoints: Vec<_> = ["fromClipId", "toClipId"].into_iter().filter_map(|key| transition[key].as_str()).collect();
                if !endpoints.iter().any(|id| plan(&prepared, composition_id, track_id, id).is_some()) { continue; }
                let plans: Vec<_> = endpoints.iter().map(|id| plan(&prepared, composition_id, track_id, id).context("Перенесите вместе оба клипа перехода")).collect::<Result<_>>()?;
                let first = plans.first().context("Не найден клип перехода")?;
                ensure!(plans.iter().all(|item| same_motion(first, item)), "Клипы перехода должны переноситься на одну дорожку с общим сдвигом");
                let mut shifted = transition.clone();
                if !transition["timeline"].is_null() {
                    let mut timeline: super::transition_timeline::TransitionTimeline = serde_json::from_value(transition["timeline"].clone())?;
                    let (start, _) = timeline.source_span().map_err(|error| anyhow::anyhow!("Некорректное окно перехода: {error:?}"))?;
                    let delta = i128::from(first.request.start_us) - i128::from(first.old_start);
                    timeline.start_tick = super::caption_time::CaptionDelta::SourceTicks(i64::try_from(i128::from(start) + delta * super::caption_time::SOURCE_TICKS_PER_MICROSECOND)?);
                    timeline.source_span().map_err(|error| anyhow::anyhow!("Некорректное окно перехода: {error:?}"))?;
                    shifted["timeline"] = serde_json::to_value(timeline)?;
                }
                transitions.push((composition_id.clone(), track_id.to_owned(), first.request.to_composition.clone(), first.request.to_track_id.clone(), shifted));
            }
            if source_track["kind"] == "audio" {
                let mut clips: Vec<_> = source_track["clips"].as_array().unwrap().iter().collect();
                clips.sort_by_key(|clip| clip["placement"]["timelineStartTick"].as_u64().unwrap());
                for pair in clips.windows(2) {
                    if pair[1]["crossfadeInTicks"].as_u64().unwrap_or(0) == 0 { continue; }
                    let a = plan(&prepared, composition_id, track_id, pair[0]["id"].as_str().unwrap());
                    let b = plan(&prepared, composition_id, track_id, pair[1]["id"].as_str().unwrap());
                    if a.is_some() || b.is_some() { ensure!(a.zip(b).is_some_and(|(a,b)| same_motion(a,b)), "Перенесите вместе оба участника аудиокроссфейда"); }
                }
            }
        }
    }
    let mut changed = original.clone();
    for item in &prepared {
        track_mut(&mut changed, &item.request.from_composition, &item.request.from_track_id)?["clips"].as_array_mut().unwrap().retain(|clip| clip["id"] != item.request.clip_id);
        if item.request.from_composition != item.request.to_composition {
            if let Some(metadata) = changed["compositions"][&item.request.from_composition]["content"].get_mut("metadata") {
                for field in ["clipNames", "mediaVersions"] { if let Some(map) = metadata.get_mut(field).and_then(Value::as_object_mut) { map.remove(&item.request.clip_id); } }
            }
        }
    }
    for (source, source_track, _, _, transition) in &transitions {
        track_mut(&mut changed, source, source_track)?["transitions"].as_array_mut().unwrap().retain(|item| item["id"] != transition["id"]);
    }
    for item in &prepared {
        let target = &mut changed["compositions"][&item.request.to_composition];
        ensure!(!target["content"]["tracks"].as_array().unwrap().iter().flat_map(|track| track["clips"].as_array().unwrap()).any(|clip| clip["id"] == item.request.clip_id), "В целевом таймлайне уже есть клип с таким идентификатором");
        for (id, source) in &item.sources {
            if let Some(existing) = target["content"]["sources"].get(id) { ensure!(existing == source, "Метаданные источника в целевом таймлайне отличаются"); }
            target["content"]["sources"][id] = source.clone();
        }
        if item.request.from_composition != item.request.to_composition {
            let metadata = &mut target["content"]["metadata"];
            if metadata.is_null() && (item.name.is_some() || item.versions.is_some() || !item.source_names.is_empty()) { *metadata = json!({}); }
            if !metadata.is_null() {
                for (field, value) in [("clipNames", &item.name), ("mediaVersions", &item.versions)] {
                    if value.is_some() && metadata.get(field).is_none_or(Value::is_null) { metadata[field] = json!({}); }
                    if let Some(map) = metadata.get_mut(field).and_then(Value::as_object_mut) { map.remove(&item.request.clip_id); if let Some(value) = value { map.insert(item.request.clip_id.clone(), value.clone()); } }
                }
                if !item.source_names.is_empty() { if metadata["sourceNames"].is_null() { metadata["sourceNames"] = json!({}); } for (id, name) in &item.source_names { metadata["sourceNames"].as_object_mut().unwrap().entry(id.clone()).or_insert_with(|| name.clone()); } }
            }
        }
        let tracks = target["content"]["tracks"].as_array_mut().unwrap();
        if !tracks.iter().any(|track| track["id"] == item.request.to_track_id) { tracks.push(item.template.clone()); }
        let destination = tracks.iter_mut().find(|track| track["id"] == item.request.to_track_id).unwrap();
        ensure!(destination["kind"] == item.kind && destination["locked"] != true, "Конфликт новой целевой дорожки");
        for clip in destination["clips"].as_array().unwrap() { let (start,end) = span(clip, &item.kind)?; ensure!(item.end <= start || item.request.start_us >= end, "Клипы накладываются на целевой дорожке"); }
        destination["clips"].as_array_mut().unwrap().push(item.clip.clone());
        if let Some(duration) = target["durationUs"].as_u64() { if item.end > duration { target["durationUs"] = json!(item.end); } }
    }
    for (_, _, target, target_track, transition) in transitions {
        let destination = track_mut(&mut changed, &target, &target_track)?;
        if destination["transitions"].is_null() { destination["transitions"] = json!([]); }
        destination["transitions"].as_array_mut().unwrap().push(transition);
    }
    rebind_dependencies(&original, &mut changed, &prepared)?;
    move_link_groups(&original, &mut changed, &prepared)?;
    let updated: NestedCompositionProject = serde_json::from_value(changed)?;
    updated.dependency_order()?;
    let mut affected: BTreeSet<String> = prepared.iter().flat_map(|item| [item.request.from_composition.clone(), item.request.to_composition.clone()]).collect();
    loop {
        let before = affected.len();
        for (id, node) in &project.compositions { if node.instances.iter().any(|instance| affected.contains(&instance.timing.composition)) { affected.insert(id.clone()); } }
        if affected.len() == before { break; }
    }
    Ok(ClipMoveResult { changed: true, project: updated, affected_compositions: affected.into_iter().collect(), moved_clip_ids: prepared.iter().map(|item| item.request.clip_id.clone()).collect() })
}

fn move_link_groups(original:&Value,changed:&mut Value,moves:&[Prepared])->Result<()> {
    for (composition,node) in original["compositions"].as_object().unwrap() {
        for (group,members) in node["content"]["metadata"]["clipLinks"].as_object().into_iter().flatten() {
            let members=members.as_array().context("Некорректная группа связанных клипов")?;
            let find=|id:&str|moves.iter().find(|item|item.request.from_composition==*composition && item.request.clip_id==id);
            let Some(first)=members.iter().filter_map(|id|id.as_str().and_then(find)).next() else {continue;};
            let delta=i128::from(first.request.start_us)-i128::from(first.old_start);
            for member in members {
                let id=member.as_str().context("Некорректный связанный клип")?;
                if let Some(item)=find(id) {
                    ensure!(item.request.to_composition==first.request.to_composition && i128::from(item.request.start_us)-i128::from(item.old_start)==delta,"Связанные клипы должны переноситься вместе с общим сдвигом");
                }else{ensure!(first.request.to_composition==*composition && delta==0,"Выберите все связанные клипы для переноса");}
            }
            if first.request.to_composition==*composition {continue;}
            changed["compositions"][composition]["content"]["metadata"]["clipLinks"].as_object_mut().unwrap().remove(group);
            let metadata=&mut changed["compositions"][&first.request.to_composition]["content"]["metadata"];
            if metadata.is_null(){*metadata=serde_json::to_value(super::metadata_burnin::CompositionMetadata::default())?;}
            if metadata["clipLinks"].is_null(){metadata["clipLinks"]=json!({});}
            let target=metadata["clipLinks"].as_object_mut().unwrap();
            let mut id=group.clone();let mut serial=0usize;
            while target.contains_key(&id){id=format!("clip-link-{serial}");serial+=1;}
            target.insert(id,Value::Array(members.clone()));
        }
    }
    Ok(())
}

fn effect_reference(effect: &Value) -> Option<(&str, u64)> {
    let field = match effect["kind"].as_str()? { "track_matte" => "matte", "gradient_wipe" | "compound_blur" => "layer", _ => return None };
    let ordinal = effect["parameters"][field].as_u64().unwrap_or(0);
    (ordinal > 0).then_some((field, ordinal))
}
fn visual_tracks(content: &Value) -> Vec<&Value> {
    content["tracks"].as_array().unwrap().iter().filter(|track| matches!(track["kind"].as_str(),Some("video" | "image" | "color" | "graphic"))).collect()
}
fn rebind_dependencies(original: &Value, changed: &mut Value, moves: &[Prepared]) -> Result<()> {
    let mut empty_controls = BTreeMap::new();
    for item in moves {
        if item.request.from_composition==item.request.to_composition {continue;}
        let layers=visual_tracks(&original["compositions"][&item.request.from_composition]["content"]);
        for effect in item.clip["effects"].as_array().into_iter().flatten() {
            let Some((_,ordinal))=effect_reference(effect) else {continue;};
            let Some(control)=layers.get(ordinal as usize-1) else {continue;};
            if !control["clips"].as_array().unwrap().is_empty(){continue;}
            let key=(item.request.from_composition.clone(),control["id"].as_str().unwrap().to_owned(),item.request.to_composition.clone());
            if empty_controls.contains_key(&key){continue;}
            let target=&mut changed["compositions"][&item.request.to_composition]["content"];
            let mut serial=0usize;
            let id=loop {
                let id=format!("empty-control-{serial}");serial+=1;
                if !target["tracks"].as_array().unwrap().iter().any(|track|track["id"]==id) && !target["captionTracks"].as_array().into_iter().flatten().any(|track|track["id"]==id) {break id;}
            };
            let mut cloned=(*control).clone();cloned["id"]=json!(id);
            target["tracks"].as_array_mut().unwrap().push(cloned);
            empty_controls.insert(key,id);
        }
    }
    // Appending a lane must not turn an unrelated missing-layer fallback into a live source.
    for (id,node) in original["compositions"].as_object().unwrap() {
        let old_layers=visual_tracks(&node["content"]);
        let new_layers=visual_tracks(&changed["compositions"][id]["content"]);
        for lane in node["content"]["tracks"].as_array().unwrap() {
            for clip in lane["clips"].as_array().unwrap() {
                if plan(moves,id,lane["id"].as_str().unwrap(),clip["id"].as_str().unwrap()).is_some_and(|p|p.request.to_composition!=*id){continue;}
                for effect in clip["effects"].as_array().into_iter().flatten() {
                    if let Some((_,ordinal))=effect_reference(effect) {
                        ensure!(!(ordinal as usize>old_layers.len() && ordinal as usize<=new_layers.len()),"Новая дорожка меняет отсутствовавшую ссылку существующего эффекта");
                    }
                }
            }
        }
    }
    let mut replacements = Vec::new();
    for item in moves {
        let source = &original["compositions"][&item.request.from_composition]["content"];
        let source_layers = visual_tracks(source);
        if let Some(ordinal) = source_layers.iter().position(|lane|lane["id"]==item.request.from_track_id).map(|index|index as u64+1) {
            for lane in source["tracks"].as_array().unwrap() {
                for clip in lane["clips"].as_array().unwrap() {
                    if clip["effects"].as_array().into_iter().flatten().any(|effect|effect_reference(effect).is_some_and(|(_,n)|n==ordinal)) {
                        let dependent=plan(moves,&item.request.from_composition,lane["id"].as_str().unwrap(),clip["id"].as_str().unwrap()).context("Нельзя оставить зависимый клип без управляющего слоя")?;
                        ensure!(dependent.request.to_composition==item.request.to_composition && i128::from(dependent.request.start_us)-i128::from(dependent.old_start)==i128::from(item.request.start_us)-i128::from(item.old_start),"Зависимые клипы должны переноситься вместе с управляющим слоем");
                    }
                }
            }
        }
        let target_layers = visual_tracks(&changed["compositions"][&item.request.to_composition]["content"]);
        for (effect_index,effect) in item.clip["effects"].as_array().into_iter().flatten().enumerate() {
            let Some((field,ordinal)) = effect_reference(effect) else { continue; };
            let Some(control) = source_layers.get(ordinal as usize-1) else {
                ensure!(ordinal as usize > target_layers.len(), "Отсутствующий управляющий слой совпадает с новым слоем в целевом таймлайне");
                continue;
            };
            let control_id = control["id"].as_str().unwrap();
            let clips = control["clips"].as_array().unwrap();
            if clips.is_empty() {
                if item.request.from_composition==item.request.to_composition {continue;}
                let key=(item.request.from_composition.clone(),control_id.to_owned(),item.request.to_composition.clone());
                let id=empty_controls.get(&key).context("Копия пустого управляющего слоя не найдена")?;
                let index=target_layers.iter().position(|track|track["id"]==*id).context("Пустой управляющий слой не найден")?;
                replacements.push((item.request.clone(),effect_index,field.to_owned(),index+1));
                continue;
            }
            let delta = i128::from(item.request.start_us)-i128::from(item.old_start);
            if item.request.from_composition==item.request.to_composition && delta==0 && clips.iter().all(|clip|plan(moves,&item.request.from_composition,control_id,clip["id"].as_str().unwrap()).is_none()) {continue;}
            let plans = clips.iter().map(|clip|plan(moves,&item.request.from_composition,control_id,clip["id"].as_str().unwrap()).context("Выберите все клипы управляющей дорожки вместе с зависимым клипом")).collect::<Result<Vec<_>>>()?;
            let first = plans[0];
            ensure!(plans.iter().all(|p|p.request.to_composition==item.request.to_composition && p.request.to_track_id==first.request.to_track_id && i128::from(p.request.start_us)-i128::from(p.old_start)==delta),"Управляющий слой должен переноситься целиком с тем же сдвигом");
            let target_index = target_layers.iter().position(|track|track["id"]==first.request.to_track_id).context("Целевой управляющий слой не найден")?;
            let target = target_layers[target_index];
            for flag in ["hidden","muted","solo"] {ensure!(control[flag].as_bool().unwrap_or(false)==target[flag].as_bool().unwrap_or(false),"Флаги целевой управляющей дорожки отличаются от исходной");}
            let old_ids:BTreeSet<_>=clips.iter().map(|clip|clip["id"].as_str().unwrap()).collect();
            let new_ids:BTreeSet<_>=target["clips"].as_array().unwrap().iter().map(|clip|clip["id"].as_str().unwrap()).collect();
            ensure!(old_ids==new_ids,"Целевая управляющая дорожка содержит посторонние клипы");
            for lane in source["tracks"].as_array().unwrap() {
                for clip in lane["clips"].as_array().unwrap() {
                    if clip["effects"].as_array().into_iter().flatten().any(|effect|effect_reference(effect).is_some_and(|(_,n)|n==ordinal)) {
                        ensure!(plan(moves,&item.request.from_composition,lane["id"].as_str().unwrap(),clip["id"].as_str().unwrap()).is_some(),"Управляющий слой используется клипом, который остаётся в исходном таймлайне");
                    }
                }
            }
            replacements.push((item.request.clone(),effect_index,field.to_owned(),target_index+1));
        }
    }
    for (request,index,field,ordinal) in replacements {
        let lane=track_mut(changed,&request.to_composition,&request.to_track_id)?;
        let clip=lane["clips"].as_array_mut().unwrap().iter_mut().find(|clip|clip["id"]==request.clip_id).unwrap();
        clip["effects"][index]["parameters"][field]=json!(ordinal);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> NestedCompositionProject {
        let value: Value = serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        serde_json::from_value(value["createdTimeline"].clone()).unwrap()
    }
    fn request(clip: &str, source_track: &str, target_track: &str, start: u64) -> ClipMove {
        ClipMove { from_composition: "root".into(), from_track_id: source_track.into(), clip_id: clip.into(), to_composition: "new-timeline".into(), to_track_id: target_track.into(), start_us: start, create_track: false }
    }
    #[test]
    fn atomic_av_transfer_preserves_opaque_clip_data_versions_sources_and_input() {
        for reverse in [false, true] {
            let mut value = serde_json::to_value(fixture()).unwrap();
            value["compositions"]["root"]["content"]["metadata"] = json!({"clipNames":{"picture":"Named picture"},"mediaVersions":{"picture":{"originalSourceId":"source","copySourceId":"next-source"}}});
            if reverse {
                value["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["playbackMode"] = json!({"mode":"reverse"});
                value["compositions"]["root"]["content"]["tracks"][1]["clips"][0]["reversed"] = json!(true);
            }
            let project: NestedCompositionProject = serde_json::from_value(value).unwrap();
            let before = serde_json::to_value(&project).unwrap();
            let result = move_clips(&project, &[request("picture", "video", "video-1", 2_000_000), request("sound", "audio", "audio-1", 2_000_000)]).unwrap();
            assert!(result.changed);
            assert_eq!(result.moved_clip_ids, ["picture", "sound"]);
            let output = serde_json::to_value(&result.project).unwrap();
            let mut picture = before["compositions"]["root"]["content"]["tracks"][0]["clips"][0].clone();
            picture["placement"]["timelineStartTick"] = json!(2_000_000);
            let mut sound = before["compositions"]["root"]["content"]["tracks"][1]["clips"][0].clone();
            sound["placement"]["timelineStartTick"] = json!(2_000_000);
            assert_eq!(output["compositions"]["new-timeline"]["content"]["tracks"][0]["clips"][0], picture);
            assert_eq!(output["compositions"]["new-timeline"]["content"]["tracks"][2]["clips"][0], sound);
            assert_eq!(output["compositions"]["new-timeline"]["content"]["metadata"]["clipNames"]["picture"], "Named picture");
            assert_eq!(output["compositions"]["new-timeline"]["content"]["metadata"]["mediaVersions"]["picture"], before["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["picture"]);
            assert!(output["compositions"]["new-timeline"]["content"]["sources"].get("next-source").is_some());
            assert!(output["compositions"]["root"]["content"]["metadata"]["mediaVersions"].get("picture").is_none());
            assert_eq!(result.project.durations_us().unwrap()["new-timeline"], 6_000_000);
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
        }
    }
    #[test]
    fn move_refuses_locks_source_conflicts_duplicates_overlaps_and_unsafe_time_atomically() {
        let base = fixture();
        let moves = [request("picture", "video", "video-1", 0), request("sound", "audio", "audio-1", 0)];
        for mode in 0..5 {
            let mut value = serde_json::to_value(&base).unwrap();
            match mode {
                0 => value["compositions"]["root"]["content"]["tracks"][0]["locked"] = json!(true),
                1 => value["compositions"]["new-timeline"]["content"]["tracks"][2]["locked"] = json!(true),
                2 => { value["compositions"]["new-timeline"]["content"]["sources"] = value["compositions"]["root"]["content"]["sources"].clone(); value["compositions"]["new-timeline"]["content"]["sources"]["source"]["width"] = json!(8); },
                3 => { value["compositions"]["new-timeline"]["content"]["sources"] = value["compositions"]["root"]["content"]["sources"].clone(); value["compositions"]["new-timeline"]["content"]["tracks"][0]["clips"] = json!([value["compositions"]["root"]["content"]["tracks"][0]["clips"][0].clone()]); },
                _ => { value["compositions"]["new-timeline"]["content"]["sources"] = value["compositions"]["root"]["content"]["sources"].clone(); let mut overlap=value["compositions"]["root"]["content"]["tracks"][0]["clips"][0].clone();overlap["id"]=json!("overlap");value["compositions"]["new-timeline"]["content"]["tracks"][0]["clips"]=json!([overlap]); },
            }
            let project: NestedCompositionProject = serde_json::from_value(value).unwrap();
            let before = serde_json::to_value(&project).unwrap();
            assert!(move_clips(&project, &moves).is_err(), "refusal mode {mode}");
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
        }
        assert!(move_clips(&base, &[moves[0].clone(), moves[0].clone()]).is_err());
        let mut unsafe_time = moves[0].clone(); unsafe_time.start_us = 9_007_199_254_740_992;
        assert!(move_clips(&base, &[unsafe_time]).is_err());
        let mut same = moves[0].clone(); same.to_composition = "root".into(); same.to_track_id = "video".into();
        assert!(!move_clips(&base, &[same]).unwrap().changed);
    }
    #[test]
    fn transition_batch_moves_exact_windows_and_new_track_keeps_authored_track_flags() {
        let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        let mut value = serde_json::to_value(self::fixture()).unwrap();
        let case = fixture["cases"].as_array().unwrap().iter().find(|case| case["name"] == "explicit").unwrap();
        value["compositions"]["root"]["content"] = case["original"].clone();
        let project: NestedCompositionProject = serde_json::from_value(value).unwrap();
        let first = request("picture", "video", "new-video", 2_000_000);
        assert!(move_clips(&project, &[ClipMove { create_track: true, ..first.clone() }]).is_err());
        let second = request("next", "video", "new-video", 6_000_000);
        let result = move_clips(&project, &[ClipMove { create_track: true, ..first }, ClipMove { create_track: true, ..second }]).unwrap();
        let output = serde_json::to_value(result.project).unwrap();
        let track = output["compositions"]["new-timeline"]["content"]["tracks"].as_array().unwrap().last().unwrap();
        assert_eq!(track["id"], "new-video");
        assert_eq!(track["name"], "Video");
        assert_eq!(track["clips"].as_array().unwrap().len(), 2);
        assert_eq!(track["transitions"][0]["timeline"]["startTick"], "1397088000000");
        assert!(output["compositions"]["root"]["content"]["tracks"][0]["transitions"].as_array().unwrap().is_empty());
    }
}

#[cfg(test)]
mod crossfade_tests {
    use super::*;
    #[test]
    fn crossfade_requires_both_clips_and_preserves_the_authored_audio_fields() {
        let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        let mut value = fixture["createdTimeline"].clone();
        let mut next = value["compositions"]["root"]["content"]["tracks"][1]["clips"][0].clone();
        next["id"] = json!("next-sound");
        next["placement"] = json!({"timelineStartTick":4000000,"sourceInTick":6000000,"sourceOutTick":10000000,"speed":1});
        next["crossfadeInTicks"] = json!(1000000);
        value["compositions"]["root"]["content"]["tracks"][1]["clips"].as_array_mut().unwrap().push(next);
        let project: NestedCompositionProject = serde_json::from_value(value).unwrap();
        project.dependency_order().unwrap();
        let first = ClipMove { from_composition:"root".into(),from_track_id:"audio".into(),clip_id:"sound".into(),to_composition:"new-timeline".into(),to_track_id:"audio-1".into(),start_us:2000000,create_track:false };
        let second = ClipMove { clip_id:"next-sound".into(),start_us:6000000,..first.clone() };
        assert!(move_clips(&project, &[first.clone()]).is_err());
        assert!(move_clips(&project, &[second.clone()]).is_err());
        let result = move_clips(&project, &[first,second]).unwrap();
        let encoded = serde_json::to_value(result.project).unwrap();
        assert_eq!(encoded["compositions"]["new-timeline"]["content"]["tracks"][2]["clips"][1]["crossfadeInTicks"],1000000);
    }
}

#[cfg(test)]
mod visual_variant_tests {
    use super::*;
    #[test]
    fn visual_variants_preserve_authored_payload_sources_and_local_graphic_clock() {
        use super::super::composition::TransformSpec;
        let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        let transform = serde_json::to_value(TransformSpec::default()).unwrap();
        let opacity = json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"ease_in_out","keyframes":[{"tick":0,"value":0.2},{"tick":2000000,"value":0.9}]}});
        let color = json!({"r":17,"g":91,"b":203,"a":221});
        let transparent = json!({"red":0,"green":0,"blue":0,"alpha":0});
        let text_color = json!({"red":0.1,"green":0.4,"blue":0.8,"alpha":0.9});
        for kind in ["image", "text", "color", "graphic"] {
            let mut value = fixture["createdTimeline"].clone();
            let mut clip = json!({"id":"visual","timelineStartTick":1000000,"timelineEndTick":3000000,"transform":transform,"opacity":opacity,"enabled":true});
            match kind {
                "image" => {
                    clip.as_object_mut().unwrap().remove("timelineEndTick");
                    clip["durationTicks"] = json!(2000000);
                    clip["sourceId"] = json!("still");
                    clip["blendMode"] = json!("normal");
                    let mut source = value["compositions"]["root"]["content"]["sources"]["source"].clone();
                    source["id"] = json!("still"); source["kind"] = json!("image"); source["hasAudio"] = json!(false); source.as_object_mut().unwrap().remove("audioChannels");
                    value["compositions"]["root"]["content"]["sources"]["still"] = source;
                }
                "text" => {
                    clip["text"] = json!("Текст\nSecond line");
                    clip["style"] = json!({"fontFamily":"Noto Sans","fontSize":24,"color":text_color,"background":transparent,"stroke":transparent,"strokeWidth":0,"shadow":transparent,"shadowX":0,"shadowY":0});
                }
                "color" => {clip["width"]=json!(16);clip["height"]=json!(16);clip["color"]=color.clone();}
                _ => {
                    clip["width"]=json!(16);clip["height"]=json!(16);clip["animationOffsetUs"]=json!(250000);
                    let shape=serde_json::to_value(super::super::graphic_shape::GraphicShape::default()).unwrap();
                    clip["graphic"]=json!({"layers":[{"uid":7,"enabled":true,"content":{"kind":"shape","parameters":shape}}]});
                }
            }
            value["compositions"]["root"]["content"]["tracks"] = json!([{"kind":kind,"id":"visual-track","name":"Authored lane","hidden":false,"locked":false,"clips":[clip]}]);
            let project: NestedCompositionProject=serde_json::from_value(value).unwrap();
            project.dependency_order().unwrap();
            let before=serde_json::to_value(&project).unwrap();
            let request=ClipMove{from_composition:"root".into(),from_track_id:"visual-track".into(),clip_id:"visual".into(),to_composition:"new-timeline".into(),to_track_id:"moved-visual".into(),start_us:5000000,create_track:true};
            let result=move_clips(&project,&[request]).unwrap();
            let output=serde_json::to_value(&result.project).unwrap();
            let target=output["compositions"]["new-timeline"]["content"]["tracks"].as_array().unwrap().last().unwrap();
            let mut expected=before["compositions"]["root"]["content"]["tracks"][0]["clips"][0].clone();
            expected["timelineStartTick"]=json!(5000000);
            if kind!="image" {expected["timelineEndTick"]=json!(7000000);}
            assert_eq!(target["clips"][0],expected,"{kind}");
            assert_eq!(target["name"],"Authored lane");
            assert!(output["compositions"]["root"]["content"]["tracks"][0]["clips"].as_array().unwrap().is_empty());
            assert_eq!(result.project.durations_us().unwrap()["new-timeline"],7000000);
            assert_eq!(serde_json::to_value(&project).unwrap(),before);
            {
                use super::super::{flat_surface::render_generated_images,group_surface::GroupSurface};
                let original=&project.compositions["root"].content;
                let moved=&result.project.compositions["new-timeline"].content;
                let mut visible=false;
                for delta in [0,1,250000,999999,1750000,1999999,2000000] {
                    let mut image=|_:&super::super::composition::SourceId| Ok(GroupSurface{width:4,height:4,rgba:(0..16).flat_map(|i|[i*13,211-i*7,31+i*3,200]).collect()});
                    let old=render_generated_images(original,1000000+delta,&mut||Ok(()),Some(&mut image)).unwrap().expect("native original variant");
                    let new=render_generated_images(moved,5000000+delta,&mut||Ok(()),Some(&mut image)).unwrap().unwrap_or_else(||panic!("native moved {kind} delta{delta}"));
                    assert_eq!(old,new,"{kind} pixels delta{delta}");
                    if delta<2000000 {visible|=old.rgba.chunks_exact(4).any(|pixel|pixel[0]!=0||pixel[1]!=0||pixel[2]!=0);}
                }
                assert!(visible,"{kind} must produce visible pixels");
            }
            if kind=="image" {assert_eq!(output["compositions"]["new-timeline"]["content"]["sources"]["still"],before["compositions"]["root"]["content"]["sources"]["still"]);}
            if kind=="graphic" {
                let old:super::super::graphic_timeline::GraphicTimelineClip=serde_json::from_value(before["compositions"]["root"]["content"]["tracks"][0]["clips"][0].clone()).unwrap();
                let new:super::super::graphic_timeline::GraphicTimelineClip=serde_json::from_value(target["clips"][0].clone()).unwrap();
                for delta in [0,1,999999,1999999] {assert_eq!(old.layer_time(1000000+delta).unwrap(),new.layer_time(5000000+delta).unwrap());}
            }
        }
    }
}

#[cfg(test)]
mod dependency_move_tests {
    use super::*;
    #[test]
    fn external_control_layers_move_atomically_with_rebound_ordinals() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        for case in fixture["nativeRebinds"].as_array().unwrap() {
            let project:NestedCompositionProject=serde_json::from_value(case["before"].clone()).unwrap();
            let before=serde_json::to_value(&project).unwrap();
            let picture=ClipMove{from_composition:"root".into(),from_track_id:"dependent".into(),clip_id:"picture".into(),to_composition:"new-timeline".into(),to_track_id:"new-picture".into(),start_us:4000000,create_track:true};
            let control=ClipMove{from_track_id:"control".into(),clip_id:"control-clip".into(),to_track_id:"new-control".into(),..picture.clone()};
            assert!(move_clips(&project,&[picture.clone()]).is_err());
            assert!(move_clips(&project,&[control.clone()]).is_err());
            let mut incompatible=case["before"].clone();
            let mut lane=incompatible["compositions"]["root"]["content"]["tracks"][2].clone();lane["id"]=json!("new-control");lane["hidden"]=json!(false);lane["clips"]=json!([]);
            incompatible["compositions"]["new-timeline"]["content"]["tracks"].as_array_mut().unwrap().push(lane);
            let incompatible:NestedCompositionProject=serde_json::from_value(incompatible).unwrap();
            assert!(move_clips(&incompatible,&[picture.clone(),control.clone()]).is_err());
            let mut bad=control.clone();bad.start_us+=1;
            assert!(move_clips(&project,&[picture.clone(),bad]).is_err());
            let moved=move_clips(&project,&[picture,control]).unwrap();
            let output=serde_json::to_value(&moved.project).unwrap();
            let clips=&output["compositions"]["new-timeline"]["content"]["tracks"][4]["clips"];
            let field=if case["kind"]=="track_matte" {"matte"}else{"layer"};
            assert_eq!(clips[0]["effects"][0]["parameters"][field],4);
            assert_eq!(serde_json::to_value(&project).unwrap(),before);
        }
    }
}

#[cfg(test)]
mod empty_dependency_tests {
    use super::*;
    #[test]
    fn empty_control_is_cloned_once_with_flags_and_collision_free_identity() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        for case in fixture["nativeRebinds"].as_array().unwrap() {
            let mut value=case["before"].clone();
            value["compositions"]["root"]["content"]["tracks"][2]["clips"]=json!([]);
            value["compositions"]["root"]["content"]["tracks"][2]["locked"]=json!(true);
            value["compositions"]["new-timeline"]["content"]["tracks"][0]["id"]=json!("empty-control-0");
            let effect=value["compositions"]["root"]["content"]["tracks"][1]["clips"][0]["effects"][0].clone();
            value["compositions"]["root"]["content"]["tracks"][1]["clips"][0]["effects"].as_array_mut().unwrap().push(effect);
            let project:NestedCompositionProject=serde_json::from_value(value).unwrap();let before=serde_json::to_value(&project).unwrap();
            let request=ClipMove{from_composition:"root".into(),from_track_id:"dependent".into(),clip_id:"picture".into(),to_composition:"new-timeline".into(),to_track_id:"new-picture".into(),start_us:4000000,create_track:true};
            let mut collision=before.clone();
            let mut resident=collision["compositions"]["root"]["content"]["tracks"][1]["clips"][0].clone();resident["id"]=json!("resident");
            let field=if case["kind"]=="track_matte" {"matte"}else{"layer"};
            for effect in resident["effects"].as_array_mut().unwrap(){effect["parameters"][field]=json!(4);}
            collision["compositions"]["new-timeline"]["content"]["sources"]=collision["compositions"]["root"]["content"]["sources"].clone();
            collision["compositions"]["new-timeline"]["content"]["tracks"][0]=json!({"kind":"image","id":"resident-lane","name":"Resident","hidden":false,"locked":false,"clips":[resident]});
            let collision:NestedCompositionProject=serde_json::from_value(collision).unwrap();collision.dependency_order().unwrap();
            assert!(move_clips(&collision,&[request.clone()]).is_err());
            let moved=move_clips(&project,&[request]).unwrap();
            let output=serde_json::to_value(&moved.project).unwrap();
            let tracks=output["compositions"]["new-timeline"]["content"]["tracks"].as_array().unwrap();assert_eq!(tracks.len(),6);
            let control=tracks.last().unwrap();assert_eq!(control["id"],"empty-control-1");assert_eq!(control["locked"],true);assert_eq!(control["hidden"],true);assert!(control["clips"].as_array().unwrap().is_empty());
            let field=if case["kind"]=="track_matte" {"matte"}else{"layer"};
            for effect in tracks[4]["clips"][0]["effects"].as_array().unwrap(){assert_eq!(effect["parameters"][field],4);}
            assert_eq!(serde_json::to_value(&project).unwrap(),before);
        }
    }
}

#[cfg(test)]
mod local_dependency_tests {
    use super::*;
    #[test]
    fn same_timeline_batch_rebinds_controls_and_keeps_stationary_or_empty_references() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        for case in fixture["nativeRebinds"].as_array().unwrap() {
            let project:NestedCompositionProject=serde_json::from_value(case["before"].clone()).unwrap();let before=serde_json::to_value(&project).unwrap();
            let picture=ClipMove{from_composition:"root".into(),from_track_id:"dependent".into(),clip_id:"picture".into(),to_composition:"root".into(),to_track_id:"empty".into(),start_us:4000000,create_track:false};
            let control=ClipMove{from_track_id:"control".into(),clip_id:"control-clip".into(),to_track_id:"new-control".into(),create_track:true,..picture.clone()};
            assert!(move_clips(&project,&[picture.clone()]).is_err());
            assert!(move_clips(&project,&[control.clone()]).is_err());
            let field=if case["kind"]=="track_matte" {"matte"}else{"layer"};
            let result=move_clips(&project,&[picture.clone(),control]).unwrap();
            let output=serde_json::to_value(&result.project).unwrap();
            assert_eq!(output["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["parameters"][field],4);
            assert_eq!(output["compositions"]["root"]["content"]["tracks"][3]["hidden"],true);
            let stationary=ClipMove{start_us:0,..picture.clone()};
            let result=move_clips(&project,&[stationary]).unwrap();
            let output=serde_json::to_value(&result.project).unwrap();assert_eq!(output["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["effects"][0]["parameters"][field],3);
            let mut empty=before.clone();empty["compositions"]["root"]["content"]["tracks"][2]["clips"]=json!([]);
            let empty:NestedCompositionProject=serde_json::from_value(empty).unwrap();
            let result=move_clips(&empty,&[picture]).unwrap();assert_eq!(result.project.compositions["root"].content.tracks.len(),3);
            assert_eq!(serde_json::to_value(&project).unwrap(),before);
        }
    }
}

#[cfg(test)]
mod animated_control_tests {
    use super::*;
    #[test]
    fn color_control_samples_local_microseconds_and_matches_explicit_static_reference() {
        use super::super::{composition::Composition,group_surface::GroupSurface,flat_surface::render_generated_images,animated_color::{ColorSampleRequest,PreparedColorTrack}};
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        let animation=json!({"keys":[{"t_us":0,"value":{"r":255,"g":255,"b":255,"a":255},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}},{"t_us":2000000,"value":{"r":0,"g":0,"b":0,"a":64},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}});
        for case in fixture["nativeRebinds"].as_array().unwrap() {
            let mut value=case["before"]["compositions"]["root"]["content"].clone();
            value["tracks"][2]["clips"][0]["colorAnimation"]=animation.clone();
            let animated:Composition=serde_json::from_value(value.clone()).unwrap();
            let track=PreparedColorTrack::new(ColorSampleRequest{track:serde_json::from_value(animation.clone()).unwrap(),default:serde_json::from_value(value["tracks"][2]["clips"][0]["color"].clone()).unwrap(),times_us:vec![]}).unwrap();
            let mut pixels=Vec::new();
            for local in [0u64,500000,1000000,1999999] {
                let color=track.sample(local as f64).unwrap();
                let mut reference=value.clone();reference["tracks"][2]["clips"][0].as_object_mut().unwrap().remove("colorAnimation");reference["tracks"][2]["clips"][0]["color"]=serde_json::to_value(color).unwrap();
                let reference:Composition=serde_json::from_value(reference).unwrap();
                let mut decode=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:4,height:4,rgba:(0..16u8).flat_map(|i|[i.wrapping_mul(53),255-i*13,i*11,255]).collect()});
                let actual=render_generated_images(&animated,local,&mut||Ok(()),Some(&mut decode)).unwrap().unwrap();
                let expected=render_generated_images(&reference,local,&mut||Ok(()),Some(&mut decode)).unwrap().unwrap();
                assert_eq!(actual,expected,"{} local{local}",case["kind"]);pixels.push(actual);
            }
            assert_ne!(pixels[0],pixels[3],"{} animated control must alter pixels",case["kind"]);
        }
    }
}

#[cfg(test)]
mod linked_move_tests {
    use super::*;
    #[test]
    fn linked_pairs_validate_and_move_together_with_portable_metadata() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        let mut value=fixture["nativeFirstFree"][0]["after"].clone();
        // Move the newly linked AV pair into the source root's free gap.
        let project:NestedCompositionProject=serde_json::from_value(value.clone()).unwrap();project.dependency_order().unwrap();
        let picture=ClipMove{from_composition:"new-timeline".into(),from_track_id:"video-2".into(),clip_id:"auto-first".into(),to_composition:"root".into(),to_track_id:"linked-video".into(),start_us:4000000,create_track:true};
        let sound=ClipMove{from_track_id:"audio-2".into(),clip_id:"auto-first-audio".into(),to_track_id:"linked-audio".into(),..picture.clone()};
        assert!(move_clips(&project,&[picture.clone()]).is_err());
        let mut different=sound.clone();different.start_us+=1;assert!(move_clips(&project,&[picture.clone(),different]).is_err());
        let result=move_clips(&project,&[picture.clone(),sound.clone()]).unwrap();
        let output=serde_json::to_value(result.project).unwrap();
        assert!(output["compositions"]["new-timeline"]["content"]["metadata"].get("clipLinks").is_none());
        assert_eq!(output["compositions"]["root"]["content"]["metadata"]["clipLinks"]["auto-first-link"],json!(["auto-first","auto-first-audio"]));
        let mut collision=serde_json::to_value(&project).unwrap();
        collision["compositions"]["root"]["content"]["metadata"]=json!({"clipLinks":{"auto-first-link":["picture","sound"]}});
        let collision:NestedCompositionProject=serde_json::from_value(collision).unwrap();
        let result=move_clips(&collision,&[picture,sound]).unwrap();let output=serde_json::to_value(result.project).unwrap();
        assert_eq!(output["compositions"]["root"]["content"]["metadata"]["clipLinks"]["auto-first-link"],json!(["picture","sound"]));
        assert_eq!(output["compositions"]["root"]["content"]["metadata"]["clipLinks"]["clip-link-0"],json!(["auto-first","auto-first-audio"]));
        for links in [json!({"g":["auto-first"]}),json!({"g":["auto-first","missing"]}),json!({"g":["auto-first","auto-first"]}),json!({"g":["auto-first","auto-first-audio"],"other":["auto-first","auto-first-audio"]})] {
            value["compositions"]["new-timeline"]["content"]["metadata"]["clipLinks"]=links;
            let invalid:NestedCompositionProject=serde_json::from_value(value.clone()).unwrap();assert!(invalid.dependency_order().is_err());
        }
    }
    #[test]
    fn portable_cutout_moves_its_mask_and_actual_input_with_the_image(){
        let all:Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();let mut value=all["createdTimeline"].clone();
        let transform=value["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["transform"].clone();
        let root=&mut value["compositions"]["root"]["content"];
        for id in ["source","next-source"]{root["sources"][id]["kind"]=json!("image");root["sources"][id]["durationTicks"]=json!(0);root["sources"][id]["hasAudio"]=json!(false);root["sources"][id].as_object_mut().unwrap().remove("audioChannels");}
        root["sources"]["mask"]=json!({"id":"mask","kind":"image","durationTicks":0,"width":512,"height":512,"hasAudio":false});
        root["tracks"]=json!([{"id":"images","name":"Images","kind":"image","clips":[{"id":"picture","sourceId":"source","timelineStartTick":0,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]}]);
        root["metadata"]=json!({"mediaVersions":{"picture":{"originalSourceId":"source","copySourceId":"next-source","cutout":{"sourceId":"source","maskSourceId":"mask","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"modelHash":"d".repeat(64),"runtimeHash":"e".repeat(64),"correction":{"custom":true,"strokes":[],"feather":0.01}}}}});
        use sha2::Digest;
        let points=json!([[0.5,0.5]]);let prompt_hash=format!("{:x}",sha2::Sha256::digest(serde_json::to_vec(&points).unwrap()));
        root["sources"]["learned"]=json!({"id":"learned","kind":"image","durationTicks":0,"width":256,"height":256,"hasAudio":false});
        root["metadata"]["mediaVersions"]["picture"]["cutout"]["correction"]["strokes"]=json!([{"tool":"smartBrush","size":0.1,"points":points,"at":null,"region":{"sourceId":"source","maskSourceId":"learned","sourceHash":"a".repeat(64),"maskHash":"f".repeat(64),"maskFileHash":"1".repeat(64),"encoderHash":"2".repeat(64),"decoderHash":"3".repeat(64),"runtimeHash":"4".repeat(64),"promptHash":prompt_hash,"points":points}}]);
        value["compositions"]["new-timeline"]["content"]["tracks"]=json!([{"id":"images-1","name":"Images","kind":"image","clips":[]}]);
        let project:NestedCompositionProject=serde_json::from_value(value).unwrap();let before=serde_json::to_value(&project).unwrap();
        let result=move_clips(&project,&[ClipMove{from_composition:"root".into(),from_track_id:"images".into(),clip_id:"picture".into(),to_composition:"new-timeline".into(),to_track_id:"images-1".into(),start_us:0,create_track:false}]).unwrap();let out=serde_json::to_value(result.project).unwrap();
        assert!(out["compositions"]["new-timeline"]["content"]["sources"].get("learned").is_some());        assert!(out["compositions"]["new-timeline"]["content"]["sources"].get("mask").is_some());assert_eq!(out["compositions"]["new-timeline"]["content"]["metadata"]["mediaVersions"]["picture"],before["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["picture"]);
    }

}
