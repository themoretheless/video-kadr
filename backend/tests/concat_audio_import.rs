use serde_json::json;
use video_kadr_backend::domain::{
    concat_audio_import::convert, concat_effect_upgrade::AppliedFilter,
};
fn link(value: serde_json::Value) -> AppliedFilter {
    serde_json::from_value(value).unwrap()
}
#[test]
fn preserves_original_aliases_order_defaults_and_bypass() {
    let chain = convert(&[
        link(json!({"id":"echo"})),
        link(json!({"id":"hall","enabled":false,"params":{"size":75}})),
        link(json!({"id":"concat.pitch","params":{"semitones":7}})),
    ])
    .unwrap();
    assert_eq!(
        chain.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["concat.echo", "concat.hall", "concat.pitch"]
    );
    assert_eq!(chain[0].params["delay"], 0.25);
    assert_eq!(chain[0].params["decay"], 0.4);
    assert!(chain[1].bypass);
    assert_eq!(chain[1].params["size"], 75.);
    assert_eq!(chain[2].params["semitones"], 7.);
    for id in [
        "concat.bass",
        "concat.bright",
        "concat.cathedral",
        "concat.chipmunk",
        "concat.deep",
        "concat.denoise",
        "concat.distorted",
        "concat.echo",
        "concat.enhance-voice",
        "concat.hall",
        "concat.lofi",
        "concat.loudness",
        "concat.megaphone",
        "concat.pitch",
        "concat.plate",
        "concat.radio",
        "concat.robot",
        "concat.room",
        "concat.stadium",
        "concat.sweet",
        "concat.telephone",
        "concat.treble",
        "concat.underwater",
        "concat.vinyl",
        "concat.whisper",
    ] {
        let specs = convert(&[link(json!({"id":id}))]).unwrap();
        assert_eq!(specs[0].id, id);
    }
}
#[test]
fn rejects_bad_parameters_before_f32_narrowing_unknown_keys_and_resource_overflow() {
    for value in [
        json!({"id":"concat.echo","params":{"delay":0.0499999999}}),
        json!({"id":"concat.echo","params":{"delay":1.00000001}}),
        json!({"id":"concat.hall","params":{"size":100.0000001}}),
        json!({"id":"concat.pitch","params":{"semitones":12.0000001}}),
        json!({"id":"concat.hall","params":{"unknown":1}}),
        json!({"id":"concat.exposure"}),
        json!({"id":"concat.echo","keys":{"unknown":[{"at":0,"value":0.25}]}}),
    ] {
        assert!(convert(&[link(value.clone())]).is_err(), "{value}");
    }
    assert!(convert(&vec![link(json!({"id":"concat.echo"})); 17]).is_err());
    assert!(convert(&[link(json!({"id":"concat.echo","params":{"delay":0.05}}))]).is_ok());
}

// Independent pinned package manifest declaration matrix (not a PCM oracle).
const SOURCE_BOUNDS: &str = r#"[["concat.bass","gain",5.0,0.0,12.0],["concat.bright","gain",5.0,0.0,12.0],["concat.cathedral","size",55.0,0.0,100.0],["concat.chipmunk","pitch",7.0,3.0,12.0],["concat.deep","pitch",-3.0,-8.0,-1.0],["concat.deep","body",3.0,0.0,8.0],["concat.denoise","strength",50.0,0.0,100.0],["concat.distorted","drive",50.0,0.0,100.0],["concat.echo","delay",0.25,0.05,1.0],["concat.echo","decay",0.4,0.1,0.9],["concat.enhance-voice","amount",60.0,0.0,100.0],["concat.hall","size",50.0,0.0,100.0],["concat.lofi","bits",8.0,4.0,12.0],["concat.loudness","target",-23.0,-30.0,-12.0],["concat.megaphone","drive",5.0,1.0,10.0],["concat.pitch","semitones",0.0,-12.0,12.0],["concat.plate","size",40.0,0.0,100.0],["concat.radio","noise",30.0,0.0,100.0],["concat.robot","depth",5.0,1.0,10.0],["concat.room","size",40.0,0.0,100.0],["concat.stadium","size",6.0,1.0,10.0],["concat.sweet","amount",65.0,0.0,100.0],["concat.sweet","pitch",0.0,0.0,8.0],["concat.telephone","drive",3.0,0.0,10.0],["concat.treble","gain",4.0,0.0,12.0],["concat.underwater","depth",5.0,1.0,10.0],["concat.vinyl","warmth",5.0,0.0,10.0],["concat.whisper","air",5.0,1.0,10.0]]"#;

#[test]
fn source_manifest_defaults_endpoints_and_sub_f32_rejections() {
    let declarations: Vec<serde_json::Value> = serde_json::from_str(SOURCE_BOUNDS).unwrap();
    for declaration in declarations {
        let id = declaration[0].as_str().unwrap();
        let key = declaration[1].as_str().unwrap();
        let default = declaration[2].as_f64().unwrap();
        let lo = declaration[3].as_f64().unwrap();
        let hi = declaration[4].as_f64().unwrap();
        let specs = convert(&[link(json!({"id":id}))]).unwrap();
        assert_eq!(specs[0].params[key].to_bits(), (default as f32).to_bits());
        for value in [lo, hi] {
            assert!(
                convert(&[link(json!({"id":id,"params":{key:value}}))]).is_ok(),
                "{id}.{key}={value}"
            );
        }
        for value in [lo - 1e-7, hi + 1e-7] {
            assert!(
                convert(&[link(json!({"id":id,"params":{key:value}}))]).is_err(),
                "{id}.{key}={value}"
            );
        }
    }
}

#[test]
fn inactive_source_audio_keys_do_not_replace_static_params(){
 let plain=link(json!({"id":"concat.echo","params":{"delay":0.25,"decay":0.4}}));
 let keyed=link(json!({"id":"concat.echo","params":{"delay":0.25,"decay":0.4},"keys":{"delay":[{"at":0,"value":0.05},{"at":1,"value":1,"ease":[0.42,-0.5,0.58,1]}]}}));
 let plain=convert(&[plain]).unwrap();let mut keyed=convert(&[keyed]).unwrap();assert_eq!(keyed[0].concat_source_keys["delay"].len(),2);let encoded=serde_json::to_value(&keyed[0]).unwrap();let decoded:video_kadr_backend::domain::filmcraft_dsp::EffectSpec=serde_json::from_value(encoded).unwrap();assert_eq!(decoded,keyed[0]);video_kadr_backend::domain::filmcraft_dsp::validate_specs(&[decoded]).unwrap();keyed[0].concat_source_keys.clear();assert_eq!(plain,keyed);
 let too_many=vec![json!({"at":0.5,"value":0.25});4097];assert!(convert(&[link(json!({"id":"echo","keys":{"delay":too_many}}))]).is_err());
 assert!(convert(&[link(json!({"id":"echo","keys":{"delay":[{"at":-0.1,"value":0.25}]}}))]).is_err());
}
