mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{io::Read, path::Path, time::Duration};
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition},
    library::MediaEntry,
};
fn command(args: &[&str]) -> Vec<u8> {
    let out = std::process::Command::new("ffmpeg")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn request(body: &Value, token: Option<&str>) -> Request<Body> {
    let mut r = Request::builder()
        .method("POST")
        .uri("/api/compositions/render")
        .header("content-type", "application/json");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(body.to_string())).unwrap()
}
async fn terminal(app: &axum::Router, id: &str, token: &str) -> Value {
    for _ in 0..3000 {
        let r = Request::builder()
            .uri(format!("/api/jobs/{id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (status, done, _) = support::send(app, r).await;
        assert_eq!(status, StatusCode::OK, "{done}");
        if matches!(
            done["status"].as_str(),
            Some("done" | "error" | "cancelled")
        ) {
            return done;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Echo export did not finish")
}
fn source(frame: usize, ch: usize, frames: usize) -> f32 {
    if [0, 48000 / 7, frames - 1].contains(&frame) {
        return [1.25, -0.75][ch];
    }
    let h = (frame % 257 + ch * 1061) as u32;
    let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
    (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
}
fn decode(path: &Path) -> Vec<f32> {
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-ar",
        "48000",
        "-ac",
        "2",
        "-c:a",
        "pcm_f32le",
        "-f",
        "f32le",
        "pipe:1",
    ])
    .chunks_exact(4)
    .map(|p| f32::from_le_bytes(p.try_into().unwrap()))
    .collect()
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and native PCM export worker"]
async fn private_video_source_imported_echo_wav_export_and_selected_seek() {
    let (mut state, _dir) = support::make_state(true, false).await;
    let caps = std::sync::Arc::make_mut(&mut state.tools);
    caps.ffmpeg_encoders.push("pcm_s16le".into());
    caps.ffmpeg_muxers.push("wav".into());
    let owner = state
        .db
        .register_auth_user("echo-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("echo-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/concat-echo-pcm.json")).unwrap();
    let mut pcm = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/concat-echo-pcm.f32le.gz").as_slice())
        .read_to_end(&mut pcm)
        .unwrap();
    let cases: Vec<&Value> = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["rate"] == 48000 && c["channels"] == 2)
        .collect();
    assert_eq!(cases.len(), 6);
    let frames = cases[0]["inputFrames"].as_u64().unwrap() as usize;
    let input: Vec<u8> = (0..frames)
        .flat_map(|i| (0..2).flat_map(move |ch| source(i, ch, frames).to_le_bytes()))
        .collect();
    let raw = state.sources_dir().join("echo.f32le");
    std::fs::write(&raw, &input).unwrap();
    let wav = state.sources_dir().join("echo.wav");
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-y",
        "-f",
        "f32le",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-i",
        raw.to_str().unwrap(),
        "-c:a",
        "pcm_f32le",
        wav.to_str().unwrap(),
    ]);
    assert_eq!(
        decode(&wav)
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        input
    );
    let av=state.sources_dir().join("echo-av.mov");command(&["-nostdin","-v","error","-y","-f","lavfi","-i","color=c=red:s=16x16:r=4:d=2","-i",wav.to_str().unwrap(),"-map","0:v:0","-map","1:a:0","-c:v","qtrle","-c:a","pcm_f32le","-shortest",av.to_str().unwrap()]);
    state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"echo-av.mov","url":"/files/sources/echo-av.mov","mediaType":"video","width":16,"height":16,"fps":4,"duration":frames as f64/48000.,"hasAudio":true,"audioChannels":2,"acodec":"pcm_f32le"}))).await;
    let space = state
        .db
        .create_space(&owner.user.username, "Echo source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Echo source claim",
            &json!({"schemaVersion":1,"sources":{"source":{"id":"source"}}}),
            &["source".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let (status, catalog, _) =
        support::send(&app, support::get("/api/compositions/audio-dsp-catalog")).await;
    assert_eq!(status, StatusCode::OK);
    let info = catalog["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "concat.echo")
        .unwrap();
    assert_eq!(info["nativeExport"], true);
    assert!(info["params"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["animatable"] == false && p["step"] == 0.05));
    let mut jobs = 0;
    let mut first_export = None;
    for fade in [0.75_f64,2.0] {
    for (pi, case) in cases.iter().enumerate().filter(|(i, _)| [0, 4].contains(i)) {
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources=serde_json::from_value(json!({"source":{"id":"source","kind":"video","durationTicks":1125354,"width":16,"height":16,"hasAudio":true,"audioChannels":2}})).unwrap();
        composition.tracks=serde_json::from_value(json!([{"kind":"audio","id":"audio","name":"Echo","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":case["settings"]}]}])).unwrap();
        let import_body=json!({"schemaVersion":1,"document":{"version":1,"media":[{"id":"m1","kind":"video","path":"/never/open-echo.wav"}],"timelines":[{"id":"TL1","tracks":[{"id":"T1"}],"clips":[{"id":"clip","trackId":"T1","mediaId":"m1","kind":"video","start":1,"duration":1,"fadeIn":fade,"fadeOut":fade,"volume":0.9,"keys":[{"property":"volume","at":0,"value":0.25},{"property":"volume","at":1,"value":1}],"filters":case["settings"]}]}]},"mediaBindings":{"m1":composition.sources.values().next().unwrap()}});
        let import_request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(import_body.to_string())).unwrap();let(status,imported,_)=support::send(&app,import_request).await;assert_eq!(status,StatusCode::OK);assert_eq!(imported["status"],"converted","{imported}");composition=serde_json::from_value(imported["project"]["compositions"]["concat-timeline-0"]["content"].clone()).unwrap();
        composition.validate().unwrap();
        assert!(
            video_kadr_backend::tools::composition_native_audio::needs_native_audio(&composition)
        );
        let body = json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":1000000,"endTicks":2000000}}});
        if pi == 0 {
            let (s, _, _) = support::send(&app, request(&body, None)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED);
            let (s, _, _) = support::send(&app, request(&body, Some(&outsider.token))).await;
            assert_eq!(s, StatusCode::FORBIDDEN);
            let mut bad = body.clone();
            bad["composition"]["tracks"][0]["clips"][0]["dspEffects"][0]["params"] =
                json!({"delay":1.5});
            let (s, _, _) = support::send(&app, request(&bad, Some(&owner.token))).await;
            assert_eq!(s, StatusCode::BAD_REQUEST);
        }
        for (start, end, first, count) in [
            (1000000, 2000000, 0, 48000),
            (1500000, 1750000, 24000, 12000),
            (500000, 750000, 0, 12000),
        ] {
            let mut selected = body.clone();
            selected["output"]["range"] = json!({"startTicks":start,"endTicks":end});
            let (s, accepted, _) =
                support::send(&app, request(&selected, Some(&owner.token))).await;
            assert_eq!(s, StatusCode::ACCEPTED, "{accepted}");
            let done = terminal(&app, accepted["jobId"].as_str().unwrap(), &owner.token).await;
            assert_eq!(done["status"], "done", "{done}");
            let actual = decode(
                &state
                    .outputs_dir()
                    .join(done["result"]["filename"].as_str().unwrap()),
            );
            assert_eq!(actual.len(), count * 2);
            if pi == 0 && start == 1000000 {
                first_export = Some((
                    selected.clone(),
                    accepted["jobId"].as_str().unwrap().to_owned(),
                    actual.clone(),
                ));
            }
            let offset = case["offsetBytes"].as_u64().unwrap() as usize;
            for (i, v) in actual.iter().enumerate() {
                let expected = if start < 1000000 {
                    0.
                } else {
                    let j = offset + (first * 2 + i) * 4;
                    let time = (first + i / 2) as f64 / 48000.;
                    let fade_start=(1.-fade).max(0.);
                    let envelope = (time / fade).min(1.) * (1.-(time-fade_start).max(0.)/fade).clamp(0.,1.);
                    (f32::from_le_bytes(pcm[j..j + 4].try_into().unwrap()) * (envelope*(0.25+0.75*time)) as f32)
                        .clamp(-1., 1. - 1. / 32768.)
                };
                assert!(
                    (v - expected).abs() <= 1. / 32768. + 2e-7,
                    "plan{pi} selected{start} sample{i}: native{v} original{expected}"
                );
            }
            jobs += 1;
        }
    }
    }
    assert_eq!(jobs, 12);
    // The same imported sparse video must deliver its picture and embedded DSP
    // through the public video export path, including the leading empty canvas.
    let mut av_request = first_export.as_ref().unwrap().0.clone();
    av_request["composition"]["canvas"]["width"] = json!(16);
    av_request["composition"]["canvas"]["height"] = json!(16);
    av_request["output"]["profile"] = json!({"container":"mp4","codec":"h264"});
    av_request["output"]["range"] = json!({"startTicks":500000,"endTicks":1750000});
    let (status, accepted, _) = support::send(&app, request(&av_request, Some(&owner.token))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let done = terminal(&app, accepted["jobId"].as_str().unwrap(), &owner.token).await;
    assert_eq!(done["status"], "done", "{done}");
    let movie = state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
    let sound = decode(&movie);
    assert!(sound.len() >= 58000 * 2, "selected MP4 audio was truncated");
    assert!(sound[..20000 * 2].iter().all(|sample|sample.abs()<0.002), "leading gap must remain silent");
    assert!(sound[30000 * 2..].iter().any(|sample|sample.abs()>0.01), "embedded DSP audio missing");
    let pixel_at = |time:&str| command(&["-nostdin","-v","error","-ss",time,"-i",movie.to_str().unwrap(),"-frames:v","1","-vf","scale=1:1","-pix_fmt","rgb24","-f","rawvideo","pipe:1"]);
    let before = pixel_at("0.25"); let during = pixel_at("0.75");
    assert_eq!(before.len(),3);assert_eq!(during.len(),3);
    assert!(before.iter().all(|value|*value<8), "leading canvas: {before:?}");
    assert!(during[0]>200 && during[1]<30 && during[2]<30, "source picture: {during:?}");
    let mut second = av_request["composition"]["tracks"][0]["clips"][0].clone();
    second["id"] = json!("second-video");
    second["audioFadeInTicks"]=json!(250000);second["audioFadeOutTicks"]=json!(250000);
    second["placement"]["timelineStartTick"] = json!(2500000);
    second["placement"]["sourceOutTick"] = json!(500000);
    av_request["composition"]["tracks"][0]["clips"].as_array_mut().unwrap().push(second);
    av_request["output"]["range"]["endTicks"] = json!(3000000);
    let (status, accepted, _) = support::send(&app, request(&av_request, Some(&owner.token))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let done = terminal(&app, accepted["jobId"].as_str().unwrap(), &owner.token).await;
    assert_eq!(done["status"], "done", "{done}");
    let movie = state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
    let sound = decode(&movie);
    assert!(sound.len() >= 118000 * 2, "two-clip MP4 audio was truncated");
    assert!(sound[77000 * 2..86000 * 2].iter().all(|sample|sample.abs()<0.002), "internal gap must remain silent");
    assert!(sound[100000 * 2..].iter().any(|sample|sample.abs()>0.01), "second video audio missing");
    let pixel_at = |time:&str| command(&["-nostdin","-v","error","-ss",time,"-i",movie.to_str().unwrap(),"-frames:v","1","-vf","scale=1:1","-pix_fmt","rgb24","-f","rawvideo","pipe:1"]);
    let between = pixel_at("1.75"); let second_picture = pixel_at("2.25");
    assert_eq!(between.len(),3);assert_eq!(second_picture.len(),3);
    assert!(between.iter().all(|value|*value<8), "internal canvas: {between:?}");
    assert!(second_picture[0]>200 && second_picture[1]<30 && second_picture[2]<30, "second picture: {second_picture:?}");
    if let Ok(path)=std::env::var("CONCAT_LINKED_AUDIO_ROUNDTRIP_INPUT") {
        let stages:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(stages.as_object().unwrap().len(),3);
        let mut previous:Option<Vec<f32>>=None;
        for(name,composition)in stages.as_object().unwrap(){
            let mut composition=composition.clone();composition["sources"]=json!({"source":{"id":"source","kind":"video","durationTicks":1125354,"width":16,"height":16,"hasAudio":true,"audioChannels":2}});
            let body=json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":0,"endTicks":1000000}}});
            let(status,accepted,_)=support::send(&app,request(&body,Some(&owner.token))).await;assert_eq!(status,StatusCode::ACCEPTED,"{name}: {accepted}");
            let done=terminal(&app,accepted["jobId"].as_str().unwrap(),&owner.token).await;assert_eq!(done["status"],"done","{name}: {done}");
            let actual=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(actual.len(),48000*2);
            let offset=cases[0]["offsetBytes"].as_u64().unwrap()as usize;
            for(i,actual)in actual.iter().enumerate(){
                let time=(i/2)as f64/48000.;let envelope=(0.25+0.75*time)*(time/2.)*(1.-time/2.);let at=offset+i*4;
                let expected=(f32::from_le_bytes(pcm[at..at+4].try_into().unwrap())*envelope as f32).clamp(-1.,1.-1./32768.);
                assert!((actual-expected).abs()<=1./32768.+2e-7,"linked WAV {name} sample{i}: {actual}/{expected}");
            }
            if let Some(previous)=&previous{assert_eq!(&actual,previous,"linked WAV stages differ: {name}");}previous=Some(actual);
        }
    }
    state
        .db
        .set_space_member(
            &space.id,
            &owner.user.username,
            &outsider.user.username,
            video_kadr_backend::db::SpaceRole::Viewer,
        )
        .await
        .unwrap()
        .unwrap();
    let (same_request, owner_job, owner_pcm) = first_export.unwrap();
    let (status, accepted, _) =
        support::send(&app, request(&same_request, Some(&outsider.token))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let member_job = accepted["jobId"].as_str().unwrap();
    assert_ne!(
        member_job, owner_job,
        "render job deduplication must retain actor ownership"
    );
    let done = terminal(&app, member_job, &outsider.token).await;
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(
        decode(
            &state
                .outputs_dir()
                .join(done["result"]["filename"].as_str().unwrap())
        ),
        owner_pcm
    );

    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}
