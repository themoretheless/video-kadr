mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
fn body(document: Value) -> Value {
    json!({"schemaVersion":1,"document":document,"mediaBindings":{"m1":{"id":"source","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}}})
}
fn source() -> Value {
    json!({"concat":"0.1.0","version":1,"name":"Test","media":[{"id":"m1","path":"/never/open-this.mov","kind":"video"}],"activeTimelineId":"TL2","timelines":[{"id":"TL1","name":"First","video":{"width":16,"height":16,"rateNum":4,"rateDen":1},"tracks":[{"id":"T1","visible":true,"muted":false}],"clips":[{"id":"clip","trackId":"T1","mediaId":"m1","kind":"video","start":1,"duration":1,"videoEffects":[{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0},{"at":1,"value":4,"ease":[0.42,-0.5,0.58,1]}]}}]}]},{"id":"TL2","name":"Second","tracks":[],"clips":[]}]})
}
fn request(value: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-project/import")
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}
#[tokio::test]
async fn imports_multiple_timelines_bindings_and_original_effect_keys() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let original = source();
    let (status, result, _) = support::send(&app, request(body(original.clone()))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["status"], "converted", "{result}");
    assert_eq!(result["sourceDocument"], original);
    assert_eq!(result["project"]["root"], "concat-timeline-1");
    assert_eq!(
        result["project"]["timelineOrder"],
        json!(["concat-timeline-0", "concat-timeline-1"])
    );
    let clip =
        &result["project"]["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0];
    assert_eq!(clip["sourceId"], "source");
    assert_eq!(clip["placement"]["timelineStartTick"], 1000000);
    assert_eq!(clip["effects"][0]["concatKeys"][1]["value"], 4.0);
    if let Ok(path) = std::env::var("CONCAT_IMPORTED_PROJECT_OUTPUT") {
        let mut render_document = source();
        render_document["activeTimelineId"] = json!("TL1");
        let (status, rendered, _) = support::send(&app, request(body(render_document))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rendered["status"], "converted", "{rendered}");
        std::fs::write(path, serde_json::to_vec(&rendered["project"]).unwrap()).unwrap();
    }
}
#[tokio::test]
async fn keeps_original_and_never_returns_a_partial_project_on_unsupported_content() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    for mode in 0..6 {
        let mut original = source();
        let mut value = body(original.clone());
        match mode {
            0 => {
                value["mediaBindings"] = json!({});
            }
            1 => {
                original["timelines"][0]["clips"][0]["crop"] = json!({"left":0.2,"futureCropField":true});
                value["document"] = original.clone();
            }
            2 => {
                original["timelines"][0]["clips"][0]["videoEffects"][0]["id"] =
                    json!("not-supported");
                value["document"] = original.clone();
            }
            3 => {
                original["version"] = json!(2);
                value["document"] = original.clone();
            }
            4 => {
                original["timelines"][0]["clips"][0]["videoEffects"][0]["enabled"] = json!(false);
                original["timelines"][0]["clips"][0]["videoEffects"][0]["id"] = json!("");
                value["document"] = original.clone();
            }
            _ => {
                original["futureFeature"] = json!({"keep":true});
                value["document"] = original.clone();
            }
        }
        let (status, result, _) = support::send(&app, request(value)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["status"], "needs_attention", "{result}");
        assert!(result["project"].is_null());
        assert_eq!(result["sourceDocument"], original);
        assert!(!result["diagnostics"].as_array().unwrap().is_empty());
    }
}

