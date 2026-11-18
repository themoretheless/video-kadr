use serde_json::{json,Value};
use video_kadr_backend::domain::{concat_project_import::{import,ImportRequest},concat_project_export::export,flat_surface,composition::Composition};
fn convert(document:Value)->video_kadr_backend::domain::nested_composition::NestedCompositionProject{let report=import(serde_json::from_value::<ImportRequest>(json!({"schemaVersion":1,"document":document,"mediaBindings":{}})).unwrap()).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);report.project.unwrap()}
fn frame(composition:&Composition,time:u64)->Vec<u8>{flat_surface::render_generated(composition,time,&mut||Ok(())).unwrap().unwrap().rgba}
fn source()->Value{json!({"version":1,"media":[],"activeTimelineId":"TL","timelines":[{"id":"TL","name":"Interchange","video":{"width":160,"height":120,"rateNum":30000,"rateDen":1001},"tracks":[{"id":"bottom","name":"Ground"},{"id":"look","name":"Treatment"},{"id":"top","name":"Words"}],"clips":[{"id":"ground","trackId":"bottom","kind":"shape","start":0,"duration":4,"shape":{"kind":"arrow","size":0.8,"fill":"#804020","strokeWidth":0},"flipH":true,"crop":{"left":0.05},"keys":[{"property":"rotation","at":0,"value":0},{"property":"rotation","at":1,"value":90}]},{"id":"look","trackId":"look","kind":"layer","start":1,"duration":2,"opacity":0.5,"fadeIn":0.5,"fadeOut":0.5,"videoEffects":[{"id":"concat.exposure","params":{"stops":1},"keys":{"stops":[{"at":0,"value":0},{"at":1,"value":2}]}}]},{"id":"words","trackId":"top","kind":"text","start":0,"duration":4,"text":{"content":"Round trip","fontSize":0.2},"offsetY":0.2,"rotation":12}]}]})}
#[test]
fn template_export_preserves_an_empty_active_timeline_and_unused_slot() {
    use std::collections::BTreeMap;
    use video_kadr_backend::domain::concat_project_export::export_with_paths;
    let mut document = source();
    document["activeTimelineId"] = json!("empty");
    document["timelines"].as_array_mut().unwrap().push(json!({
        "id":"empty", "name":"Empty", "tracks":[], "clips":[],
        "video":{"width":120,"height":160,"rateNum":24000,"rateDen":1001}
    }));
    document["media"] = json!([{"id":"slot","kind":"video","path":"","placeholder":true}]);
    let imported = import(serde_json::from_value(json!({
        "schemaVersion":1, "document":document,
        "mediaBindings":{"slot":{"id":"slot","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}},
    })).unwrap()).unwrap();
    assert_eq!(imported.status, "converted", "{:?}", imported.diagnostics);
    let project = imported.project.unwrap();
    let output = export_with_paths(&project, &BTreeMap::from([("slot".into(), String::new())])).unwrap();
    assert_eq!(output["activeTimelineId"], project.root);
    assert_eq!(output["video"], output["timelines"][1]["video"]);
    assert_eq!(output["video"]["rateDen"], 1001);
    assert_eq!(output["timelines"][1]["tracks"].as_array().unwrap().len(), 1);
    assert_eq!(output["timelines"][1]["clips"], json!([]));
    assert_eq!(output["media"][0]["placeholder"], true);
    assert_eq!(output["media"][0]["path"], "");
    if let Ok(path) = std::env::var("CONCAT_EXPORTED_TEMPLATE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&output).unwrap()).unwrap();
    }
}

#[test]
fn retained_three_kind_project_exports_actual_source_json_and_reimports_exact_frames(){
 let project=convert(source());let before=serde_json::to_value(&project).unwrap();let document=export(&project).unwrap();if let Ok(path)=std::env::var("CONCAT_EXPORTED_PROJECT_OUTPUT"){std::fs::write(path,serde_json::to_vec(&document).unwrap()).unwrap();}let restored=convert(document.clone());
 assert_eq!(document["activeTimelineId"],project.root);assert_eq!(document["timelines"][0]["video"]["rateNum"],30000);
 for time in [0,500000,1000000,1250000,1750000,2500000,2999999,3500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"time {time}");}
 if let Ok(path)=std::env::var("CONCAT_REWRITTEN_PROJECT_INPUT"){
  let rewritten:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let restored=convert(rewritten);
  for time in [0,500000,1250000,1750000,2500000,3500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"Original reader/writer at {time}");}
  println!("Original Concat reader/writer roundtrip: 6 exact RGBA frames passed");
 }
 assert_eq!(serde_json::to_value(&project).unwrap(),before);
}
#[test]
fn native_only_animation_refuses_without_altering_project(){
 let project=convert(source());let mut value=serde_json::to_value(&project).unwrap();value["compositions"][&project.root]["content"]["tracks"][1]["clips"][0]["effects"][0].as_object_mut().unwrap().remove("concatKeys");value["compositions"][&project.root]["content"]["tracks"][1]["clips"][0]["effects"][0]["animation"]=json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":1000000,"value":0},{"tick":3000000,"value":2}]}});
 let project=serde_json::from_value(value.clone()).unwrap();let before=serde_json::to_value(&project).unwrap();let error=export(&project).unwrap_err();assert!(format!("{error:#}").contains("cannot be exported losslessly"),"{error:#}");assert_eq!(serde_json::to_value(project).unwrap(),before);
}
#[test]
fn scalar_source_packages_and_disabled_owners_have_exact_reverse_links(){
 use video_kadr_backend::domain::{concat_effect_upgrade::{AppliedFilter,native_effect},concat_effect_export::export_effect,composition::VideoEffect};
 for name in ["exposure","contrast","saturation","white-balance","glow","fisheye","motion-blur","gaussian-blur","halftone","pixelate","vibrance","posterize","film-grain","mono","film","digicam","midnight","vhs"]{
  let link:AppliedFilter=serde_json::from_value(json!({"id":format!("concat.{name}")})).unwrap();let effect=native_effect(&link).unwrap().unwrap_or_else(||panic!("missing source adapter: {name}"));let exported=export_effect(&effect).unwrap_or_else(|error|panic!("{name}: {error:#}"));let restored=native_effect(&exported).unwrap().unwrap();assert_eq!(serde_json::to_value(&effect).unwrap(),serde_json::to_value(restored).unwrap(),"{name}");
  let disabled=export_effect(&VideoEffect::NativeBypass{effect:Box::new(effect)}).unwrap();assert!(!disabled.enabled);assert_eq!(disabled.id,link.id);
 }
}
#[test]
fn all_seven_shapes_and_portable_scalar_keys_survive_source_roundtrip(){
 for kind in ["square","circle","triangle","parallelogram","trapezoid","line","arrow"]{
  let mut document=source();document["timelines"][0]["clips"][0]["shape"]["kind"]=json!(kind);
  let project=convert(document);let restored=convert(export(&project).unwrap());
  for time in [250000,1750000,3500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"{kind} at {time}");}
 }
}
#[test]
fn incompatible_native_workspace_state_is_never_silently_exported(){
 let original=convert(source());let before=serde_json::to_value(&original).unwrap();
 for mode in ["lock","tail","background"]{
  let mut value=before.clone();let node=&mut value["compositions"][&original.root];
  match mode{"lock"=>node["content"]["tracks"][0]["locked"]=json!(true),"tail"=>node["durationUs"]=json!(4500000),_=>node["content"]["canvas"]["background"]["red"]=json!(0.2)}
  let project=serde_json::from_value(value.clone()).unwrap();assert!(export(&project).is_err(),"{mode}");assert_eq!(serde_json::to_value(project).unwrap(),value);
 }
}
#[test]
fn disabled_plugin_parameter_names_are_not_confused_with_key_metadata(){
 let mut document=source();document["timelines"][0]["clips"][1]["videoEffects"]=json!([{"id":"custom.plugin","enabled":false,"params":{"at":0.1,"value":0.2,"derived":0.7},"keys":{"derived":[{"at":0.5,"value":0.8}]}}]);
 let project=convert(document);let output=export(&project).unwrap();let link=&output["timelines"][0]["clips"].as_array().unwrap().iter().find(|clip|clip["id"]=="look").unwrap()["videoEffects"][0];
 assert_eq!(link["params"]["derived"].as_f64(),Some(0.7));assert_eq!(link["keys"]["derived"][0]["value"].as_f64(),Some(0.8));let restored=convert(output);assert_eq!(frame(&project.compositions[&project.root].content,1750000),frame(&restored.compositions[&restored.root].content,1750000));
}
#[test]
fn mixed_native_and_source_pose_animation_refuses_instead_of_dropping_native_keys(){
 let project=convert(source());let mut value=serde_json::to_value(&project).unwrap();value["compositions"][&project.root]["content"]["tracks"][2]["clips"][0]["transform"]["scaleX"]=json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1.0},{"tick":4000000,"value":2.0}]}});
 let project=serde_json::from_value(value).unwrap();let before=serde_json::to_value(&project).unwrap();assert!(format!("{:#}",export(&project).unwrap_err()).contains("Native animation needs source-key conversion"));assert_eq!(serde_json::to_value(&project).unwrap(),before);
}
#[test]
fn composite_color_parameters_and_source_wheel_keys_roundtrip_structurally_and_in_pixels(){
 use video_kadr_backend::domain::{concat_effect_upgrade::{AppliedFilter,native_effect},concat_effect_export::export_effect};
 for id in ["concat.adjust","concat.color-wheels","concat.curves"]{
  let mut params=json!({});
  if id!="concat.curves"{params["lift.x"]=json!(0.2);params["lift.y"]=json!(-0.1);params["gamma.m"]=json!(0.15);params["gain.x"]=json!(-0.2);params["gain.y"]=json!(0.1);}
  if id!="concat.color-wheels"{for (index,point) in [[0.0,0.0],[0.4,0.5],[1.0,1.0]].iter().enumerate(){params[format!("luma.{index}.x")]=json!(point[0]);params[format!("luma.{index}.y")]=json!(point[1]);}}
  if id=="concat.adjust"{params["brightness"]=json!(12.0);params["temperature"]=json!(5600.0);}
  let mut link=json!({"id":id,"params":params});if id!="concat.curves"{link["keys"]=json!({"lift.x":[{"at":0.0,"value":-0.2,"ease":"inOut"},{"at":1.0,"value":0.3}],"gamma.m":[{"at":0.0,"value":0.0},{"at":1.0,"value":0.2}]});}
  let typed:AppliedFilter=serde_json::from_value(link.clone()).unwrap();let effect=native_effect(&typed).unwrap().unwrap();let exported=export_effect(&effect).unwrap();let restored=native_effect(&exported).unwrap().unwrap();assert_eq!(serde_json::to_value(&effect).unwrap(),serde_json::to_value(restored).unwrap(),"{id}");
  let mut document=source();document["timelines"][0]["clips"][1]["videoEffects"]=json!([link]);let project=convert(document);let output=export(&project).unwrap();if id=="concat.adjust"{if let Ok(path)=std::env::var("CONCAT_EXPORTED_COMPOSITE_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}}
  if id=="concat.adjust"{if let Ok(path)=std::env::var("CONCAT_REWRITTEN_COMPOSITE_INPUT"){
   let rewritten:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let restored=convert(rewritten);
   for time in [1250000,1750000,2500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"Original composite reader/writer at {time}");}
   println!("Original composite reader/writer roundtrip: 3 exact RGBA frames passed");
  }}
  let restored=convert(output);for time in [1250000,1750000,2500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"{id} at {time}");}
 }
}
#[test]
fn noncanonical_native_curve_ownership_is_not_silently_rewritten(){
 use video_kadr_backend::domain::{composition::VideoEffect,concat_effect_export::export_effect};
 let effect:VideoEffect=serde_json::from_value(json!({"kind":"concat_curves","parameters":{"luma":[[0.2,0.3]],"red":[],"green":[],"blue":[]}})).unwrap();let before=serde_json::to_value(&effect).unwrap();assert!(export_effect(&effect).is_err());assert_eq!(serde_json::to_value(effect).unwrap(),before);
}
#[test]
fn explicit_media_paths_roundtrip_audio_video_and_image_without_opening_paths(){
 use video_kadr_backend::domain::{concat_project_export::export_with_paths,composition::CompositionSource};
 use std::collections::BTreeMap;
 let mut document=source();document["media"]=json!([{"id":"snd","kind":"audio","path":"/missing/snd.wav"},{"id":"vid","kind":"video","path":"/missing/vid.mov"},{"id":"pic","kind":"image","path":"/missing/pic.png"}]);
 let tracks=document["timelines"][0]["tracks"].as_array_mut().unwrap();for id in ["sound","picture","movie"]{tracks.push(json!({"id":id,"name":id}));}
 let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();
 clips.push(json!({"id":"sound","trackId":"sound","kind":"audio","mediaId":"snd","start":0.25,"duration":2,"sourceStart":0.5,"speed":1.5,"volume":0.7,"fadeIn":0.1,"fadeOut":0.2,"keys":[{"property":"volume","at":0,"value":0.2},{"property":"volume","at":1,"value":0.8,"ease":[0.2,-0.1,0.85,1.2]}],"filters":[{"id":"concat.echo","params":{"delay":0.25,"decay":0.4},"keys":{"delay":[{"at":0.5,"value":0.4}]}},{"id":"concat.hall","enabled":false}]}));
 clips.push(json!({"id":"picture","trackId":"picture","kind":"image","mediaId":"pic","start":0,"duration":4,"flipH":true,"offsetX":0.2,"keys":[{"property":"rotation","at":0,"value":0},{"property":"rotation","at":1,"value":45}]}));
 clips.push(json!({"id":"movie","trackId":"movie","kind":"video","mediaId":"vid","start":0.5,"duration":3,"sourceStart":0.75,"speed":1.25,"volume":0.5,"audioStream":1,"fadeIn":0.2,"fadeOut":0.3,"crop":{"left":0.1},"videoEffects":[{"id":"concat.exposure","params":{"stops":0.5}}]}));
 let bindings:BTreeMap<String,CompositionSource>=serde_json::from_value(json!({"snd":{"id":"snd","kind":"audio","durationTicks":10000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2},"vid":{"id":"vid","kind":"video","durationTicks":10000000,"width":160,"height":120,"hasAudio":true,"audioChannels":2},"pic":{"id":"pic","kind":"image","durationTicks":0,"width":160,"height":120,"hasAudio":false}})).unwrap();
 let report=import(serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":bindings})).unwrap()).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);let project=report.project.unwrap();let before=serde_json::to_value(&project).unwrap();
 assert!(export(&project).is_err());let paths=BTreeMap::from([("snd".into(),"/unopened/snd.wav".into()),("vid".into(),"/unopened/vid.mov".into()),("pic".into(),"/unopened/pic.png".into())]);let output=export_with_paths(&project,&paths).unwrap();
 let sound = output["timelines"][0]["clips"].as_array().unwrap().iter()
     .find(|clip| clip["id"] == "sound").unwrap();
 assert_eq!(sound["keys"][1]["ease"], json!([0.2, -0.1, 0.85, 1.2]));
 let mut trimmed = before.clone();
 let boundary = &mut trimmed["compositions"][&project.root]["content"]["tracks"][2]["clips"][0]["concatGainKeys"][0];
 boundary["derived"] = json!(true);
 boundary["value"] = json!(0.35);
 let trimmed_project = serde_json::from_value(trimmed.clone()).unwrap();
 let trimmed_output = export_with_paths(&trimmed_project, &paths).unwrap();
 let volume = trimmed_output["timelines"][0]["clips"].as_array().unwrap().iter()
     .find(|clip| clip["id"] == "sound").unwrap()["keys"][0].clone();
 assert_eq!(volume, json!({"property":"volume","at":0.0,"value":0.35,"ease":[0.0,0.0,1.0,1.0]}));
 assert_eq!(serde_json::to_value(&trimmed_project).unwrap(), trimmed);
 trimmed["compositions"][&project.root]["content"]["tracks"][2]["clips"][0]["concatGainKeys"][0]["value"] = json!(-0.1);
 let negative = serde_json::from_value(trimmed).unwrap();
 assert!(export_with_paths(&negative, &paths).unwrap_err().to_string().contains("Negative boundary gain"));
 let mut slot_paths = paths.clone();
 slot_paths.insert("vid".into(), String::new());
 assert!(export_with_paths(&project, &slot_paths).is_err());
 let mut slot_document = output.clone();
 slot_document["media"].as_array_mut().unwrap().iter_mut()
     .find(|entry| entry["id"] == "vid").unwrap()["placeholder"] = json!(true);
 let slots = import(serde_json::from_value(json!({
     "schemaVersion": 1, "document": slot_document, "mediaBindings": bindings,
     "preserveMediaIds": true,
 })).unwrap()).unwrap().project.unwrap();
 let template = export_with_paths(&slots, &slot_paths).unwrap();
 let slot = template["media"].as_array().unwrap().iter().find(|entry| entry["id"] == "vid").unwrap();
 assert_eq!(slot["path"], "");
 assert_eq!(slot["placeholder"], true);
 if let Ok(path)=std::env::var("CONCAT_EXPORTED_MEDIA_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
 let restored=import(serde_json::from_value(json!({"schemaVersion":1,"document":output,"mediaBindings":bindings})).unwrap()).unwrap().project.unwrap();
 assert_eq!(project.compositions[&project.root].content.tracks,restored.compositions[&restored.root].content.tracks);
 if let Ok(path)=std::env::var("CONCAT_REWRITTEN_MEDIA_INPUT"){let rewritten:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let restored=import(serde_json::from_value(json!({"schemaVersion":1,"document":rewritten,"mediaBindings":bindings})).unwrap()).unwrap();assert_eq!(restored.status,"converted","{:?}",restored.diagnostics);let restored=restored.project.unwrap();assert_eq!(project.compositions[&project.root].content.tracks,restored.compositions[&restored.root].content.tracks);}
 // Compare actual DSP output, including the retained inactive source parameter keys.
 use video_kadr_backend::domain::{composition::CompositionTrack,filmcraft_dsp::Chain};
 let CompositionTrack::Audio{clips:original_audio,..}=&project.compositions[&project.root].content.tracks[2] else{panic!("audio track missing")};let CompositionTrack::Audio{clips:restored_audio,..}=&restored.compositions[&restored.root].content.tracks[2] else{panic!("restored audio missing")};
 let signal=(0..24000).map(|i|if i==0{0.8}else{(i as f32*0.057).sin()*0.2}).collect::<Vec<_>>();let mut reference=signal.clone();let mut actual=signal;Chain::new(48000,1,&original_audio[0].dsp_effects).unwrap().process(&mut[reference.as_mut_slice()],&mut||Ok(())).unwrap();Chain::new(48000,1,&restored_audio[0].dsp_effects).unwrap().process(&mut[actual.as_mut_slice()],&mut||Ok(())).unwrap();assert_eq!(reference,actual);
 assert_eq!(serde_json::to_value(&project).unwrap(),before);
 let mut renamed=before.clone();renamed["compositions"][&project.root]["content"]["tracks"][2]["id"]=json!("native-audio-lane");assert!(export_with_paths(&serde_json::from_value(renamed).unwrap(),&paths).is_ok());
 for mode in ["solo","derived"]{let mut changed=before.clone();let track=&mut changed["compositions"][&project.root]["content"]["tracks"][2];if mode=="solo"{track["solo"]=json!(true);}else{track["clips"][0]["dspEffects"][0]["concatSourceKeys"]["delay"][0]["derived"]=json!(true);}assert!(export_with_paths(&serde_json::from_value(changed).unwrap(),&paths).is_err(),"{mode}");}
 let mut changed=before.clone();changed["compositions"][&project.root]["content"]["tracks"][2]["clips"][0]["pan"]=json!({"mode":"constant","value":0.5});let changed=serde_json::from_value(changed).unwrap();assert!(export_with_paths(&changed,&paths).is_err());
}

#[test]
fn caption_lane_exports_editable_concat_titles_with_bottom_anchor() {
    let mut project = serde_json::to_value(convert(source())).unwrap();
    let root = project["root"].as_str().unwrap().to_owned();
    let content = &mut project["compositions"][root]["content"];
    let caption_lane = content["tracks"][0]["id"].clone();
    content["metadata"]["captionTextTracks"] = json!([caption_lane]);
    let clip = &mut content["tracks"][0]["clips"][0];
    assert!(clip.get("concatTitle").is_some());
    clip.as_object_mut().unwrap().remove("concatTitle");
    clip["timelineStartTick"] = json!(1_234_567);
    clip["timelineEndTick"] = json!(2_345_678);
    clip["text"] = json!("First line\nSecond line");
    clip["textAnchor"] = json!([0.5, 1.0]);
    clip["textBoxWidth"] = json!(144);
    clip["transform"] = serde_json::to_value(video_kadr_backend::domain::composition::TransformSpec::default()).unwrap();
    clip["transform"]["y"]["value"] = json!(54);
    clip["style"]["fontSize"] = json!(12);
    clip["style"]["align"] = json!("center");
    clip["style"]["bold"] = json!(true);
    clip["style"]["italic"] = json!(true);
    clip["style"]["strokeWidth"] = json!(1);
    clip["style"]["color"] = json!({"red":1,"green":0.5,"blue":0,"alpha":1});
    clip["opacity"] = json!({
        "mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[
            {"tick":0,"value":0},
            {"tick":100000,"value":1},
            {"tick":800000,"value":1},
            {"tick":800001,"value":0.5},
            {"tick":1111111,"value":0}
        ]}
    });
    let project = serde_json::from_value(project).unwrap();
    let converted = export(&project).unwrap();
    let title = converted["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(title["kind"], "text");
    assert_eq!(title["start"], 1.234567);
    assert_eq!(title["duration"], 1.111111);
    assert_eq!(title["text"]["content"], "First line\nSecond line");
    assert_eq!(title["text"]["fontSize"], 0.1);
    assert_eq!(title["text"]["fontWeight"].as_f64(), Some(700.0));
    assert_eq!(title["text"]["italic"], true);
    assert_eq!(title["text"]["color"], "#ff8000ff");
    assert_eq!(title["text"]["maxWidth"], 0.9);
    assert!(title["offsetY"].as_f64().unwrap() < 54.0 / 120.0);

    assert_eq!(title["keys"].as_array().unwrap().len(), 5);
    assert!(title["keys"].as_array().unwrap().iter().all(|key| key["property"] == "opacity"));
    let restored = convert(converted);
    let content = &restored.compositions[&restored.root].content;
    let opacity = content.tracks.iter().find_map(|track| match track {
        video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } =>
            clips.iter().find(|clip| clip.id.as_str() == "words")
                .and_then(|clip| clip.transform.concat_animation.as_ref()),
        _ => None,
    }).expect("editable opacity keys after reimport");
    for (time, expected) in [(0, 0.0), (50000, 0.5), (200000, 1.0), (800000, 1.0), (800001, 0.5), (1111111, 0.0)] {
        assert!((opacity.sample(time).unwrap().opacity - expected).abs() < 1e-10, "opacity at {time}");
    }
    let pixels = frame(content, 1_500_000);
    let orange = pixels.chunks_exact(4).enumerate().filter(|(_, rgba)| {
        rgba[0] > 160 && rgba[1] > 65 && rgba[1] < 180 && rgba[2] < 12
    }).map(|(pixel, _)| pixel / 160).collect::<Vec<_>>();
    assert!(!orange.is_empty(), "caption must be visible after reopening");
    assert!(orange.iter().all(|row| (70..=117).contains(row)), "caption must remain near the bottom");

    let mut nonlinear = serde_json::to_value(&project).unwrap();
    let root = nonlinear["root"].as_str().unwrap().to_owned();
    let track = &mut nonlinear["compositions"][root]["content"]["tracks"][0]["clips"][0]["opacity"]["track"];
    track["interpolation"] = json!("ease_in_out_cubic");
    track["keyframes"] = json!([
        {"tick":0,"value":0.1,"interpolation":"ease_in"},
        {"tick":300001,"value":0.9,"interpolation":"ease_out"},
        {"tick":600000,"value":0.2},
        {"tick":1111111,"value":0.8}
    ]);
    let native: video_kadr_backend::domain::keyframes::KeyframeTrack<f64> =
        serde_json::from_value(track.clone()).unwrap();
    let nonlinear = serde_json::from_value(nonlinear).unwrap();
    let exported = export(&nonlinear).unwrap();
    let title = exported["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(title["keys"].as_array().unwrap().len(), 5);
    let reopened = convert(exported);
    let animation = reopened.compositions[&reopened.root].content.tracks.iter().find_map(|track| match track {
        video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } =>
            clips.iter().find(|clip| clip.id.as_str() == "words")
                .and_then(|clip| clip.transform.concat_animation.as_ref()),
        _ => None,
    }).unwrap();
    for tick in (0..=1_111_111).step_by(997).chain([300001, 600000, 855555, 855556, 1111111]) {
        let expected = native.sample_tick(tick);
        let actual = animation.sample(tick as u64).unwrap().opacity;
        assert!((actual - expected).abs() < 1e-10, "cubic opacity at {tick}: {actual} vs {expected}");
    }

    let mut bounce = serde_json::to_value(&project).unwrap();
    let root = bounce["root"].as_str().unwrap().to_owned();
    let track = &mut bounce["compositions"][root]["content"]["tracks"][0]["clips"][0]["opacity"]["track"];
    track["interpolation"] = json!("bounce_in_out");
    track["keyframes"] = json!([
        {"tick":0,"value":0.0,"interpolation":"bounce_in"},
        {"tick":300001,"value":1.0,"interpolation":"bounce_out"},
        {"tick":600000,"value":0.0},
        {"tick":1111111,"value":1.0}
    ]);
    let native: video_kadr_backend::domain::keyframes::KeyframeTrack<f64> =
        serde_json::from_value(track.clone()).unwrap();
    let restored = convert(export(&serde_json::from_value(bounce).unwrap()).unwrap());
    let animation = restored.compositions[&restored.root].content.tracks.iter().find_map(|track| match track {
        video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } =>
            clips.iter().find(|clip| clip.id.as_str() == "words")
                .and_then(|clip| clip.transform.concat_animation.as_ref()),
        _ => None,
    }).unwrap();
    assert_eq!(animation.keys["opacity"].len(), 29);
    for tick in (0..=1_111_111).step_by(251).chain([300000, 300001, 600000, 855555, 855556, 1111111]) {
        let expected = native.sample_tick(tick);
        let actual = animation.sample(tick as u64).unwrap().opacity;
        assert!((actual - expected).abs() < 1e-10, "bounce opacity at {tick}: {actual} vs {expected}");
    }

    let mut hold = serde_json::to_value(&project).unwrap();
    let root = hold["root"].as_str().unwrap().to_owned();
    let track = &mut hold["compositions"][root]["content"]["tracks"][0]["clips"][0]["opacity"]["track"];
    track["interpolation"] = json!("hold");
    track["keyframes"] = json!([
        {"tick":0,"value":0.0},
        {"tick":1,"value":1.0},
        {"tick":2,"value":0.25},
        {"tick":300001,"value":0.25,"interpolation":"linear"},
        {"tick":600000,"value":0.75},
        {"tick":1111111,"value":0.0}
    ]);
    let native: video_kadr_backend::domain::keyframes::KeyframeTrack<f64> =
        serde_json::from_value(track.clone()).unwrap();
    let converted = export(&serde_json::from_value(hold).unwrap()).unwrap();
    let encoded = serde_json::to_vec(&converted).unwrap();
    let reopened = convert(serde_json::from_slice(&encoded).unwrap());
    let animation = reopened.compositions[&reopened.root].content.tracks.iter().find_map(|track| match track {
        video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } =>
            clips.iter().find(|clip| clip.id.as_str() == "words")
                .and_then(|clip| clip.transform.concat_animation.as_ref()),
        _ => None,
    }).unwrap();
    assert_eq!(animation.keys["opacity"].len(), 12);
    for tick in (0..=1_111_111).step_by(997).chain([0, 1, 2, 3, 300000, 300001, 599999, 600000, 1111110, 1111111]) {
        let expected = native.sample_tick(tick);
        let actual = animation.sample(tick as u64).unwrap().opacity;
        assert!((actual - expected).abs() < 1e-10, "hold opacity at {tick}: {actual} vs {expected}");
    }

    let mut shadowed = serde_json::to_value(&project).unwrap();
    let root = shadowed["root"].as_str().unwrap().to_owned();
    let style = &mut shadowed["compositions"][root]["content"]["tracks"][0]["clips"][0]["style"];
    style["shadow"] = json!({"red":0.2,"green":0.4,"blue":0.8,"alpha":0.5});
    style["shadowX"] = json!(4);
    style["shadowY"] = json!(-2);
    let shadowed = export(&serde_json::from_value(shadowed).unwrap()).unwrap();
    let title = shadowed["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(title["text"]["shadow"], true);
    let unshadowed = export(&project).unwrap();
    let title = unshadowed["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(title["text"]["shadow"], false);
    let shadowed = convert(shadowed);
    let unshadowed = convert(unshadowed);
    assert_ne!(
        frame(&shadowed.compositions[&shadowed.root].content, 1_500_000),
        frame(&unshadowed.compositions[&unshadowed.root].content, 1_500_000),
        "Concat's standard shadow must be rendered after reopening",
    );

    let mut faded = serde_json::to_value(&project).unwrap();
    let root = faded["root"].as_str().unwrap().to_owned();
    faded["compositions"][&root]["content"]["tracks"][0]["clips"][0]["opacityMultiplier"] = json!(0.25);
    let faded: video_kadr_backend::domain::nested_composition::NestedCompositionProject =
        serde_json::from_value(faded).unwrap();
    let faded_output = export(&faded).unwrap();
    let faded_title = faded_output["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(faded_title["text"]["opacity"], 0.25);
    assert_eq!(faded_title["keys"][1]["value"], 1.0);
    assert_ne!(frame(&faded.compositions[&root].content, 1_500_000), frame(&project.compositions[&root].content, 1_500_000));

    let mut scaled = serde_json::to_value(&project).unwrap();
    let root = scaled["root"].as_str().unwrap().to_owned();
    let caption = &mut scaled["compositions"][root]["content"]["tracks"][0]["clips"][0];
    caption["textAnchor"] = json!([0.5,0.5]);
    caption["transform"]["scaleX"]["value"] = json!(2.0);
    caption["transform"]["scaleY"]["value"] = json!(0.75);
    caption["transform"]["rotationDegrees"]["value"] = json!(25.0);
    let source = export(&serde_json::from_value(scaled).unwrap()).unwrap();
    let title = source["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(title["stretchX"], 2.0);
    assert_eq!(title["stretchY"], 0.75);
    assert_eq!(title["rotation"], 25.0);
    assert!(title["keys"].as_array().unwrap().iter().all(|key| key["property"] == "opacity"));

    for (sx, sy, opacity_keys) in [(80.0, 0.8, true), (0.005, 0.5, false)] {
        let mut extreme = serde_json::to_value(&project).unwrap();
        let root = extreme["root"].as_str().unwrap().to_owned();
        let caption = &mut extreme["compositions"][root]["content"]["tracks"][0]["clips"][0];
        caption["textAnchor"] = json!([0.5,0.5]);
        caption["transform"]["scaleX"]["value"] = json!(sx);
        caption["transform"]["scaleY"]["value"] = json!(sy);
        if !opacity_keys {
            caption["opacity"] = json!({"mode":"constant","value":0.35});
        }
        let source = export(&serde_json::from_value(extreme).unwrap()).unwrap();
        let title = source["timelines"][0]["clips"].as_array().unwrap().iter()
            .find(|clip| clip["id"] == "words").unwrap();
        let scale = title["scale"].as_f64().unwrap();
        assert!((scale * title["stretchX"].as_f64().unwrap() - sx).abs() < 1e-12);
        assert!((scale * title["stretchY"].as_f64().unwrap() - sy).abs() < 1e-12);
        assert_eq!(!title["keys"].as_array().unwrap().is_empty(), opacity_keys);
        if !opacity_keys {
            assert_eq!(title["opacity"], 0.35);
        }
        convert(source);
    }

    let mut reexport = serde_json::to_value(&project).unwrap();
    let root = reexport["root"].as_str().unwrap().to_owned();
    let caption = &mut reexport["compositions"][&root]["content"]["tracks"][0]["clips"][0];
    caption["opacity"] = json!({"mode":"constant","value":1});
    caption["transform"]["concatAnimation"] = json!({
        "durationTicks":1111111,"keys":{"opacity":[{"at":0,"value":0.25},{"at":1,"value":0.9}],"offsetY":[{"at":0,"value":0.45},{"at":1,"value":0.55}]},
        "rest":{"scaleX":1,"scaleY":1,"x":0,"y":0.45,"rotationDegrees":0,"opacity":1},
        "stretch":[1,1],"canvas":[160,120]
    });
    let mut pivoted = reexport.clone();
    let pivoted_caption = &mut pivoted["compositions"][&root]["content"]["tracks"][0]["clips"][0];
    pivoted_caption["transform"]["concatAnimation"]["keys"]["scale"] = json!([{"at":0,"value":1.25}]);
    pivoted_caption["transform"]["concatAnimation"]["keys"]["rotation"] = json!([{"at":1,"value":30}]);
    assert!(export(&serde_json::from_value(pivoted.clone()).unwrap()).is_err());
    pivoted["compositions"][&root]["content"]["tracks"][0]["clips"][0]["textAnchor"] = json!([0.5,0.5]);
    let pivoted = convert(export(&serde_json::from_value(pivoted).unwrap()).unwrap());
    let pose = pivoted.compositions[&pivoted.root].content.tracks.iter().find_map(|track| match track {
        video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } => {
            clips.iter().find(|clip| clip.id.as_str() == "words")
                .and_then(|clip| clip.transform.concat_animation.as_ref())
        }
        _ => None,
    }).unwrap().sample(500_000).unwrap();
    assert_eq!((pose.scale_x, pose.scale_y, pose.rotation_degrees), (1.25, 1.25, 30.0));

    let reexport = export(&serde_json::from_value(reexport).unwrap()).unwrap();
    let title = reexport["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert!(title["offsetY"].as_f64().unwrap() < 0.45);
    let keys = title["keys"].as_array().unwrap();
    let position = keys.iter().filter(|key| key["property"] == "offsetY").collect::<Vec<_>>();
    assert_eq!(position[0]["value"], title["offsetY"]);
    assert!((position[1]["value"].as_f64().unwrap() - position[0]["value"].as_f64().unwrap() - 0.1).abs() < 1e-10);
    let opacity = keys.iter().filter(|key| key["property"] == "opacity").collect::<Vec<_>>();
    assert_eq!(opacity[0]["value"], 0.25);
    assert_eq!(opacity[1]["value"], 0.9);

    let mut bezier = serde_json::to_value(&project).unwrap();
    let root = bezier["root"].as_str().unwrap().to_owned();
    let track = &mut bezier["compositions"][root]["content"]["tracks"][0]["clips"][0]["opacity"]["track"];
    track["interpolation"] = json!({"kind":"bezier","p1":[0.2,0.4],"p2":[0.8,0.6]});
    track["keyframes"] = json!([
        {"tick":0,"value":0.1,"tangents":{"incoming":{"x":0.8,"y":0.6,"mode":"free"},"outgoing":{"x":0.3,"y":-0.2,"mode":"free"},"continuity":"broken"}},
        {"tick":500001,"value":0.9,"tangents":{"incoming":{"x":0.7,"y":1.2,"mode":"free"},"outgoing":{"x":0.4,"y":0.2,"mode":"free"},"continuity":"broken"}},
        {"tick":1111111,"value":0.3}
    ]);
    let native: video_kadr_backend::domain::keyframes::KeyframeTrack<f64> =
        serde_json::from_value(track.clone()).unwrap();
    let exported = export(&serde_json::from_value(bezier).unwrap()).unwrap();
    let title = exported["timelines"][0]["clips"].as_array().unwrap().iter()
        .find(|clip| clip["id"] == "words").unwrap();
    assert_eq!(title["keys"][1]["ease"], json!([0.3, -0.2, 0.7, 1.2]));
    assert_eq!(title["keys"][2]["ease"], json!([0.4, 0.2, 0.8, 0.6]));
    let reopened = convert(exported);
    let animation = reopened.compositions[&reopened.root].content.tracks.iter().find_map(|track| match track {
        video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } =>
            clips.iter().find(|clip| clip.id.as_str() == "words")
                .and_then(|clip| clip.transform.concat_animation.as_ref()),
        _ => None,
    }).unwrap();
    for tick in (0..=1_111_111).step_by(337).chain([500000, 500001, 500002, 1111111]) {
        let expected = native.sample_tick(tick).clamp(0.0, 1.0);
        let actual = animation.sample(tick as u64).unwrap().opacity;
        assert!((actual - expected).abs() < 2e-6, "Bezier opacity at {tick}: {actual} vs {expected}");
    }

    for direction in ["in", "out", "in_out"] {
        let mut elastic = serde_json::to_value(&project).unwrap();
        let root = elastic["root"].as_str().unwrap().to_owned();
        let track = &mut elastic["compositions"][root]["content"]["tracks"][0]["clips"][0]["opacity"]["track"];
        track["interpolation"] = json!({"kind":"elastic","dir":direction,"amplitude":1.4,"period":0.3});
        track["keyframes"] = json!([{"tick":0,"value":0.1},{"tick":1111111,"value":0.95}]);
        let native: video_kadr_backend::domain::keyframes::KeyframeTrack<f64> =
            serde_json::from_value(track.clone()).unwrap();
        let restored = convert(export(&serde_json::from_value(elastic).unwrap()).unwrap());
        let animation = restored.compositions[&restored.root].content.tracks.iter().find_map(|track| match track {
            video_kadr_backend::domain::composition::CompositionTrack::Text { clips, .. } =>
                clips.iter().find(|clip| clip.id.as_str() == "words")
                    .and_then(|clip| clip.transform.concat_animation.as_ref()),
            _ => None,
        }).unwrap();
        for tick in (0..=1_111_111).step_by(277).chain([1, 555555, 555556, 1111110, 1111111]) {
            let expected = native.sample_tick(tick).clamp(0.0, 1.0);
            let actual = animation.sample(tick as u64).unwrap().opacity;
            assert!((actual - expected).abs() <= 0.000100001,
                "Elastic {direction} opacity at {tick}: {actual} vs {expected}");
        }
    }

    let mut unsupported = serde_json::to_value(project).unwrap();
    let root = unsupported["root"].as_str().unwrap().to_owned();
    unsupported["compositions"][root]["content"]["tracks"][0]["clips"][0]["opacity"]["track"]["interpolation"] = json!({"kind":"elastic","dir":"out","amplitude":1,"period":0.000001});
    let error = export(&serde_json::from_value(unsupported).unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("Elastic caption curve exceeds the Concat key budget"));
}

#[test]
fn source_speed_curve_roundtrips_and_rebases_with_new_media() {
    use std::collections::BTreeMap;
    use video_kadr_backend::domain::{composition::{CompositionSource, SourceId},
        concat_project_export::export_with_paths, project_media_fill::fill};
    for (kind, width, height) in [("video", 16, 8), ("audio", 0, 0)] {
        let curve = json!([{"at":0.0,"speed":0.5},{"at":1.0,"speed":1.5}]);
        let document = json!({
            "version":1,"media":[{"id":"slot","kind":kind,"placeholder":true}],
            "timelines":[{"id":"main","video":{"width":16,"height":8,"rateNum":24,"rateDen":1},
              "tracks":[{"id":"video"}],"clips":[{"id":"authored","trackId":"video","mediaId":"slot",
                "kind":kind,"start":0.125,"sourceStart":0.25,"duration":0.5,"speed":1,
                "muted":true,"speedCurve":curve}]}]
        });
        let binding = json!({"id":"slot","kind":kind,"durationTicks":1000000,
            "width":width,"height":height,"hasAudio":true});
        let convert = |document| import(serde_json::from_value(json!({
            "schemaVersion":1,"document":document,"mediaBindings":{"slot":binding},"preserveMediaIds":true
        })).unwrap()).unwrap();
        let result = convert(document.clone());
        assert_eq!(result.status, "converted", "{:?}", result.diagnostics);
        let project = result.project.unwrap();
        let paths = BTreeMap::from([("slot".into(), String::new())]);
        let output = export_with_paths(&project, &paths).unwrap();
        let clip = &output["timelines"][0]["clips"][0];
        assert_eq!(clip["sourceStart"], 0.25);
        assert_eq!(clip["duration"], 0.5);
        assert_eq!(clip["speedCurve"], curve);
        let replacement: CompositionSource = serde_json::from_value(json!({"id":"new","kind":kind,
            "durationTicks":250000,"width":width,"height":height,"hasAudio":true})).unwrap();
        let filled = fill(&project, &SourceId::parse("slot").unwrap(), &json!({
            "path":"library:new","kind":kind,"name":"Replacement","duration":0.25,"width":width,
            "height":height,"hasAudio":true
        }), &replacement).unwrap();
        let value = serde_json::to_value(&filled).unwrap();
        let placement = &value["compositions"][&filled.root]["content"]["tracks"][0]["clips"][0]["placement"];
        assert_eq!(placement["sourceInTick"], 0);
        assert_eq!(placement["sourceOutTick"], 500_000);
        assert_eq!(placement["concatSourceEof"], true);
        assert_eq!(placement["concatRetime"]["documentClock"]["sourceStart"], 0.0);
        assert_eq!(placement["concatRetime"]["documentClock"]["speedCurve"], curve);
        let (other_kind, other_width, other_height) = if kind == "video" {
            ("audio", 0, 0)
        } else {
            ("video", 16, 8)
        };
        let cross_source: CompositionSource = serde_json::from_value(json!({
            "id":"other","kind":other_kind,"durationTicks":250000,
            "width":other_width,"height":other_height,"hasAudio":true
        })).unwrap();
        let crossed = fill(&project, &SourceId::parse("slot").unwrap(), &json!({
            "path":"library:other","kind":other_kind,"name":"Cross replacement","duration":0.25,
            "width":other_width,"height":other_height,"hasAudio":true
        }), &cross_source).unwrap();
        let value = serde_json::to_value(&crossed).unwrap();
        let lane = &value["compositions"][&crossed.root]["content"]["tracks"][0];
        assert_eq!(lane["kind"], other_kind);
        let placement = &lane["clips"][0]["placement"];
        assert_eq!(placement["timelineStartTick"], 125_000);
        assert_eq!(placement["timelineDurationTick"], 500_000);
        assert_eq!(placement["sourceInTick"], 0);
        assert_eq!(placement["sourceOutTick"], 500_000);
        assert_eq!(placement["concatRetime"]["documentClock"]["speedCurve"], curve);
        let mut audible = document;
        audible["timelines"][0]["clips"][0]["muted"] = json!(false);
        let audible = convert(audible);
        assert_eq!(audible.status, "converted", "{:?}", audible.diagnostics);
        let moving = audible.project.unwrap();
        let output = export_with_paths(&moving, &paths).unwrap();
        assert_eq!(output["timelines"][0]["clips"][0]["muted"], false);
        assert_eq!(output["timelines"][0]["clips"][0]["speedCurve"], curve);

        let mut cleared = serde_json::to_value(&moving).unwrap();
        let placement = &mut cleared["compositions"][&moving.root]["content"]["tracks"][0]["clips"][0]["placement"];
        let clock: video_kadr_backend::domain::concat_retime_edit::Clock =
            serde_json::from_value(placement["concatRetime"]["documentClock"].clone()).unwrap();
        *placement = serde_json::to_value(clock.set_curve(None).unwrap().placement(24, 1).unwrap()).unwrap();
        let cleared = serde_json::from_value(cleared).unwrap();
        let output = export_with_paths(&cleared, &paths).unwrap();
        assert_eq!(output["timelines"][0]["clips"][0]["speedCurve"], json!([]));
        let mut original_resave = output.clone();
        let clip = original_resave["timelines"][0]["clips"][0].as_object_mut().unwrap();
        clip.remove("speedCurve");
        clip.insert("sourceStart".into(), json!(0.2500004));
        let resaved = convert(original_resave);
        assert_eq!(resaved.status, "converted", "{:?}", resaved.diagnostics);
        let resaved = resaved.project.unwrap();
        let reexported = export_with_paths(&resaved, &paths).unwrap();
        assert_eq!(reexported["timelines"][0]["clips"][0]["sourceStart"], json!(0.2500004));
        assert_eq!(reexported["timelines"][0]["clips"][0]["speedCurve"], json!([]));
        let reopened = convert(output);
        assert_eq!(reopened.status, "converted", "{:?}", reopened.diagnostics);
        let reopened = reopened.project.unwrap();
        let native = serde_json::to_value(&reopened).unwrap();
        let clock = &native["compositions"][&reopened.root]["content"]["tracks"][0]["clips"][0]
            ["placement"]["concatRetime"]["documentClock"];
        assert!(clock["speedCurve"].is_null());
        assert_eq!(clock["sourceStart"], 0.25);

        let image: CompositionSource = serde_json::from_value(json!({
            "id":"picture","kind":"image","durationTicks":0,"width":16,"height":8,"hasAudio":false
        })).unwrap();
        let slot = SourceId::parse("slot").unwrap();
        for (project, portable_curve, native_curve) in [
            (&moving, &curve, &curve),
            (&cleared, &json!([]), &serde_json::Value::Null),
        ] {
            let still = fill(project, &slot, &json!({
                "path":"library:picture","kind":"image","name":"Still","width":16,"height":8,"hasAudio":false
            }), &image).unwrap();
            let native = serde_json::to_value(&still).unwrap();
            let clip = &native["compositions"][&still.root]["content"]["tracks"][0]["clips"][0];
            assert_eq!(clip["concatSourceTiming"]["speedCurve"], *portable_curve);
            assert_eq!(clip["timelineStartTick"], 125_000);
            assert_eq!(clip["durationTicks"], 500_000);
            let (marked, _) = video_kadr_backend::domain::project_media::set_placeholder(&still, &slot, true).unwrap();
            let portable = export_with_paths(&marked, &paths).unwrap();
            assert_eq!(portable["timelines"][0]["clips"][0]["speedCurve"], *portable_curve);
            let reopened = import(serde_json::from_value(json!({
                "schemaVersion":1,"document":portable,"mediaBindings":{"slot":image},"preserveMediaIds":true
            })).unwrap()).unwrap();
            assert_eq!(reopened.status, "converted", "{:?}", reopened.diagnostics);
            let refilled = fill(&reopened.project.unwrap(), &slot, &json!({
                "path":"library:new","kind":kind,"name":"Replacement","duration":0.25,
                "width":width,"height":height,"hasAudio":true
            }), &replacement).unwrap();
            let native = serde_json::to_value(&refilled).unwrap();
            let lane = &native["compositions"][&refilled.root]["content"]["tracks"][0];
            assert_eq!(lane["kind"], kind);
            let placement = &lane["clips"][0]["placement"];
            assert_eq!(placement["sourceInTick"], 0);
            assert_eq!(placement["timelineDurationTick"], 500_000);
            let clock = &placement["concatRetime"]["documentClock"];
            assert!(clock.is_object());
            assert_eq!(clock["speedCurve"], *native_curve);
            assert_eq!(clock["sourceStart"], 0.0);
        }
    }
}

#[test]
fn native_video_export_refusal_names_the_unrepresentable_clip_fields() {
    use std::collections::BTreeMap;
    let report = import(serde_json::from_value(json!({
        "schemaVersion":1,"document":{"version":1,"media":[{"id":"slot","kind":"video","placeholder":true}],
            "timelines":[{"id":"main","video":{"width":16,"height":8,"rateNum":24,"rateDen":1},
                "tracks":[{"id":"video"}],
                "clips":[{"id":"authored","trackId":"video","mediaId":"slot","kind":"video","duration":1}]}]},
        "mediaBindings":{"slot":{"id":"slot","kind":"video","durationTicks":1000000,
            "width":16,"height":8,"hasAudio":false}},
        "preserveMediaIds":true
    })).unwrap()).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let imported = report.project.unwrap();
    let mut wire = serde_json::to_value(&imported).unwrap();
    let mut trimmed = wire.clone();
    let trimmed_node = &mut trimmed["compositions"][&imported.root];
    trimmed_node["durationUs"] = json!(750_000);
    let trimmed_clip = &mut trimmed_node["content"]["tracks"][0]["clips"][0];
    trimmed_clip["placement"]["timelineDurationTick"] = json!(750_000);
    trimmed_clip["concatGainKeys"] = json!([
        {"at":0.0,"value":0.25},
        {"at":1.0,"value":0.75,"ease":[0.2,0.1,0.8,0.9],"derived":true}
    ]);
    let trimmed_project = serde_json::from_value(trimmed.clone()).unwrap();
    let paths = BTreeMap::from([("slot".into(), String::new())]);
    let output = video_kadr_backend::domain::concat_project_export::export_with_paths(
        &trimmed_project, &paths,
    ).unwrap();
    let clip = &output["timelines"][0]["clips"][0];
    assert_eq!(clip["duration"], json!(0.75));
    assert_eq!(clip["keys"][1], json!({"property":"volume","at":1.0,"value":0.75,"ease":[0.2,0.1,0.8,0.9]}));
    assert_eq!(serde_json::to_value(trimmed_project).unwrap()["compositions"][&imported.root]
        ["content"]["tracks"][0]["clips"][0]["placement"]["sourceOutTick"], 1_000_000);

    let mut head_trimmed = trimmed.clone();
    let node = &mut head_trimmed["compositions"][&imported.root];
    node["durationUs"] = json!(875_000);
    let placement = &mut node["content"]["tracks"][0]["clips"][0]["placement"];
    placement["timelineStartTick"] = json!(125_000);
    placement["sourceTimelineOffsetTick"] = json!(125_000);
    let output = video_kadr_backend::domain::concat_project_export::export_with_paths(
        &serde_json::from_value(head_trimmed).unwrap(), &paths,
    ).unwrap();
    let clip = &output["timelines"][0]["clips"][0];
    assert_eq!(clip["start"], json!(0.125));
    assert_eq!(clip["sourceStart"], json!(0.125));
    assert_eq!(clip["duration"], json!(0.75));

    let mut extended = trimmed;
    let node = &mut extended["compositions"][&imported.root];
    node["durationUs"] = json!(875_000);
    let placement = &mut node["content"]["tracks"][0]["clips"][0]["placement"];
    placement["timelineStartTick"] = json!(125_000);
    placement["sourceInTick"] = json!(250_000);
    placement["sourceTimelineOffsetTick"] = json!(-125_000);
    placement["sourceHeadTimelineTick"] = json!(125_000);
    let output = video_kadr_backend::domain::concat_project_export::export_with_paths(
        &serde_json::from_value(extended).unwrap(), &paths,
    ).unwrap();
    let clip = &output["timelines"][0]["clips"][0];
    assert_eq!(clip["start"], json!(0.125));
    assert_eq!(clip["sourceStart"], json!(0.125));
    assert_eq!(clip["duration"], json!(0.75));

    for (speed, expected_in) in [(0.5, 0.25), (0.75, 0.275), (1.25, 0.325), (1.5, 0.35), (2.0, 0.4)] {
        let mut retimed = wire.clone();
        let node = &mut retimed["compositions"][&imported.root];
        node["durationUs"] = json!(300_000);
        let clip = &mut node["content"]["tracks"][0]["clips"][0];
        clip["sourceAudioEnabled"] = json!(false);
        clip["placement"] = json!({"timelineStartTick":100_000,"timelineDurationTick":200_000,
            "sourceInTick":200_000,"sourceOutTick":1_000_000,"sourceTimelineOffsetTick":100_000,"speed":speed});
        let output = video_kadr_backend::domain::concat_project_export::export_with_paths(
            &serde_json::from_value(retimed.clone()).unwrap(), &paths,
        ).unwrap();
        let clip = &output["timelines"][0]["clips"][0];
        assert_eq!(clip["sourceStart"], expected_in);
        assert_eq!(clip["duration"], 0.2);
        assert_eq!(clip["speed"], speed);
        assert_eq!(clip["muted"], true);

        if speed == 0.5 {
            let mut unaligned = retimed.clone();
            unaligned["compositions"][&imported.root]["content"]["tracks"][0]["clips"][0]
                ["placement"]["sourceTimelineOffsetTick"] = json!(100_001);
            assert!(video_kadr_backend::domain::concat_project_export::export_with_paths(
                &serde_json::from_value(unaligned).unwrap(), &paths,
            ).is_err());
        }
        retimed["compositions"][&imported.root]["content"]["tracks"][0]["clips"][0]["sourceAudioEnabled"] = json!(true);
        assert!(video_kadr_backend::domain::concat_project_export::export_with_paths(
            &serde_json::from_value(retimed).unwrap(), &paths,
        ).is_err());
    }

    let mut inexact = wire.clone();
    let node = &mut inexact["compositions"][&imported.root];
    node["durationUs"] = json!(300_000);
    let clip = &mut node["content"]["tracks"][0]["clips"][0];
    clip["sourceAudioEnabled"] = json!(false);
    clip["placement"] = json!({"timelineStartTick":100_000,"timelineDurationTick":200_000,
        "sourceInTick":200_000,"sourceOutTick":1_000_000,"sourceTimelineOffsetTick":100_000,"speed":1.1});
    assert!(video_kadr_backend::domain::concat_project_export::export_with_paths(
        &serde_json::from_value(inexact).unwrap(), &paths,
    ).is_err());

    let clip = &mut wire["compositions"][&imported.root]["content"]["tracks"][0]["clips"][0];
    clip["transform"].as_object_mut().unwrap().remove("concatPreparation");
    clip.as_object_mut().unwrap().remove("independentAudio");
    let project = serde_json::from_value(wire).unwrap();
    let error = video_kadr_backend::domain::concat_project_export::export_with_paths(
        &project, &BTreeMap::from([("slot".into(), String::new())]),
    ).unwrap_err().to_string();
    assert!(error.contains("authored"), "{error}");
    assert!(error.contains("transform.concatPreparation"), "{error}");
    assert!(error.contains("independentAudio"), "{error}");
}
