mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::domain::composition::{CanvasSpec, Composition};
fn body() -> Value {
    json!({"project":{"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":40000000000u64,"content":Composition::new(CanvasSpec::default()),"instances":[]}}},"startSample":1,"frames":1})
}
fn request(body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/compositions/audio-preview")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[tokio::test]
async fn exact_sample_blocks_silence_and_invalid_requests() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state.clone());
    for (start, frames) in [(0, 1), (1, 1), (47, 49), (47999, 96000), (1728000000, 17)] {
        let mut b = body();
        b["startSample"] = json!(start);
        b["frames"] = json!(frames);
        let r = app.clone().oneshot(request(&b)).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()["x-audio-start-sample"], start.to_string());
        assert_eq!(r.headers()["x-audio-rate"], "48000");
        assert_eq!(r.headers()["cache-control"], "private, no-store");
        let pcm = to_bytes(r.into_body(), 1_000_000).await.unwrap();
        assert_eq!(pcm.len(), frames as usize * 8);
        assert!(pcm.iter().all(|v| *v == 0));
    }
    for (key, value) in [
        ("frames", json!(0)),
        ("frames", json!(96001)),
        ("startSample", json!(u64::MAX)),
        ("unknown", json!(1)),
    ] {
        let mut b = body();
        b[key] = value;
        let (status, _, _) = support::send(&app, request(&b)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let mut b = body();
    b["compositionId"] = json!("missing");
    assert_eq!(
        support::send(&app, request(&b)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
}
#[tokio::test]
#[ignore = "requires actual FFmpeg and FFprobe"]
async fn actual_echo_preview_non_aligned_blocks_match_original_pcm() {
    use std::io::Read;
    use video_kadr_backend::library::MediaEntry;
    let (state, _dir) = support::make_state(true, false).await;
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/concat-echo-pcm.json")).unwrap();
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["rate"] == 48000 && c["channels"] == 2)
        .unwrap();
    let frames = case["inputFrames"].as_u64().unwrap() as usize;
    let mut input = vec![];
    for i in 0..frames {
        for ch in 0..2 {
            let h = (i % 257 + ch * 1061) as u32;
            let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
            let value = if [0, 48000 / 7, frames - 1].contains(&i) {
                [1.25f32, -0.75][ch]
            } else {
                (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
            };
            input.extend(value.to_le_bytes());
        }
    }
    let raw = state.sources_dir().join("echo.raw");
    let wav = state.sources_dir().join("echo.wav");
    // Retain the original PCM prefix; pad the private browser source to eight seconds.
    input.resize(48000 * 8 * 8, 0);
    std::fs::write(&raw, input).unwrap();
    let result = std::process::Command::new("ffmpeg")
        .args([
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
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    state.library.add(MediaEntry::from_result("source",&json!({"id":"echo","filename":"echo.wav","url":"/files/sources/echo.wav","mediaType":"audio","duration":8.0,"hasAudio":true,"audioChannels":2}))).await;
    let owner = state
        .db
        .register_auth_user("preview-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("preview-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Preview source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Claim",
            &json!({"schemaVersion":1,"sources":{"echo":{"id":"echo"}}}),
            &["echo".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let mut b = body();
    b["project"]["compositions"]["root"]["content"]["sources"] = json!({"echo":{"id":"echo","kind":"audio","durationTicks":2000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}});
    b["project"]["compositions"]["root"]["content"]["tracks"] = json!([{"kind":"audio","id":"audio","name":"Echo","clips":[{"id":"clip","sourceId":"echo","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":case["settings"]}]}]);
    let app = support::router(state.clone());
    let import_body=json!({"schemaVersion":1,"document":{"version":1,"media":[{"id":"m1","kind":"audio","path":"/never/open-echo.wav"}],"timelines":[{"id":"TL1","tracks":[{"id":"T1"}],"clips":[{"id":"clip","trackId":"T1","mediaId":"m1","kind":"audio","duration":1,"filters":case["settings"]}]}]},"mediaBindings":{"m1":{"id":"echo","kind":"audio","durationTicks":2000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}}});
    let import_request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(import_body.to_string())).unwrap();
    let(status,imported,_)=support::send(&app,import_request).await;assert_eq!(status,StatusCode::OK);assert_eq!(imported["status"],"converted","{imported}");

    let av=state.sources_dir().join("echo-av.mov");
    let result=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i","color=c=black:s=16x16:r=4:d=8","-i",wav.to_str().unwrap(),"-map","0:v:0","-map","1:a:0","-c:v","qtrle","-c:a","pcm_f32le","-shortest",av.to_str().unwrap()]).output().unwrap();assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    state.library.add(MediaEntry::from_result("source",&json!({"id":"echo-av","filename":"echo-av.mov","url":"/files/sources/echo-av.mov","mediaType":"video","width":16,"height":16,"duration":8.0,"fps":4,"hasAudio":true,"audioChannels":2,"acodec":"pcm_f32le"}))).await;
    state.db.create_space_composition_project("AV source claim",&json!({"schemaVersion":1,"sources":{"echo-av":{"id":"echo-av"}}}),&["echo-av".into()],&owner.user.username,&space.id).await.unwrap();
    let mut av_import=import_body.clone();av_import["document"]["media"][0]["kind"]=json!("video");av_import["document"]["timelines"][0]["video"]=json!({"width":16,"height":16,"rateNum":4,"rateDen":1});av_import["document"]["timelines"][0]["tracks"]=json!([{"id":"T1","visible":false},{"id":"T2"}]);let first=&mut av_import["document"]["timelines"][0]["clips"][0];first["kind"]=json!("video");first["volume"]=json!(0.25);let mut second=first.clone();second["id"]=json!("second");second["trackId"]=json!("T2");second["volume"]=json!(0.5);av_import["document"]["timelines"][0]["clips"].as_array_mut().unwrap().push(second);av_import["mediaBindings"]["m1"]=json!({"id":"echo-av","kind":"video","durationTicks":8000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2});
    let r=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(av_import.to_string())).unwrap();let(status,av_imported,_)=support::send(&app,r).await;assert_eq!(status,StatusCode::OK);assert_eq!(av_imported["status"],"converted","{av_imported}");
    let mut faded_import=av_import.clone();
    for clip in faded_import["document"]["timelines"][0]["clips"].as_array_mut().unwrap(){clip["fadeIn"]=json!(2.);clip["fadeOut"]=json!(2.);}
    let r=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(faded_import.to_string())).unwrap();
    let(status,faded_imported,_)=support::send(&app,r).await;assert_eq!(status,StatusCode::OK);assert_eq!(faded_imported["status"],"converted","{faded_imported}");
    let mut faded_audio_import=import_body.clone();
    for clip in faded_audio_import["document"]["timelines"][0]["clips"].as_array_mut().unwrap(){clip["fadeIn"]=json!(2.);clip["fadeOut"]=json!(2.);}
    let r=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(faded_audio_import.to_string())).unwrap();
    let(status,faded_audio_imported,_)=support::send(&app,r).await;assert_eq!(status,StatusCode::OK);assert_eq!(faded_audio_imported["status"],"converted","{faded_audio_imported}");
    let mut keyed_projects=Vec::new();
    for(mut keyed,lo,hi)in [(import_body.clone(),0.25,1.),(av_import.clone(),0.1,0.4)] {
        for clip in keyed["document"]["timelines"][0]["clips"].as_array_mut().unwrap(){clip["volume"]=json!(0.9);clip["keys"]=json!([{"property":"volume","at":0,"value":lo},{"property":"volume","at":1,"value":hi}]);}
        let r=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(keyed.to_string())).unwrap();
        let(status,report,_)=support::send(&app,r).await;assert_eq!(status,StatusCode::OK);assert_eq!(report["status"],"converted","{report}");keyed_projects.push(report["project"].clone());
    }
    if let Ok(path)=std::env::var("CONCAT_VOLUME_EDITOR_KEYS_INPUT") {
        let keys:Vec<video_kadr_backend::domain::concat_effect_upgrade::ParamKey>=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(keys.len(),1);assert_eq!(keys[0].at,0.5);assert_eq!(keys[0].value,2.);assert_eq!(keys[0].ease,[0.,3.,1.,1.]);
        let mut edited=import_body.clone();edited["document"]["timelines"][0]["clips"][0]["volume"]=json!(0.9);
        edited["document"]["timelines"][0]["clips"][0]["keys"]=json!(keys.iter().map(|key|json!({"property":"volume","at":key.at,"value":key.value,"ease":key.ease})).collect::<Vec<_>>());
        let r=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(edited.to_string())).unwrap();
        let(status,report,_)=support::send(&app,r).await;assert_eq!(status,StatusCode::OK);assert_eq!(report["status"],"converted","{report}");keyed_projects.push(report["project"].clone());
    }
    let restored_project=std::env::var("CONCAT_DETACHED_REJOINED_PROJECT_INPUT").ok().map(|path|{
        let mut project:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for node in project["compositions"].as_object_mut().unwrap().values_mut(){
            let content=&mut node["content"];assert_eq!(content["sources"].as_object().unwrap().len(),1);
            content["sources"]=json!({"echo-av":{"id":"echo-av","kind":"video","durationTicks":8000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2}});
            if let Some(names)=content["metadata"]["sourceNames"].as_object_mut(){if let Some(name)=names.remove("source"){names.insert("echo-av".into(),name);}}
            for track in content["tracks"].as_array_mut().unwrap(){for clip in track["clips"].as_array_mut().unwrap(){if clip["sourceId"]=="source"{clip["sourceId"]=json!("echo-av");}}}
        }
        project
    });
    let linked_roundtrip=std::env::var("CONCAT_LINKED_AUDIO_ROUNDTRIP_INPUT").ok().map(|path|{
        let stages:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(stages.as_object().unwrap().len(),3);
        stages.as_object().unwrap().iter().map(|(name,content)|{
            let mut content=content.clone();assert_eq!(content["sources"].as_object().unwrap().len(),1);
            content["sources"]=json!({"echo-av":{"id":"echo-av","kind":"video","durationTicks":8000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2}});
            for track in content["tracks"].as_array_mut().unwrap(){for clip in track["clips"].as_array_mut().unwrap(){assert_eq!(clip["sourceId"],"source");clip["sourceId"]=json!("echo-av");}}
            (name.clone(),json!({"schemaVersion":1,"root":"linked","compositions":{"linked":{"content":content,"instances":[]}}}))
        }).collect::<Vec<_>>()
    });
    let mut inherited_audio=import_body.clone();
    let audio=inherited_audio["document"]["timelines"][0]["clips"][0].as_object_mut().unwrap();
    for(key,value)in json!({"scale":2,"stretchX":1.3,"stretchY":0.8,"offsetX":0.2,"offsetY":-0.3,"rotation":30,"opacity":0.4,"blend":"multiply","flipH":true,"flipV":true,"crop":{"left":0.1,"right":0.1,"top":0.2,"bottom":0},"cutout":{"mode":"auto","feather":0.02,"strokes":[]},"videoEffects":[{"id":"concat.exposure","params":{"stops":2}}],"keys":[{"property":"opacity","at":0,"value":0},{"property":"scale","at":1,"value":2}]}).as_object().unwrap(){audio.insert(key.clone(),value.clone());}
    audio.get_mut("filters").unwrap()[0]["keys"]=json!({"delay":[{"at":0,"value":0.05},{"at":1,"value":1,"ease":"inOut"}]});
    let r=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(inherited_audio.to_string())).unwrap();
    let(status,inherited_imported,_)=support::send(&app,r).await;assert_eq!(status,StatusCode::OK);assert_eq!(inherited_imported["status"],"converted","{inherited_imported}");assert_eq!(inherited_imported["sourceDocument"],inherited_audio["document"]);
    assert_eq!(
        support::send(&app, request(&b)).await.0,
        StatusCode::UNAUTHORIZED
    );
    let mut r = request(&b);
    r.headers_mut().insert(
        "authorization",
        format!("Bearer {}", outsider.token).parse().unwrap(),
    );
    assert_eq!(support::send(&app, r).await.0, StatusCode::FORBIDDEN);
    let mut oracle = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/concat-echo-pcm.f32le.gz").as_slice())
        .read_to_end(&mut oracle)
        .unwrap();
    let offset = case["offsetBytes"].as_u64().unwrap() as usize;
    for (start, count) in [(1, 1), (47, 49), (12001, 1003), (24017, 1709), (47999, 1)] {
        b["startSample"] = json!(start);
        b["frames"] = json!(count);
        let mut r = request(&b);
        r.headers_mut().insert(
            "authorization",
            format!("Bearer {}", owner.token).parse().unwrap(),
        );
        let response = app.clone().oneshot(r).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(bytes.len(), count * 8);
        let mut imported_body=b.clone();imported_body["project"]=imported["project"].clone();let mut imported_request=request(&imported_body);imported_request.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());let response=app.clone().oneshot(imported_request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let imported_pcm=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(imported_pcm,bytes,"imported audio chain at sample{start}");
        let mut av_body=b.clone();av_body["project"]=av_imported["project"].clone();let mut r=request(&av_body);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());let response=app.clone().oneshot(r).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let av_pcm=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(av_pcm.len(),bytes.len());for(actual,expected)in av_pcm.chunks_exact(4).zip(bytes.chunks_exact(4)){let actual=f32::from_le_bytes(actual.try_into().unwrap());let expected=f32::from_le_bytes(expected.try_into().unwrap())*0.75;assert!((actual-expected).abs()<=2e-7,"independent embedded lanes at sample{start}: {actual}/{expected}");}

        let mut faded_body=b.clone();faded_body["project"]=faded_imported["project"].clone();
        let mut r=request(&faded_body);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
        let response=app.clone().oneshot(r).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        let faded_pcm=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(faded_pcm.len(),bytes.len());
        for(i,(actual,reference))in faded_pcm.chunks_exact(4).zip(bytes.chunks_exact(4)).enumerate(){
            let actual=f32::from_le_bytes(actual.try_into().unwrap());
            let local_us=((start+i/2) as u64*1_000_000)/48000;
            let local=local_us as f64/1_000_000.;
            let expected=f32::from_le_bytes(reference.try_into().unwrap())*(0.75*(local/2.)*((1.-local)/2.)) as f32;
            assert!((actual-expected).abs()<=2e-7,"overlong preview sample{start}+{i}: {actual}/{expected}");
        }

        let mut faded_audio_body=b.clone();faded_audio_body["project"]=faded_audio_imported["project"].clone();
        let mut r=request(&faded_audio_body);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
        let response=app.clone().oneshot(r).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        let faded_audio_pcm=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(faded_audio_pcm.len(),bytes.len());
        for(i,(actual,reference))in faded_audio_pcm.chunks_exact(4).zip(bytes.chunks_exact(4)).enumerate(){
            let actual=f32::from_le_bytes(actual.try_into().unwrap());
            let local=(((start+i/2)as u64*1_000_000)/48000)as f64/1_000_000.;
            let expected=f32::from_le_bytes(reference.try_into().unwrap())*((local/2.)*((1.-local)/2.))as f32;
            assert!((actual-expected).abs()<=2e-7,"audio overlong preview sample{start}+{i}: {actual}/{expected}");
        }

        for(index,project)in keyed_projects.iter().enumerate(){
            let mut keyed_body=b.clone();keyed_body["project"]=project.clone();let mut r=request(&keyed_body);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
            let response=app.clone().oneshot(r).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
            let keyed_pcm=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(keyed_pcm.len(),bytes.len());
            for(i,(actual,reference))in keyed_pcm.chunks_exact(4).zip(bytes.chunks_exact(4)).enumerate(){
                let actual=f32::from_le_bytes(actual.try_into().unwrap());let local=(((start+i/2)as u64*1_000_000)/48000)as f64/1_000_000.;
                let gain=if index==0 {0.25+0.75*local}else if index==1 {0.2+0.6*local}else{2.};let expected=f32::from_le_bytes(reference.try_into().unwrap())*gain as f32;
                assert!((actual-expected).abs()<=2e-7,"source volume preview{index} sample{start}+{i}: {actual}/{expected}");
            }
        }

        if let Some(project)=&restored_project {
            let mut restored=b.clone();restored["project"]=project.clone();restored["startSample"]=json!(48000+start);
            let mut r=request(&restored);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
            let response=app.clone().oneshot(r).await.unwrap();let status=response.status();let actual=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&actual));assert_eq!(actual.len(),bytes.len());
            for(actual,expected)in actual.chunks_exact(4).zip(bytes.chunks_exact(4)){let actual=f32::from_le_bytes(actual.try_into().unwrap());let expected=f32::from_le_bytes(expected.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"reattached source sample{start}: {actual}/{expected}");}
        }

        if let Some(stages)=&linked_roundtrip {
            let mut previous:Option<Vec<u8>>=None;
            for(name,project)in stages {
                let mut stage=b.clone();stage["project"]=project.clone();let mut r=request(&stage);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
                let response=app.clone().oneshot(r).await.unwrap();let status=response.status();let actual=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(status,StatusCode::OK,"{name}: {}",String::from_utf8_lossy(&actual));assert_eq!(actual.len(),bytes.len());
                for(i,(actual,reference))in actual.chunks_exact(4).zip(bytes.chunks_exact(4)).enumerate(){
                    let actual=f32::from_le_bytes(actual.try_into().unwrap());let local=(((start+i/2)as u64*1_000_000)/48000)as f64/1_000_000.;
                    let expected=f32::from_le_bytes(reference.try_into().unwrap())*((0.25+0.75*local)*(local/2.)*((1.-local)/2.))as f32;
                    assert!((actual-expected).abs()<=2e-7,"linked {name} sample{start}+{i}: {actual}/{expected}");
                }
                if let Some(previous)=&previous{assert_eq!(actual.as_ref(),previous.as_slice(),"linked stages differ at {start}: {name}");}previous=Some(actual.to_vec());
            }
        }

        let mut inherited=b.clone();inherited["project"]=inherited_imported["project"].clone();let mut r=request(&inherited);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
        let response=app.clone().oneshot(r).await.unwrap();let status=response.status();let actual=to_bytes(response.into_body(),1_000_000).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&actual));assert_eq!(actual,bytes,"inherited picture controls changed audio at {start}");


        for (i, p) in bytes.chunks_exact(4).enumerate() {
            let actual = f32::from_le_bytes(p.try_into().unwrap());
            let begin = offset + start * 8 + i * 4;
            let expected = f32::from_le_bytes(oracle[begin..begin + 4].try_into().unwrap());
            assert!(
                (actual - expected).abs() <= 2e-7,
                "start {start}, value {i}: {actual} vs {expected}"
            );
        }
    }
    if std::env::var_os("VIDEO_KADR_PREVIEW_BROWSER").is_some() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = tokio_util::sync::CancellationToken::new();
        let shutdown = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let mut command = tokio::process::Command::new("node");
        command
            .current_dir(workspace)
            .arg("frontend/scripts/run-native-audio-preview-e2e.mjs")
            .env("NATIVE_AUDIO_BACKEND_URL", format!("http://{address}"))
            .env("NATIVE_AUDIO_TEST_TOKEN", &owner.token)
            .env("NATIVE_AUDIO_OUTSIDER_TOKEN", &outsider.token)
            .kill_on_drop(true);
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(90), command.output()).await;
        stop.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(35), server)
            .await
            .unwrap()
            .unwrap();
        let output = result.expect("browser qualification timeout").unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
}