#[tokio::test]
async fn imports_legacy_flat_document_and_typed_audio_image_lanes() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut flat = source();
    let timeline = flat["timelines"][0].clone();
    flat.as_object_mut().unwrap().remove("timelines");
    for field in ["video", "tracks", "clips"] {
        flat[field] = timeline[field].clone();
    }
    let (status, result, _) = support::send(&app, request(body(flat))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["status"], "converted", "{result}");
    assert_eq!(
        result["project"]["compositions"].as_object().unwrap().len(),
        1
    );
    let mut mixed = source();
    mixed["media"].as_array_mut().unwrap().extend([
        json!({"id":"m2","kind":"image","path":"/never/image.png"}),
        json!({"id":"m3","kind":"audio","path":"/never/audio.wav"}),
    ]);
    mixed["timelines"][0]["tracks"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"T2","name":"Still","visible":false}),
            json!({"id":"T3","muted":true}),
        ]);
    mixed["timelines"][0]["clips"].as_array_mut().unwrap().extend([json!({"id":"still","trackId":"T2","mediaId":"m2","kind":"image","duration":1}),json!({"id":"audio","trackId":"T3","mediaId":"m3","kind":"audio","duration":1,"volume":0.5,"fadeIn":0.25})]);
    let mut value = body(mixed);
    value["mediaBindings"]["m2"] = json!({"id":"image-source","kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false});
    value["mediaBindings"]["m3"] = json!({"id":"audio-source","kind":"audio","durationTicks":1000000,"width":0,"height":0,"hasAudio":true});
    let (status, result, _) = support::send(&app, request(value)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["status"], "converted", "{result}");
    let tracks = &result["project"]["compositions"]["concat-timeline-0"]["content"]["tracks"];
    let tracks=tracks.as_array().unwrap();
    assert_eq!(tracks.iter().map(|track|track["id"].as_str().unwrap()).collect::<Vec<_>>(),vec!["lane-2-audio","lane-1-image","lane-0-video"]);
    let image=tracks.iter().find(|track|track["kind"]=="image").unwrap();
    let audio=tracks.iter().find(|track|track["kind"]=="audio").unwrap();
    assert_eq!(image["hidden"],true);
    assert_eq!(audio["muted"],true);
    assert_eq!(audio["clips"][0]["fadeInTicks"],250000);
    assert_eq!(audio["clips"][0]["gain"]["value"],0.5);
}

#[tokio::test]
#[ignore = "requires captured mounted UI request"]
async fn captured_mounted_ui_import_request() {
    let input = std::env::var("CONCAT_UI_IMPORT_REQUEST_INPUT").expect("captured UI request path");
    let value: Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let (status, result, _) = support::send(&app, request(value.clone())).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["status"], "converted", "{result}");
    assert_eq!(result["sourceDocument"], value["document"]);
    let path =
        std::env::var("CONCAT_UI_IMPORTED_PROJECT_OUTPUT").expect("converted UI project output");
    std::fs::write(path, serde_json::to_vec(&result["project"]).unwrap()).unwrap();
}

