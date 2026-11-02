use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::{Composition, CompositionTrack},
    concat_project_import::{import, ImportRequest},
    concat_treatment::Treatment,
    flat_surface,
};
fn source() -> Value {
    json!({"version":1,"media":[],"activeTimelineId":"TL","timelines":[{"id":"TL","video":{"width":160,"height":120,"rateNum":10,"rateDen":1},"tracks":[{"id":"bottom"},{"id":"adjustment"},{"id":"top"}],"clips":[{"id":"ground","trackId":"bottom","kind":"shape","start":0,"duration":4,"shape":{"kind":"square","size":0.9,"fill":"#804020","strokeWidth":0}},{"id":"look","trackId":"adjustment","kind":"layer","start":1,"duration":2,"opacity":0.5,"fadeIn":0.5,"fadeOut":0.5,"videoEffects":[{"id":"concat.exposure","params":{"stops":1}}]},{"id":"top","trackId":"top","kind":"shape","start":0,"duration":4,"shape":{"kind":"circle","size":0.2,"fill":"#204080","strokeWidth":0}}]}]})
}
fn convert(source: Value) -> video_kadr_backend::domain::concat_project_import::ImportReport {
    import(
        serde_json::from_value::<ImportRequest>(
            json!({"schemaVersion":1,"document":source,"mediaBindings":{}}),
        )
        .unwrap(),
    )
    .unwrap()
}
fn frame(composition: &Composition, time: u64) -> Vec<u8> {
    flat_surface::render_generated(composition, time, &mut || Ok(()))
        .unwrap()
        .unwrap()
        .rgba
}
#[test]
fn source_strength_uses_minimum_ramps_and_half_open_original_clock() {
    let treatment = Treatment {
        strength: 0.8,
        fade_in: 2.,
        fade_out: 2.,
        duration_ticks: 3000000,
    };
    assert_eq!(treatment.strength_at(0).unwrap(), 0.);
    assert_eq!(treatment.strength_at(1000000).unwrap(), 0.4);
    assert!((treatment.strength_at(1500000).unwrap() - 0.6).abs() < 1e-12);
    assert_eq!(treatment.strength_at(2000000).unwrap(), 0.4);
    assert_eq!(treatment.strength_at(3000000).unwrap(), 0.);
    for invalid in [
        Treatment {
            strength: 1.1,
            ..treatment.clone()
        },
        Treatment {
            fade_in: -1.,
            ..treatment.clone()
        },
        Treatment {
            duration_ticks: 0,
            ..treatment.clone()
        },
    ] {
        assert!(invalid.validate().is_err());
    }
}
#[test]
fn imported_treatment_processes_only_lower_tracks_with_fades_and_exact_roundtrip() {
    let original = source();
    let report = convert(original.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, original);
    let project = report.project.unwrap();
    let composition = &project.compositions[&project.root].content;
    assert!(composition.sources.is_empty());
    let CompositionTrack::Graphic { clips, .. } = &composition.tracks[1] else {
        panic!("graphic treatment lane required")
    };
    assert_eq!(clips[0].concat_treatment.as_ref().unwrap().strength, 0.5);
    assert!(clips[0].graphic.layers.is_empty());
    assert!(clips[0]
        .render_timeline(
            (160, 120),
            1500000,
            &mut |_, _, _, _| Ok(()),
            &mut || Ok(())
        )
        .is_err());
    let before = frame(composition, 500000);
    let ramp = frame(composition, 1250000);
    let full = frame(composition, 2000000);
    let after = frame(composition, 3000000);
    assert_eq!(before, after);
    let index = (60 * 160 + 40) * 4;
    assert!(before[index] < ramp[index] && ramp[index] < full[index]);
    let mut treated = video_kadr_backend::domain::group_surface::GroupSurface {
        width: 160,
        height: 120,
        rgba: before.clone(),
    };
    flat_surface::apply_effect_stack(
        &mut treated,
        &clips[0].effects,
        2000000,
        &[],
        &mut || Ok(()),
    )
    .unwrap();
    for channel in 0..3 {
        let under = (before[index + channel] as f64 / 255.).powf(2.4);
        let over = (treated.rgba[index + channel] as f64 / 255.).powf(2.4);
        let expected = (((under + over) * 0.5).powf(1. / 2.4) * 255.).round() as u8;
        assert!(
            full[index + channel].abs_diff(expected) <= 1,
            "linear half-strength blend: {:?}",
            &full[index..index + 4]
        );
    }
    let top = (60 * 160 + 80) * 4;
    for pixels in [&before, &ramp, &full, &after] {
        assert_eq!(&pixels[top..top + 4], [32, 64, 128, 255]);
    }
    let restored: Composition =
        serde_json::from_value(serde_json::to_value(composition).unwrap()).unwrap();
    assert_eq!(frame(&restored, 2000000), full);
}
#[test]
fn keyed_treatment_parameters_use_clip_progress_instead_of_absolute_shader_time() {
    let mut original = source();
    let layer = &mut original["timelines"][0]["clips"][1];
    layer["fadeIn"] = json!(0);
    layer["fadeOut"] = json!(0);
    layer["videoEffects"] = json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0,"ease":"linear"},{"at":1,"value":2}]}}]);
    let report = convert(original);
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let composition = &project.compositions[&project.root].content;
    let start = frame(composition, 1000000);
    let middle = frame(composition, 2000000);
    let later = frame(composition, 2800000);
    let index = (60 * 160 + 40) * 4;
    assert_eq!(start[index], 128);
    assert!(start[index] < middle[index] && middle[index] < later[index]);
}
#[test]
fn same_source_track_pixels_remain_above_treatment_and_bad_styles_refuse_atomically() {
    let mut original = source();
    original["timelines"][0]["clips"][2]["trackId"] = json!("adjustment");
    let report = convert(original.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let pixels = frame(&project.compositions[&project.root].content, 2000000);
    let top = (60 * 160 + 80) * 4;
    assert_eq!(&pixels[top..top + 4], [32, 64, 128, 255]);
    original["timelines"][0]["clips"][1]["opacity"] = json!(-0.1);
    let report = convert(original);
    assert_ne!(report.status, "converted");
    assert!(report.project.is_none());
}

#[test]
fn source_ignored_picture_controls_remain_in_source_document_without_affecting_treatment() {
    let original = source();
    let baseline = convert(original.clone());
    let baseline = baseline.project.unwrap();
    let expected = frame(&baseline.compositions[&baseline.root].content, 2000000);
    let mut changed = original;
    let layer = &mut changed["timelines"][0]["clips"][1];
    layer["scale"] = json!(3);
    layer["rotation"] = json!(45);
    layer["offsetX"] = json!(1);
    layer["blend"] = json!("multiply");
    layer["keys"] = json!([{"property":"opacity","at":0,"value":0}]);
    let report = convert(changed.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, changed);
    let project = report.project.unwrap();
    assert_eq!(
        frame(&project.compositions[&project.root].content, 2000000),
        expected
    );
}
#[test]
fn overlapping_same_track_treatments_keep_source_clip_order_not_start_order() {
    let mut original = source();
    let second = json!({"id":"second","trackId":"adjustment","kind":"layer","start":0,"duration":4,"opacity":0.7,"videoEffects":[{"id":"concat.contrast","params":{"amount":40}}]});
    original["timelines"][0]["clips"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let report = convert(original);
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let mut composition = project.compositions[&project.root].content.clone();
    let normal = frame(&composition, 2000000);
    if let CompositionTrack::Graphic { clips, .. } = &mut composition.tracks[1] {
        assert_eq!(clips[0].id.as_str(), "look");
        assert_eq!(clips[1].id.as_str(), "second");
        clips.reverse();
    } else {
        panic!("treatment lane required")
    };
    assert_ne!(normal, frame(&composition, 2000000));
}

#[test]
fn moved_treatment_native_keys_follow_clip_and_preserve_relative_frame() {
    use video_kadr_backend::domain::{nested_composition::NestedCompositionProject,nested_clip_edit::{move_clips,ClipMove}};
    let imported=convert(source()).project.unwrap();
    let mut value=serde_json::to_value(&imported).unwrap();
    let root=imported.root.clone();
    let track_id=value["compositions"][&root]["content"]["tracks"][1]["id"].as_str().unwrap().to_owned();
    value["compositions"][&root]["content"]["tracks"][1]["clips"][0]["effects"][0]["animation"]=json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":1000000,"value":0.0},{"tick":3000000,"value":2.0}]}});
    let project:NestedCompositionProject=serde_json::from_value(value.clone()).unwrap();project.dependency_order().unwrap();
    let before=frame(&project.compositions[&root].content,1500000);
    assert_ne!(before,frame(&project.compositions[&root].content,2000000));
    for start in [0,2000000] {
        let request=ClipMove{from_composition:root.clone(),from_track_id:track_id.clone(),clip_id:"look".into(),to_composition:root.clone(),to_track_id:track_id.clone(),start_us:start,create_track:false};
        let moved=move_clips(&project,&[request]).unwrap();
        let wire=serde_json::to_value(&moved.project).unwrap();
        let clip=&wire["compositions"][&root]["content"]["tracks"][1]["clips"][0];
        assert_eq!(clip["effects"][0]["animation"]["track"]["keyframes"][0]["tick"],start);
        assert_eq!(clip["concatTreatment"],value["compositions"][&root]["content"]["tracks"][1]["clips"][0]["concatTreatment"]);
        assert_eq!(frame(&moved.project.compositions[&root].content,start+500000),before);
    }
    assert_eq!(serde_json::to_value(&project).unwrap(),value);
}

#[test]
fn off_grid_source_treatment_keys_follow_quantized_window_and_half_open_end() {
    let mut document=source();
    document["timelines"][0]["video"]["rateNum"]=json!(25);
    let layer=&mut document["timelines"][0]["clips"][1];
    layer["start"]=json!(1.53);layer["duration"]=json!(0.9);
    layer["opacity"]=json!(1);layer["fadeIn"]=json!(0);layer["fadeOut"]=json!(0);
    layer["videoEffects"]=json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0,"ease":"linear"},{"at":1,"value":2,"ease":"linear"}]}}]);
    let report=convert(document.clone());assert_eq!(report.status,"converted","{:?}",report.diagnostics);
    let project=report.project.unwrap();let content=&project.compositions[&project.root].content;
    for source_frame in [37,38,40,49,60,61,62]{
        let progress=((source_frame as f64-38.)/23.).clamp(0.,1.);
        let mut frozen=document.clone();frozen["timelines"][0]["clips"][1]["videoEffects"]=json!([{"id":"concat.exposure","params":{"stops":2.*progress}}]);
        let expected=convert(frozen).project.unwrap();
        assert_eq!(frame(content,source_frame*40000),frame(&expected.compositions[&expected.root].content,source_frame*40000),"quantized source treatment key {source_frame}");
    }
    let CompositionTrack::Graphic{clips,..}=&content.tracks[1]else{panic!()};
    let clip=&clips[0];
    let first=38i128*10160640000;let end=61i128*10160640000;
    assert!(video_kadr_backend::domain::concat_treatment::sample_clock(clip,&content.canvas,first-1).unwrap().is_none());
    assert_eq!(video_kadr_backend::domain::concat_treatment::sample_clock(clip,&content.canvas,first).unwrap().unwrap().progress(),0.);
    assert!(video_kadr_backend::domain::concat_treatment::sample_clock(clip,&content.canvas,end-1).unwrap().is_some());
    assert!(video_kadr_backend::domain::concat_treatment::sample_clock(clip,&content.canvas,end).unwrap().is_none());
}