#[tokio::test]
#[ignore = "requires actual FFmpeg and FFprobe"]
async fn actual_packet_effect_catalog_and_http_preview_match_original() {
    use video_kadr_backend::library::MediaEntry;
    let (state,_dir)=support::make_state(true,false).await;
    let wav=state.sources_dir().join("packet.wav");
    let generated=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","aevalsrc=1.4*sin(2*PI*317*t)|0.7*cos(2*PI*997*t):s=44100:d=3","-c:a","pcm_f64le"]).arg(&wav).output().unwrap();
    assert!(generated.status.success(),"{}",String::from_utf8_lossy(&generated.stderr));
    state.library.add(MediaEntry::from_result("source",&json!({"id":"packet","filename":"packet.wav","url":"/files/sources/packet.wav","mediaType":"audio","duration":3.0,"hasAudio":true,"audioChannels":2}))).await;
    let owner=state.db.register_auth_user("packet-preview-owner","public synthetic password").await.unwrap().unwrap();
    let space=state.db.create_space(&owner.user.username,"Packet preview").await.unwrap();
    state.db.create_space_composition_project("Claim",&json!({"schemaVersion":1,"sources":{"packet":{"id":"packet"}}}),&["packet".into()],&owner.user.username,&space.id).await.unwrap();
    state.library.relocate_source_to_space("packet",&space.id).await.unwrap();
    let wav=state.library.resolve_media_path(&state.library.get("packet").await.unwrap()).await.unwrap();
    let app=support::router(state.clone());
    // Catalogue route is intentionally public; authored source preview is authenticated.
    let response=app.clone().oneshot(Request::builder().uri("/api/compositions/audio-dsp-catalog").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(),StatusCode::OK);
    let catalog:Value=serde_json::from_slice(&to_bytes(response.into_body(),2_000_000).await.unwrap()).unwrap();
    let sweet_plans:Value=serde_json::from_str(include_str!("fixtures/concat-sweet-plans.json")).unwrap();
    let sweet=sweet_plans["cases"].as_array().unwrap().iter().find(|c|c["amount"].as_f64()==Some(65.) && c["pitch"].as_f64()==Some(0.)).unwrap();
    for (id,pitch,suffix) in [("concat.pitch",7f64,""),("concat.chipmunk",7.,",highpass=f=120,alimiter=limit=0.94"),("concat.deep",-3.,",highpass=f=55,equalizer=f=140:t=q:w=1.0:g=3.00,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60"),("concat.sweet",0.,""),("concat.loudness",0.,"")] {
        let entry=catalog["effects"].as_array().unwrap().iter().find(|e|e["id"]==id).unwrap();
        assert_eq!(entry["nativeExport"],true);
        assert!(entry["params"].as_array().unwrap().iter().all(|p|p["step"]==if p["id"]=="amount" || p["id"]=="target" {json!(1.0)} else {json!(0.5)} && p["animatable"]==false));
        let settings=if id=="concat.pitch" {json!([{"id":id,"params":{"semitones":7}}])} else {json!([{"id":id}])};
        let mut b=body();b["project"]["compositions"]["root"]["durationUs"]=json!(2_000_000);
        b["project"]["compositions"]["root"]["content"]["sources"]=json!({"packet":{"id":"packet","kind":"audio","durationTicks":3000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}});
        b["project"]["compositions"]["root"]["content"]["tracks"]=json!([{"kind":"audio","id":"audio","name":"Packet","clips":[{"id":"clip","sourceId":"packet","placement":{"timelineStartTick":50000,"sourceInTick":100000,"sourceOutTick":900000,"speed":1},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":settings}]}]);
        b["startSample"]=json!(2401);b["frames"]=json!(49);
        assert_eq!(support::send(&app,request(&b)).await.0,StatusCode::UNAUTHORIZED);
        let ratio=2f64.powf(pitch/12.);
        let chain=if id=="concat.loudness" {"atrim=start_sample=4410:end_sample=39690,asetpts=PTS-STARTPTS,loudnorm=I=-23.0:TP=-1.5:LRA=11,aresample=48000:osf=flt".to_string()} else if id=="concat.sweet" {format!("atrim=start_sample=4410:end_sample=39690,asetpts=PTS-STARTPTS,{}",sweet["chain"].as_str().unwrap())} else {format!("atrim=start_sample=4410:end_sample=39690,asetpts=PTS-STARTPTS,aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}{suffix}",48000.*ratio,1./ratio)};
        let original=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&wav).args(["-af",&chain,"-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
        assert!(original.status.success(),"{}",String::from_utf8_lossy(&original.stderr));
        for (start,count) in [(0usize,47usize),(2401,49),(12001,1003),(40801,79)] {
            b["startSample"]=json!(start);b["frames"]=json!(count);
            let mut r=request(&b);r.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
            let response=app.clone().oneshot(r).await.unwrap();
            let status=response.status();let headers=response.headers().clone();let bytes=to_bytes(response.into_body(),1_000_000).await.unwrap();
            assert_eq!(status,StatusCode::OK,"{id}: {}",String::from_utf8_lossy(&bytes));
            assert_eq!(headers["cache-control"],"private, no-store");
            assert_eq!(headers["x-audio-start-sample"],start.to_string());assert_eq!(bytes.len(),count*8);
            for (i,p) in bytes.chunks_exact(4).enumerate() {
                let clock=start+i/2;let ch=i%2;
                let expected=if (2400..40800).contains(&clock) {
                    let offset=((clock-2400)*2+ch)*4;
                    original.stdout.get(offset..offset+4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0.)
                } else {0.};
                let actual=f32::from_le_bytes(p.try_into().unwrap());
                assert!((actual-expected).abs()<=2e-7,"{id} start{start} value{i}: {actual} vs {expected}");
            }
        }
    }
    if std::env::var_os("VIDEO_KADR_PACKET_PREVIEW_BROWSER").is_some() {
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let stop=tokio_util::sync::CancellationToken::new();let shutdown=stop.clone();
        let server=tokio::spawn(async move {axum::serve(listener,app).with_graceful_shutdown(shutdown.cancelled_owned()).await.unwrap()});
        let workspace=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut command=tokio::process::Command::new("node");
        command.current_dir(workspace).arg("frontend/scripts/run-packet-audio-preview-e2e.mjs")
            .env("NATIVE_AUDIO_BACKEND_URL",format!("http://{address}"))
            .env("NATIVE_AUDIO_TEST_TOKEN",&owner.token).env("NATIVE_AUDIO_TEST_SOURCE",&wav).kill_on_drop(true);
        let result=tokio::time::timeout(std::time::Duration::from_secs(90),command.output()).await;
        stop.cancel();tokio::time::timeout(std::time::Duration::from_secs(35),server).await.unwrap().unwrap();
        let output=result.expect("packet browser qualification timeout").unwrap();
        println!("{}",String::from_utf8_lossy(&output.stdout));
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    }
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(),0);
}