#[tokio::test]
async fn imports_audio_chain_order_bypass_aliases_and_muted_gain_without_partial_state() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut original = source();
    original["media"][0]["kind"] = json!("audio");
    let clip = &mut original["timelines"][0]["clips"][0];
    clip["kind"] = json!("audio");
    clip["videoEffects"] = json!([]);
    clip["volume"] = json!(0.5);
    clip["muted"] = json!(true);
    clip["filters"] = json!([{"id":"echo"},{"id":"hall","enabled":false,"params":{"size":75}}]);
    let mut value = body(original.clone());
    value["mediaBindings"]["m1"] = json!({"id":"source","kind":"audio","durationTicks":1000000,"width":0,"height":0,"hasAudio":true});
    let (status, result, _) = support::send(&app, request(value.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["status"], "converted", "{result}");
    let clip =
        &result["project"]["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0];
    assert_eq!(clip["gain"]["value"], 0.5);
    assert_eq!(clip["enabled"], false);
    assert_eq!(clip["dspEffects"][0]["id"], "concat.echo");
    assert_eq!(clip["dspEffects"][1]["id"], "concat.hall");
    assert_eq!(clip["dspEffects"][1]["bypass"], true);
    value["document"]["timelines"][0]["clips"][0]["filters"][0]["params"] =
        json!({"delay":0.0499999999});
    let (status, result, _) = support::send(&app, request(value.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["status"], "needs_attention");
    assert!(result["project"].is_null());
    assert_eq!(result["sourceDocument"], value["document"]);
}

#[tokio::test]
async fn embedded_video_dsp_stays_attached_and_independent_tracks_mix_without_legacy_regression(){
 use video_kadr_backend::domain::{nested_composition::NestedCompositionProject,nested_audio::plan_audio_tracks};
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);let mut original=source();original["timelines"][0]["clips"][0]["filters"]=json!([{"id":"echo"}]);original["timelines"][0]["clips"][0]["volume"]=json!(0.5);original["timelines"][0]["tracks"][0]["visible"]=json!(false);original["timelines"][0]["tracks"].as_array_mut().unwrap().push(json!({"id":"T2"}));let mut second=original["timelines"][0]["clips"][0].clone();second["id"]=json!("second");second["trackId"]=json!("T2");original["timelines"][0]["clips"].as_array_mut().unwrap().push(second);let mut value=body(original);value["mediaBindings"]["m1"]["hasAudio"]=json!(true);let(status,result,_)=support::send(&app,request(value)).await;assert_eq!(status,StatusCode::OK);assert_eq!(result["status"],"converted","{result}");let project:NestedCompositionProject=serde_json::from_value(result["project"].clone()).unwrap();let layers=plan_audio_tracks(&project,"concat-timeline-0",0..2000000).unwrap();assert_eq!(layers.len(),2);for layer in &layers{assert_eq!(layer.clip.dsp_effects[0].id,"concat.echo");assert_eq!(serde_json::to_value(&layer.clip.gain).unwrap()["value"],0.5);}
 assert!(video_kadr_backend::tools::composition_native_audio::needs_native_audio(&project.compositions["concat-timeline-0"].content));
 let mut legacy=result["project"].clone();for track in legacy["compositions"]["concat-timeline-0"]["content"]["tracks"].as_array_mut().unwrap(){for clip in track["clips"].as_array_mut().unwrap(){clip["independentAudio"]=json!(false);clip["dspEffects"]=json!([]);}}
 let legacy:NestedCompositionProject=serde_json::from_value(legacy).unwrap();assert_eq!(plan_audio_tracks(&legacy,"concat-timeline-0",0..2000000).unwrap().len(),1);
}

#[tokio::test]
async fn imports_embedded_audio_fades_with_overlap_and_refuses_invalid_durations() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut original=source();
    let clip=&mut original["timelines"][0]["clips"][0];
    clip["videoEffects"]=json!([]);clip["fadeIn"]=json!(0.75);clip["fadeOut"]=json!(0.75);clip["volume"]=json!(0.5);
    let mut input=body(original.clone());input["mediaBindings"]["m1"]["hasAudio"]=json!(true);
    let (_,report,_)=support::send(&app,request(input.clone())).await;
    assert_eq!(report["status"],"converted","{report}");
    let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();
    let layers=video_kadr_backend::domain::nested_audio::plan_audio_tracks(&project,"concat-timeline-0",1_500_000..1_750_000).unwrap();
    assert_eq!(layers.len(),1);
    let sample=layers[0].sample(1_500_000).unwrap().unwrap();
    assert!((sample.gain-0.5*4./9.).abs()<1e-12);assert_eq!(sample.source_time_us,500000);
    input["document"]["timelines"][0]["clips"][0]["fadeIn"]=json!(2.);
    input["document"]["timelines"][0]["clips"][0]["fadeOut"]=json!(2.);
    let (_,report,_)=support::send(&app,request(input.clone())).await;
    assert_eq!(report["status"],"converted","{report}");
    let project=serde_json::from_value(report["project"].clone()).unwrap();
    let mut layers=video_kadr_backend::domain::nested_audio::plan_audio_tracks(&project,"concat-timeline-0",1_500_000..1_750_000).unwrap();
    assert!((layers[0].sample(1_500_000).unwrap().unwrap().gain-0.5*0.25*0.25).abs()<1e-12);
    layers[0].export_fade_clock=true;
    assert!((layers[0].sample(1_500_000).unwrap().unwrap().gain-0.5*0.25*0.75).abs()<1e-12);
    let mut audio_input=input.clone();audio_input["document"]["timelines"][0]["clips"][0]["kind"]=json!("audio");
    let (_,report,_)=support::send(&app,request(audio_input)).await;
    assert_eq!(report["status"],"converted","{report}");
    let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();
    assert!(video_kadr_backend::tools::composition_native_audio::needs_native_audio(&project.compositions["concat-timeline-0"].content));
    for invalid in [-0.01,86400.01] {
        input["document"]["timelines"][0]["clips"][0]["fadeIn"]=json!(invalid);
        let (_,report,_)=support::send(&app,request(input.clone())).await;
        assert_eq!(report["status"],"needs_attention","{report}");assert!(report["project"].is_null());assert_eq!(report["sourceDocument"],input["document"]);
    }
}

