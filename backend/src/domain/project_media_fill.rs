//! Pure native FillSlot planning. Physical source access/probing belongs to the handler.
use super::{
    composition::{CompositionSource, SourceId},
    nested_composition::NestedCompositionProject,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn normalized(value: Value, kind: &str) -> Result<Value> {
    Ok(match kind {
        "video" => serde_json::to_value(serde_json::from_value::<super::composition::VideoClip>(
            value,
        )?)?,
        "audio" => serde_json::to_value(serde_json::from_value::<super::composition::AudioClip>(
            value,
        )?)?,
        "image" => serde_json::to_value(serde_json::from_value::<super::composition::ImageClip>(
            value,
        )?)?,
        _ => anyhow::bail!("unsupported media kind"),
    })
}
fn default_property(key: &str) -> Value {
    match key {
        "scale" | "stretchX" | "stretchY" | "opacity" | "volume" | "speed" => json!(1.),
        "offsetX" | "offsetY" | "rotation" | "sourceStart" | "fadeIn" | "fadeOut" => json!(0.),
        "muted" | "flipH" | "flipV" => json!(false),
        "preservePitch" => json!(true),
        "blend" => json!("normal"),
        "keys" | "filters" | "videoEffects" => json!([]),
        "crop" => json!({"left":0.,"top":0.,"right":0.,"bottom":0.}),
        _ => Value::Null,
    }
}
fn visual_dependency(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            matches!(key.as_str(), "matte" | "layer")
                && value.as_u64().is_some_and(|index| index > 0)
                || visual_dependency(value)
        }),
        Value::Array(values) => values.iter().any(visual_dependency),
        _ => false,
    }
}
pub fn fill(
    project: &NestedCompositionProject,
    id: &SourceId,
    replacement: &Value,
    physical: &CompositionSource,
) -> Result<NestedCompositionProject> {
    project.dependency_order()?;
    physical.validate()?;
    let old = project.project_media.get(id).context("SlotGone")?;
    ensure!(old.placeholder, "NotASlot");
    let kind = serde_json::to_value(physical.kind)?
        .as_str()
        .context("invalid source kind")?
        .to_owned();
    ensure!(
        replacement["kind"] == kind,
        "replacement kind differs from registered descriptor"
    );
    let mut logical = physical.clone();
    logical.id = id.clone();
    let bindings = BTreeMap::from([(id.as_str().to_owned(), logical.clone())]);
    let mut next = project.clone();
    let mut wire = serde_json::to_value(&next)?;
    for (node_id, node) in &project.compositions {
        let width = node.content.canvas.width as f64;
        let height = node.content.canvas.height as f64;
        let content = &mut wire["compositions"][node_id]["content"];
        for track in content["tracks"].as_array_mut().context("missing tracks")? {
            let old_kind = track["kind"].as_str().unwrap_or("").to_owned();
            if !matches!(old_kind.as_str(), "video" | "audio" | "image") {
                continue;
            }
            let affected = track["clips"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|clip| clip["sourceId"] == id.as_str())
                .count();
            if affected == 0 {
                continue;
            }
            if kind != old_kind {
                ensure!(
                    affected == track["clips"].as_array().unwrap().len(),
                    "Mixed-source lane needs native kind-splitting before FillSlot"
                );
                ensure!(
                    track["transitions"].as_array().map_or(true, Vec::is_empty),
                    "Cross-kind FillSlot needs retained transition controls"
                );
                ensure!(
                    !(old_kind == "audio" && track["solo"] == true),
                    "Cross-kind FillSlot needs retained lane solo state"
                );
                ensure!(
                    !(kind == "audio" && track["hidden"] == true)
                        && !(kind == "image" && track["muted"] == true),
                    "Cross-kind FillSlot cannot retain this lane visibility/mute control"
                );
                if kind == "audio" || old_kind == "audio" {
                    let surface = serde_json::to_value(&node.content)?;
                    ensure!(
                        !visual_dependency(&surface),
                        "Cross-kind FillSlot needs visual dependency remapping"
                    );
                }
            }
            for clip in track["clips"].as_array_mut().unwrap() {
                if clip["sourceId"] != id.as_str() {
                    continue;
                }
                ensure!(
                    node.content
                        .metadata
                        .as_ref()
                        .map_or(true, |metadata| !metadata
                            .media_versions
                            .keys()
                            .any(|key| key.as_str() == clip["id"].as_str().unwrap_or(""))),
                    "FillSlot needs derived-media asset rebinding"
                );
                let mut source =
                    super::concat_project_export::media_clip(clip, &old_kind, width, height)?;
                let original_bindings =
                    BTreeMap::from([(id.as_str().to_owned(), old.source.clone())]);
                let reconstructed = normalized(
                    super::concat_project_import::convert_clip(
                        &source,
                        &original_bindings,
                        width,
                        height,
                    )?,
                    &old_kind,
                )?;
                ensure!(
                    reconstructed == *clip,
                    "Native clip controls cannot yet be retained by FillSlot"
                );
                source["kind"] = json!(kind);
                source["sourceStart"] = json!(0.);
                source.as_object_mut().unwrap().remove("audioStream");
                let changed = normalized(
                    super::concat_project_import::convert_clip(&source, &bindings, width, height)?,
                    &kind,
                )?;
                let restored =
                    super::concat_project_export::media_clip(&changed, &kind, width, height)?;
                for (key, value) in source.as_object().unwrap() {
                    let actual = restored
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| default_property(key));
                    ensure!(
                        &actual == value,
                        "Cross-kind FillSlot cannot retain authored {key}"
                    );
                }
                *clip = changed;
            }
            if kind != old_kind {
                track["kind"] = json!(kind);
                let allowed = match kind.as_str() {
                    "video" => vec![
                        "kind",
                        "id",
                        "name",
                        "hidden",
                        "muted",
                        "locked",
                        "clips",
                        "transitions",
                    ],
                    "audio" => vec!["kind", "id", "name", "muted", "solo", "locked", "clips"],
                    _ => vec!["kind", "id", "name", "hidden", "locked", "clips"],
                };
                track
                    .as_object_mut()
                    .unwrap()
                    .retain(|key, _| allowed.contains(&key.as_str()));
            }
        }
        if content["sources"].get(id.as_str()).is_some() {
            content["sources"][id.as_str()] = serde_json::to_value(&logical)?;
        }
    }
    next = serde_json::from_value(wire)?;
    let name = replacement["name"]
        .as_str()
        .context("missing replacement name")?;
    for node in next.compositions.values_mut() {
        if node.content.sources.contains_key(id) {
            let metadata = node.content.metadata.get_or_insert_with(Default::default);
            metadata.source_names.insert(id.clone(), name.into());
            for track in &node.content.tracks {
                let value = serde_json::to_value(track)?;
                for clip in value["clips"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|clip| clip["sourceId"] == id.as_str())
                {
                    metadata.clip_names.insert(
                        super::composition::CompositionClipId::parse(clip["id"].as_str().unwrap())?,
                        name.into(),
                    );
                }
            }
        }
    }
    let stub = json!({"version":1,"media":[{"id":id,"placeholder":true,"origin":old.concat_metadata.get("origin"),"colorRange":old.concat_metadata.get("colorRange")}],"timelines":[]});
    let filled = super::concat_media_slots::fill(&stub, id.as_str(), replacement)?;
    let entry = next.project_media.get_mut(id).unwrap();
    entry.source = logical;
    entry.library_source_id = Some(physical.id.clone());
    entry.name = name.into();
    entry.placeholder = false;
    entry.concat_metadata = super::project_media::capture_concat_metadata(&filled["media"][0]);
    next.dependency_order()?;
    Ok(next)
}
