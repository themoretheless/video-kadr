//! Lossless bridge from the source FreezeFrame splice to a typed native lane.
use super::{
    composition::{Composition, CompositionSource, SourceKind},
    concat_freeze_splice, concat_project_export, concat_project_import,
    nested_composition::NestedCompositionProject,
};
use anyhow::{ensure, Context, Result};
use serde_json::json;
use std::collections::BTreeMap;

/// Converts only the selected native lane, preserving all unrelated native data.
/// Existing exporter/importer losslessness and render-admission guards still apply.
/// Native lane grouping for later cross-kind edits remains a caller responsibility.
pub fn splice(
    composition: &Composition,
    target: &str,
    time: f64,
    still: &CompositionSource,
    tail_id: &str,
    still_id: &str,
    image_track_id: &str,
) -> Result<Option<Composition>> {
    composition.validate_native_surface()?;
    ensure!(
        still.kind == SourceKind::Image
            && still.duration_ticks == 0
            && !still.has_audio
            && still.width > 0
            && still.height > 0,
        "invalid freeze image source"
    );
    let mut output = serde_json::to_value(composition)?;
    let tracks = output["tracks"].as_array().context("missing tracks")?;
    let Some(index) = tracks.iter().position(|track| {
        track["clips"]
            .as_array()
            .is_some_and(|clips| clips.iter().any(|clip| clip["id"] == target))
    }) else {
        return Ok(None);
    };
    let original = tracks[index].clone();
    let video_id = original["id"]
        .as_str()
        .context("missing video lane ID")?
        .to_owned();
    let group = output["metadata"]["concatLaneGroups"]
        .get(&video_id)
        .and_then(|v| v.as_array());
    let existing_image = if let Some(group) = group {
        ensure!(
            group.len() == 2 && group.iter().any(|id| id == &video_id),
            "invalid freeze lane group"
        );
        let image_id = group
            .iter()
            .find(|id| *id != &video_id)
            .and_then(|id| id.as_str())
            .context("missing grouped image lane")?;
        let image_index = tracks
            .iter()
            .position(|track| track["id"] == image_id)
            .context("grouped image lane missing")?;
        ensure!(
            image_index + 1 == index
                && tracks[image_index]["kind"] == "image"
                && tracks[image_index]["hidden"] == original["hidden"]
                && tracks[image_index]["locked"] != true,
            "freeze lane group was reordered or changed visibility/lock state"
        );
        Some((image_index, tracks[image_index].clone()))
    } else {
        None
    };
    ensure!(
        original["kind"] == "video",
        "video publication requires a native video lane"
    );
    ensure!(original["locked"] != true, "freeze lane is locked");
    ensure!(
        !composition.sources.contains_key(&still.id),
        "freeze source ID already exists"
    );
    ensure!(
        !image_track_id.is_empty() && !tracks.iter().any(|track| track["id"] == image_track_id),
        "freeze image track ID collision"
    );
    ensure!(
        !tracks
            .iter()
            .flat_map(|track| track["clips"].as_array().into_iter().flatten())
            .any(|clip| clip["id"] == tail_id || clip["id"] == still_id),
        "freeze clip ID collision"
    );
    if super::native_video_freeze::eligible(composition,target) {
        return super::native_video_freeze::splice(composition,target,time,still,tail_id,still_id,image_track_id);
    }
    let mut selected = Composition::new(composition.canvas.clone());
    // Background belongs to the enclosing composition; only clip-local media,
    // poses and effects pass through the source interchange bridge.
    let mut selected_wire = serde_json::to_value(&selected)?;
    selected_wire["canvas"]["background"] = json!({"red":0.,"green":0.,"blue":0.,"alpha":1.});
    selected_wire["canvas"]["backgroundMode"] = json!("color");
    selected = serde_json::from_value(selected_wire)?;
    selected.sources = composition.sources.clone();
    selected.tracks = vec![serde_json::from_value(original.clone())?];
    if let Some((_, image)) = &existing_image {
        selected
            .tracks
            .insert(0, serde_json::from_value(image.clone())?);
    }
    let project: NestedCompositionProject = serde_json::from_value(
        json!({"schemaVersion":1,"root":"freeze","compositions":{"freeze":{"content":selected,"instances":[]}}}),
    )?;
    // Synthetic binding identifiers are never resolved as filesystem paths.
    let paths: BTreeMap<_, _> = composition
        .sources
        .keys()
        .map(|id| (id.as_str().to_owned(), format!("binding/{}", id.as_str())))
        .collect();
    let mut document = concat_project_export::export_with_paths(&project, &paths)?;
    let timeline = &mut document["timelines"][0];
    // Rejoin the explicitly recorded typed siblings before executing source ripple.
    for clip in timeline["clips"].as_array_mut().context("missing clips")? {
        clip["trackId"] = json!(video_id);
        // Source image interchange omits default playback fields. The source
        // command consumes normalized Editor clips, so restore those defaults.
        let object = clip.as_object_mut().context("invalid source clip")?;
        object.entry("sourceStart").or_insert(json!(0.));
        object.entry("speed").or_insert(json!(1.));
    }
    let source_track = timeline["tracks"]
        .as_array()
        .context("missing source tracks")?
        .iter()
        .find(|track| track["id"] == video_id)
        .context("missing source video lane")?
        .clone();
    timeline["tracks"] = json!([source_track]);
    let Some(clips) = concat_freeze_splice::freeze(
        timeline["clips"]
            .as_array()
            .context("missing source clips")?,
        target,
        time,
        Some(1.),
        Some(still.id.as_str()),
        tail_id,
        still_id,
    )?
    else {
        return Ok(None);
    };
    timeline["clips"] = json!(clips);
    document["media"].as_array_mut().context("missing source media")?.push(json!({"id":still.id,"path":format!("binding/{}",still.id.as_str()),"name":"Стоп-кадр","kind":"image","duration":0.,"width":still.width,"height":still.height,"hasAudio":false}));
    let mut bindings = composition.sources.clone();
    bindings.insert(still.id.clone(), still.clone());
    let report = concat_project_import::import(serde_json::from_value(
        json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}),
    )?)?;
    ensure!(
        report.status == "converted",
        "freeze native conversion requires attention: {}",
        report.diagnostics.join("; ")
    );
    let converted = report.project.context("missing converted freeze")?;
    let content = serde_json::to_value(&converted.compositions[&converted.root].content)?;
    let converted_tracks = content["tracks"]
        .as_array()
        .context("missing converted tracks")?;
    let video = converted_tracks
        .iter()
        .find(|track| track["kind"] == "video")
        .context("missing converted video lane")?;
    let mut replacement = original.clone();
    replacement["clips"] = video["clips"].clone();
    let mut transitions = video["transitions"].as_array().cloned().unwrap_or_default();
    for transition in &mut transitions {
        let old = original["transitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|old| old["toClipId"] == transition["toClipId"])
            .context("freeze introduced an unknown incoming transition")?;
        transition["id"] = old["id"].clone();
    }
    replacement["transitions"] = json!(transitions);
    let mut image = converted_tracks
        .iter()
        .find(|track| track["kind"] == "image")
        .context("missing converted still lane")?
        .clone();
    let retained_image_id = existing_image
        .as_ref()
        .map(|(_, track)| track["id"].as_str().unwrap())
        .unwrap_or(image_track_id);
    image["id"] = json!(retained_image_id);
    image["name"] = existing_image
        .as_ref()
        .map(|(_, track)| track["name"].clone())
        .unwrap_or(json!("Стоп-кадр"));
    let tracks = output["tracks"].as_array_mut().unwrap();
    tracks[index] = replacement;
    if let Some((image_index, _)) = &existing_image {
        tracks[*image_index] = image;
    } else {
        tracks.insert(index, image);
    }
    output["sources"][still.id.as_str()] = serde_json::to_value(still)?;
    let mut result: Composition = serde_json::from_value(output)?;
    result
        .metadata
        .get_or_insert_with(Default::default)
        .concat_lane_groups
        .insert(
            super::composition::TrackId::parse(video_id.clone())?,
            vec![
                super::composition::TrackId::parse(video_id)?,
                super::composition::TrackId::parse(retained_image_id.to_owned())?,
            ],
        );
    let target_id = super::composition::CompositionClipId::parse(target.to_owned())?;
    let name = composition
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.clip_names.get(&target_id))
        .cloned()
        .unwrap_or_else(|| target.to_owned());
    let metadata = result.metadata.as_mut().unwrap();
    for id in [tail_id, still_id] {
        metadata.clip_names.insert(
            super::composition::CompositionClipId::parse(id.to_owned())?,
            name.clone(),
        );
    }
    metadata
        .source_names
        .insert(still.id.clone(), "Стоп-кадр".into());
    result.validate_native_surface()?;
    Ok(Some(result))
}