#[tokio::test]
async fn imports_volume_keys_with_source_easing_rest_precedence_and_no_dsp_routing() {
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let mut input=body(source());input["mediaBindings"]["m1"]["hasAudio"]=json!(true);
    let clip=&mut input["document"]["timelines"][0]["clips"][0];clip["videoEffects"]=json!([]);clip["volume"]=json!(0.9);
    clip["keys"]=json!([{"property":"volume","at":0,"value":0},{"property":"volume","at":1,"value":16,"ease":[0.42,3,0.58,3]}]);
    for kind in ["video","audio"] {
        input["document"]["timelines"][0]["clips"][0]["kind"]=json!(kind);
        let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"converted","{report}");
        let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();
        let layers=video_kadr_backend::domain::nested_audio::plan_audio_tracks(&project,"concat-timeline-0",1500000..1750000).unwrap();assert_eq!(layers.len(),1);
        assert!((layers[0].sample(1500000).unwrap().unwrap().gain-38.).abs()<1e-9);
        assert!(video_kadr_backend::tools::composition_native_audio::needs_native_audio(&project.compositions["concat-timeline-0"].content));
    }
    input["document"]["timelines"][0]["clips"][0]["kind"]=json!("video");
    input["document"]["timelines"][0]["clips"][0]["keys"][0]["property"]=json!("unknown-property");
    let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"needs_attention");assert_eq!(report["sourceDocument"],input["document"]);assert!(report["project"].is_null());
}

#[tokio::test]
async fn imports_loose_detached_audio_provenance_without_doubling_video_audio() {
    let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let mut input=body(source());input["document"]["activeTimelineId"]=json!("TL1");input["mediaBindings"]["m1"]["hasAudio"]=json!(true);
    let timeline=&mut input["document"]["timelines"][0];timeline["tracks"]=json!([{"id":"T1"},{"id":"T2"},{"id":"T3"}]);timeline["clips"][0]["muted"]=json!(true);timeline["clips"][0]["videoEffects"]=json!([]);
    timeline["clips"].as_array_mut().unwrap().extend([json!({"id":"sound-b","trackId":"T3","mediaId":"m1","kind":"audio","start":1,"duration":1,"detachedFrom":"clip","filters":[{"id":"echo"}]}),json!({"id":"sound-a","trackId":"T2","mediaId":"m1","kind":"audio","start":1,"duration":1,"detachedFrom":"clip","filters":[{"id":"hall"}]})]);
    let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"converted","{report}");
    let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();
    let content=&project.compositions["concat-timeline-0"].content;
    let wire=serde_json::to_value(content).unwrap();
    let tracks=wire["tracks"].as_array().unwrap();
    assert_eq!(tracks.iter().map(|track|track["id"].as_str().unwrap()).collect::<Vec<_>>(),vec!["lane-2-audio","lane-1-audio","lane-0-video"]);
    let clip=|id:&str|tracks.iter().flat_map(|track|track["clips"].as_array().unwrap()).find(|clip|clip["id"]==id).unwrap();
    assert_eq!(clip("clip")["sourceAudioEnabled"],false);
    assert_eq!(clip("sound-a")["detachedFrom"],"clip");assert_eq!(clip("sound-a")["concatDetachedOrder"],2);
    assert_eq!(clip("sound-b")["concatDetachedOrder"],1);
    let layers=video_kadr_backend::domain::nested_audio::plan_audio_tracks(&project,"concat-timeline-0",1000000..2000000).unwrap();assert_eq!(layers.len(),2);assert!(layers.iter().all(|layer|layer.clip.id.as_str()!="clip"));
    if let Ok(path)=std::env::var("CONCAT_DETACHED_PROJECT_OUTPUT"){std::fs::write(path,serde_json::to_vec(&project).unwrap()).unwrap();}
    input["document"]["timelines"][0]["clips"][1]["audioStream"]=json!(-1);
    let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"needs_attention");assert_eq!(report["sourceDocument"],input["document"]);assert!(report["project"].is_null());
}