#[test]
fn native_graphic_timing_surrogate_preserves_disabled_tail_and_overlapping_lane_duration() {
    let project=convert(source()).project.unwrap();let mut original=project.compositions[&project.root].content.clone();
    let CompositionTrack::Graphic{clips,..}=&mut original.tracks[1]else{panic!()};
    let mut second=clips[0].clone();second.id=video_kadr_backend::domain::composition::CompositionClipId::parse("second-look").unwrap();second.timeline_start_tick=2000000;second.timeline_end_tick=4000000;clips.push(second);
    let mut inactive=clips[0].clone();inactive.id=video_kadr_backend::domain::composition::CompositionClipId::parse("inactive-tail").unwrap();inactive.timeline_start_tick=5000000;inactive.timeline_end_tick=7000000;inactive.enabled=false;clips.push(inactive);
    original.validate().unwrap();let before=serde_json::to_value(&original).unwrap();let duration=original.duration_ticks().unwrap();
    let mut timing=original.clone();video_kadr_backend::domain::transition_export::replace_native_graphics_with_timing(&mut timing);
    timing.validate().unwrap();assert_eq!(timing.duration_ticks().unwrap(),duration);assert_eq!(duration,7000000);
    let CompositionTrack::Color{clips,..}=&timing.tracks[1]else{panic!()};
    assert_eq!(clips.len(),1);assert_eq!(clips[0].timeline_start_tick,0);assert_eq!(clips[0].timeline_end_tick,7000000);assert_eq!(clips[0].color.a,0);
    assert_eq!(serde_json::to_value(&original).unwrap(),before);
}