#[tokio::test]
async fn audio_import_ignores_inherited_picture_controls_but_preserves_active_audio_and_source() {
    let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let mut input=body(source());input["mediaBindings"]["m1"]["hasAudio"]=json!(true);
    let clip=&mut input["document"]["timelines"][0]["clips"][0];clip["kind"]=json!("audio");clip["filters"]=json!([{"id":"echo"}]);clip["volume"]=json!(0.5);clip["videoEffects"]=json!([]);clip["keys"]=json!([{"property":"volume","at":0,"value":0.25},{"property":"volume","at":1,"value":1,"ease":"inOut"}]);
    let(_,plain,_)=support::send(&app,request(input.clone())).await;assert_eq!(plain["status"],"converted","{plain}");
    let clip=input["document"]["timelines"][0]["clips"][0].as_object_mut().unwrap();
    for(key,value)in json!({"scale":2,"stretchX":1.3,"stretchY":0.8,"offsetX":0.2,"offsetY":-0.3,"rotation":30,"opacity":0.4,"blend":"multiply","flipH":true,"flipV":true,"crop":{"left":0.1,"right":0.1,"top":0.2,"bottom":0},"cutout":{"mode":"auto","feather":0.02,"strokes":[]},"videoEffects":[{"id":"concat.exposure","params":{"stops":2}}],"keys":[{"property":"opacity","at":0,"value":0},{"property":"volume","at":0,"value":0.25},{"property":"scale","at":1,"value":2},{"property":"volume","at":1,"value":1,"ease":"inOut"}]}).as_object().unwrap(){clip.insert(key.clone(),value.clone());}
    let(_,decorated,_)=support::send(&app,request(input.clone())).await;assert_eq!(decorated["status"],"converted","{decorated}");assert_eq!(decorated["sourceDocument"],input["document"]);assert_eq!(decorated["project"],plain["project"]);
    input["document"]["timelines"][0]["clips"][0]["filters"]=json!([{"id":"unknown-audible-filter"}]);
    let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"needs_attention");assert!(report["project"].is_null());assert_eq!(report["sourceDocument"],input["document"]);
}

#[tokio::test]
async fn retains_inactive_audio_filter_keys_in_source_document_and_uses_static_audio_params(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for kind in ["audio","video"]{
  let mut original=source();original["timelines"][0]["clips"][0]["kind"]=json!(kind);original["timelines"][0]["clips"][0]["videoEffects"]=json!([]);original["timelines"][0]["clips"][0]["filters"]=json!([{"id":"concat.echo","params":{"delay":0.25},"keys":{"delay":[{"at":0,"value":0.05},{"at":1,"value":1,"ease":"inOut"}]}}]);
  let mut input=body(original.clone());input["mediaBindings"]["m1"]["hasAudio"]=json!(true);
  let(_,result,_)=support::send(&app,request(input)).await;assert_eq!(result["status"],"converted","{result}");assert_eq!(result["sourceDocument"],original);assert_eq!(result["diagnostics"].as_array().unwrap().len(),1);
  let effect=&result["project"]["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["dspEffects"][0];assert_eq!(effect["params"]["delay"],0.25);assert!(effect["animation"].is_null());assert_eq!(effect["concatSourceKeys"]["delay"][1]["value"].as_f64(),Some(1.));
 }
}

#[tokio::test]
async fn preserves_disabled_visual_links_in_order_and_rendering_skips_them(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let mut original=source();original["timelines"][0]["clips"][0]["videoEffects"]=json!([
  {"id":"concat.exposure","params":{"stops":1}},
  {"id":"unknown.saved.package","enabled":false,"params":{"custom":123},"keys":{"custom":[{"at":0,"value":-7},{"at":1,"value":9,"ease":"inOut"}]}},
  {"id":"concat.exposure","enabled":false,"params":{"stops":4},"keys":{"stops":[{"at":0,"value":0},{"at":1,"value":4,"ease":"inOut"}]}}
 ]);
 let(_,result,_)=support::send(&app,request(body(original.clone()))).await;assert_eq!(result["status"],"converted","{result}");assert_eq!(result["sourceDocument"],original);
 let descriptors=&result["project"]["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["effects"];
 assert_eq!(descriptors[1]["kind"],"concat_bypass");assert_eq!(descriptors[1]["link"]["id"],"unknown.saved.package");assert_eq!(descriptors[2]["link"]["params"]["stops"].as_f64(),Some(4.));
 let effects:Vec<video_kadr_backend::domain::composition::VideoEffect>=serde_json::from_value(descriptors.clone()).unwrap();
 let source=video_kadr_backend::domain::group_surface::GroupSurface{width:2,height:1,rgba:vec![32,64,128,255,170,100,50,128]};let mut actual=source.clone();let mut expected=source.clone();
 video_kadr_backend::domain::flat_surface::apply_effect_stack(&mut actual,&effects,500000,&[],&mut||Ok(())).unwrap();
 video_kadr_backend::domain::flat_surface::apply_effect_stack(&mut expected,&effects[..1],500000,&[],&mut||Ok(())).unwrap();assert_eq!(actual.rgba,expected.rgba);
 let mut unchanged=source.clone();video_kadr_backend::domain::flat_surface::apply_effect_stack(&mut unchanged,&effects[1..],500000,&[],&mut||Ok(())).unwrap();assert_eq!(unchanged.rgba,source.rgba);
 let mut invalid=effects[1].clone();if let video_kadr_backend::domain::composition::VideoEffect::ConcatBypass{link}=&mut invalid{link.enabled=true;}
 let mut unchanged=source.clone();assert!(video_kadr_backend::domain::flat_surface::apply_effect_stack(&mut unchanged,&[invalid],0,&[],&mut||Ok(())).is_err());assert_eq!(unchanged.rgba,source.rgba);
 let mut active_link=descriptors[2]["link"].clone();active_link["enabled"]=json!(true);
 let upgrade_request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":[active_link],"includeNativeEffects":true}).to_string())).unwrap();let(status,upgraded,_)=support::send(&app,upgrade_request).await;assert_eq!(status,StatusCode::OK,"{upgraded}");assert_eq!(upgraded["nativeEffects"][0]["kind"],"concat_exposure");
 if let Ok(path)=std::env::var("VISUAL_BYPASS_UPGRADE_OUTPUT"){std::fs::write(path,serde_json::to_vec(&upgraded).unwrap()).unwrap();}
 let enabled:video_kadr_backend::domain::composition::VideoEffect=serde_json::from_value(upgraded["nativeEffects"][0].clone()).unwrap();
 let render=|stack:Vec<video_kadr_backend::domain::composition::VideoEffect>|{
  let mut composition:video_kadr_backend::domain::composition::Composition=serde_json::from_value(result["project"]["compositions"]["concat-timeline-0"]["content"].clone()).unwrap();
  let video_kadr_backend::domain::composition::CompositionTrack::Video{clips,..}=&mut composition.tracks[0]else{panic!()};clips[0].effects=stack;
  let mut decode=|_:&video_kadr_backend::domain::composition::SourceId,_:u64,_:bool|Ok(video_kadr_backend::domain::group_surface::GroupSurface{width:16,height:16,rgba:source.rgba.iter().copied().cycle().take(16*16*4).collect()});
  video_kadr_backend::domain::flat_surface::render_media_layers(&composition,1500000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap()
 };
 let reenabled=render(vec![effects[0].clone(),enabled.clone()]);assert_ne!(reenabled.rgba,render(effects.clone()).rgba);
 let sampled=serde_json::from_value(json!({"kind":"concat_exposure","parameters":{"stops":2}})).unwrap();assert_eq!(reenabled.rgba,render(vec![effects[0].clone(),sampled]).rgba);
 if let Ok(path)=std::env::var("VISUAL_BYPASS_REACTIVATED_INPUT"){
  let captured:video_kadr_backend::domain::composition::VideoEffect=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(captured,enabled);assert_eq!(render(vec![effects[0].clone(),captured]).rgba,reenabled.rgba);
 }
 if let Ok(path)=std::env::var("ORDINARY_VISUAL_BYPASS_PROJECT_INPUT"){
  let project:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/nested-frame").header("content-type","application/json").body(Body::from(json!({"project":project,"timeUs":500000}).to_string())).unwrap();
  let(status,frame,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"ordinary activation candidate failed actual native validation: {frame}");assert!(!frame["entries"].as_array().unwrap().is_empty());
 }
 let restored:Vec<video_kadr_backend::domain::composition::VideoEffect>=serde_json::from_slice(&serde_json::to_vec(&effects).unwrap()).unwrap();assert_eq!(restored,effects);
}

#[test]
fn native_bypass_preserves_pixels_and_exact_descriptor() {
 use video_kadr_backend::domain::{composition::VideoEffect,group_surface::GroupSurface,flat_surface::apply_effect_stack};
 let active:VideoEffect=serde_json::from_value(json!({"kind":"concat_exposure","parameters":{"stops":1}})).unwrap();
 let bypass=VideoEffect::NativeBypass{effect:Box::new(active.clone())};
 let restored:VideoEffect=serde_json::from_value(serde_json::to_value(&bypass).unwrap()).unwrap();
 assert_eq!(serde_json::to_value(&restored).unwrap(),serde_json::to_value(&bypass).unwrap());
 let source=GroupSurface{width:2,height:1,rgba:vec![32,64,128,255,170,100,50,128]};
 let mut disabled=source.clone();apply_effect_stack(&mut disabled,&[restored],0,&[],&mut||Ok(())).unwrap();assert_eq!(disabled.rgba,source.rgba);
 let mut enabled=source.clone();apply_effect_stack(&mut enabled,&[active],0,&[],&mut||Ok(())).unwrap();assert_ne!(enabled.rgba,source.rgba);
 let nested=VideoEffect::NativeBypass{effect:Box::new(bypass)};
 let mut invalid=source.clone();assert!(apply_effect_stack(&mut invalid,&[nested],0,&[],&mut||Ok(())).is_err());assert_eq!(invalid.rgba,source.rgba);
}

#[tokio::test]
async fn imports_visual_clip_keys_and_renders_their_source_clock(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let mut input=body(source());let clip=&mut input["document"]["timelines"][0]["clips"][0];clip["videoEffects"]=json!([]);clip["opacity"]=json!(0);
 clip["keys"]=json!([{"property":"opacity","at":0,"value":0},{"property":"opacity","at":1,"value":1,"ease":"inOut"},{"property":"scale","at":0,"value":1},{"property":"scale","at":1,"value":1},{"property":"rotation","at":0,"value":0},{"property":"rotation","at":1,"value":720},{"property":"offsetX","at":0,"value":0},{"property":"offsetY","at":1,"value":0}]);
 let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"converted","{report}");assert_eq!(report["sourceDocument"],input["document"]);
 let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();
 let composition=&project.compositions["concat-timeline-0"].content;
 assert!(video_kadr_backend::domain::flat_surface::requires_native_video(composition));
 let video_kadr_backend::domain::composition::CompositionTrack::Video{clips,..}=&composition.tracks[0] else{panic!()};
 let animation=clips[0].transform.concat_animation.as_ref().unwrap();assert_eq!(animation.keys.len(),5);assert_eq!(animation.sample(500000).unwrap().rotation_degrees,360.);assert_eq!(animation.sample(500000).unwrap().opacity,0.5);
 let mut decode=|_:&video_kadr_backend::domain::composition::SourceId,_:u64,_:bool|Ok(video_kadr_backend::domain::group_surface::GroupSurface{width:16,height:16,rgba:[255,255,255,255].repeat(256)});
 let frame=video_kadr_backend::domain::flat_surface::render_media_layers(composition,1500000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
 assert!(frame.rgba.chunks_exact(4).all(|pixel|pixel==[191,191,191,255]));
 if let Ok(path)=std::env::var("CONCAT_VISUAL_POSE_PROJECT_OUTPUT"){std::fs::write(path,serde_json::to_vec(&report["project"]).unwrap()).unwrap();}
}

#[tokio::test]
async fn imports_crop_fit_flips_and_static_gamma_for_video_and_image(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for kind in ["video","image"]{for stack in [0,1,2]{
  let mut input=body(source());input["mediaBindings"]["m1"]["kind"]=json!(kind);input["mediaBindings"]["m1"]["width"]=json!(4);input["mediaBindings"]["m1"]["height"]=json!(2);
  input["document"]["media"][0]["kind"]=json!(kind);input["document"]["timelines"][0]["video"]["width"]=json!(2);input["document"]["timelines"][0]["video"]["height"]=json!(2);
  let clip=&mut input["document"]["timelines"][0]["clips"][0];clip["kind"]=json!(kind);clip["crop"]=json!({"left":0.5,"top":0,"right":0,"bottom":0});clip["flipH"]=json!(true);
  clip["videoEffects"]=match stack{0=>json!([]),1=>json!([{"id":"concat.exposure","params":{"stops":0}}]),_=>json!([{"id":"unknown.saved.package","enabled":false}])};
  let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"converted","{report}");assert_eq!(report["sourceDocument"],input["document"]);
  let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();let composition=&project.compositions["concat-timeline-0"].content;
  let picture=||video_kadr_backend::domain::group_surface::GroupSurface{width:4,height:2,rgba:[[255,0,0,255],[255,0,0,255],[0,0,255,255],[0,0,255,255]].repeat(2).into_iter().flatten().collect()};
  let mut image=|_:&video_kadr_backend::domain::composition::SourceId|Ok(picture());let mut video=|_:&video_kadr_backend::domain::composition::SourceId,_:u64,_:bool|Ok(picture());
  let frame=video_kadr_backend::domain::flat_surface::render_media_layers(composition,1500000,&mut||Ok(()),Some(&mut image),Some(&mut video)).unwrap().unwrap();assert_eq!(frame.rgba,[0,0,255,255].repeat(4),"{kind} stack{stack}");
 }}
 let mut input=body(source());input["document"]["timelines"][0]["clips"][0]["videoEffects"]=json!([]);input["document"]["timelines"][0]["clips"][0]["opacity"]=json!(0.5);
 let(_,report,_)=support::send(&app,request(input)).await;assert_eq!(report["status"],"converted","{report}");
 let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(report["project"].clone()).unwrap();let c=&project.compositions["concat-timeline-0"].content;
 let mut decode=|_:&video_kadr_backend::domain::composition::SourceId,_:u64,_:bool|Ok(video_kadr_backend::domain::group_surface::GroupSurface{width:16,height:16,rgba:[255;4].repeat(256)});
 let frame=video_kadr_backend::domain::flat_surface::render_media_layers(c,1500000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();assert!(frame.rgba.chunks_exact(4).all(|p|p==[191,191,191,255]));
}

#[tokio::test]
async fn original_add_blend_alias_imports_and_unknown_blends_refuse(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let mut input=body(source());input["document"]["timelines"][0]["clips"][0]["blend"]=json!("add");
 let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"converted","{report}");assert_eq!(report["project"]["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["blendMode"],"addition");
 input["document"]["timelines"][0]["clips"][0]["blend"]=json!("overlay");let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"needs_attention");assert!(report["project"].is_null());assert_eq!(report["sourceDocument"],input["document"]);
}

#[tokio::test]
async fn cutout_import_preserves_source_and_names_real_analysis_dependencies(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let mut input=body(source());input["document"]["timelines"][0]["clips"][0]["cutout"]=json!({"mode":"custom","subject":"object","feather":0.02,"strokes":[{"tool":"smartBrush","size":0.1,"points":[[0.5,0.5]],"at":0.25}]});
 let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"needs_attention");assert!(report["project"].is_null());assert_eq!(report["sourceDocument"],input["document"]);
 let diagnostic=report["diagnostics"].as_array().unwrap().iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n");assert!(diagnostic.contains("source-owned model/mask analysis"),"{diagnostic}");assert!(diagnostic.contains("subject=Object"),"{diagnostic}");assert!(diagnostic.contains("smart source frames=1"),"{diagnostic}");
 input["document"]["timelines"][0]["clips"][0]["cutout"]["future"]=json!(true);let(_,report,_)=support::send(&app,request(input.clone())).await;assert_eq!(report["status"],"needs_attention");assert!(report["project"].is_null());assert_eq!(report["sourceDocument"],input["document"]);
}
