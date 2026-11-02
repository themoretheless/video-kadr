mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tower::ServiceExt;
fn request(path: &str, body: Value, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and ordinary WAV export worker"]
async fn imported_concat_overlap_keeps_private_preview_and_encoded_wav_linear() {
    for family in ["concat.dissolve","cross-fade","push","zoom","wipe-left","wipe-right"]{
        run(false,false,family).await;
        run(true,false,family).await;
        run(false,true,family).await;
        run(true,true,family).await;
    }
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and native colour delivery workers"]
async fn legacy_colour_private_delivery_preserves_hard_cut_sound_and_linear_picture(){
    for family in ["fade-black","fade-white"]{for held in [false,true]{for keyed in [false,true]{run(held,keyed,family).await;}}}
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and retimed native delivery workers"]
async fn retimed_legacy_colour_decodes_source_frames_and_delivers_unfaded_audio(){
    for family in ["fade-black","fade-white"] {
        for speed in [0.123456789_f64,0.5,1.234567891,2.] {
            run_at_speed(false,false,family,speed).await;
        }
    }
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and off-grid private delivery workers"]
async fn offgrid_legacy_colour_keeps_separate_video_grid_and_audio_clock(){
    for family in ["fade-black","fade-white"] {
        for (cut,source_start,speed) in [(2.019,1.019,1.),(2.02,1.02,0.5),(2.021,1.021,1.234567891)] {
            run_with_clock(false,false,family,speed,cut,source_start).await;
        }
    }
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and fractional-rate private delivery workers"]
async fn fractional_rate_legacy_colour_decodes_original_grid_in_private_delivery(){
    for family in ["fade-black","fade-white"] {
        for num in [30000_u64,24000,60000] {
            for cut in [2.45245,2.4525,2.45255] {
                run_at_rate(false,false,family,1.,cut,1.45145,num,1001,2_502_500).await;
            }
        }
    }
}
async fn run(held: bool, keyed: bool,family:&str) {
    run_at_speed(held,keyed,family,1.).await;
}
async fn run_at_speed(held: bool, keyed: bool,family:&str,speed:f64) {
    run_with_clock(held,keyed,family,speed,2.,1.).await;
}
async fn run_with_clock(held:bool,keyed:bool,family:&str,speed:f64,cut:f64,source_start:f64){
    run_at_rate(held,keyed,family,speed,cut,source_start,25,1,2_200_000).await;
}
async fn run_at_rate(held:bool,keyed:bool,family:&str,speed:f64,cut:f64,source_start:f64,num:u64,den:u64,probe_us:u64){
    let fps=num as f64/den as f64;
    let offgrid=cut!=2.||source_start!=1.;
    let (mut state, _directory) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools
        .ffmpeg_encoders
        .extend(["pcm_s16le".into(), "pcm_f32le".into(), "ffv1".into()]);
    tools
        .ffmpeg_muxers
        .extend(["wav".into(), "matroska".into()]);
    for (id, signal) in [
        ("a", "aevalsrc=0.25|0.125:s=48000:d=6"),
        ("b", "aevalsrc=-0.125|0.25:s=48000:d=6"),
    ] {
        let duration = if held && id == "a" { 3 } else { 6 };
        let mut video = if id == "a" {
            format!("color=c=red:s=16x16:r=25:d={duration},fade=t=in:st=0:d={duration}")
        } else {
            if offgrid{format!("color=c=blue:s=16x16:r=25:d={duration},geq=r=mod(N\\,3)*100:g=mod(N+1\\,3)*100:b=mod(N+2\\,3)*100")}else if speed!=1.{format!("color=c=blue:s=16x16:r=25:d={duration},fade=t=in:st=0:d={duration}")}else{format!("color=c=blue:s=16x16:r=25:d={duration}")}
        };
        // A spatial marker makes automatic zoom observable; solid frames
        // would remain identical when scaled around the canvas centre.
        if id=="a"&&family=="zoom"{video.push_str(",drawbox=x=6:y=6:w=4:h=4:color=green:t=fill");}
        let signal = if offgrid&&id=="b" {format!("aevalsrc=0.1*sin(2*PI*440*t)|0.2*sin(2*PI*660*t):s=48000:d={duration}")}else{signal.replace(":d=6", &format!(":d={duration}"))};
        let filename = format!("{id}.mov");
        let encoded = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &video,
                "-f",
                "lavfi",
                "-i",
                &signal,
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "qtrle",
                "-c:a",
                "pcm_f32le",
                "-threads",
                "1",
            ])
            .arg(state.sources_dir().join(&filename))
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":16,"height":16,"duration":duration,"fps":25,"hasAudio":true,"audioChannels":2}))).await;
    }
    let owner = state
        .db
        .register_auth_user("concat-overlap-owner", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("concat-overlap-other", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Overlap")
        .await
        .unwrap();
    let app = support::router(state.clone());
    let mut body = json!({"schemaVersion":1,"mediaBindings":{"a":{"id":"a","kind":"video","durationTicks":6000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2},"b":{"id":"b","kind":"video","durationTicks":6000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2}},"document":{"version":1,"media":[{"id":"a","kind":"video","path":"/unopened/a.mov"},{"id":"b","kind":"video","path":"/unopened/b.mov"}],"timelines":[{"id":"TL","video":{"width":16,"height":16,"rateNum":25,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"out","trackId":"v","kind":"video","mediaId":"a","start":0,"duration":2,"sourceStart":1},{"id":"in","trackId":"v","kind":"video","mediaId":"b","start":2,"duration":2,"sourceStart":1,"transitionIn":{"id":"concat.dissolve","duration":1}}]}]}});
    body["document"]["timelines"][0]["clips"][1]["transitionIn"]["id"]=json!(family);
    if speed!=1.{for clip in body["document"]["timelines"][0]["clips"].as_array_mut().unwrap(){clip["speed"]=json!(speed);}}
    body["document"]["timelines"][0]["clips"][0]["duration"]=json!(cut);
    body["document"]["timelines"][0]["clips"][1]["start"]=json!(cut);
    body["document"]["timelines"][0]["clips"][1]["sourceStart"]=json!(source_start);
    body["document"]["timelines"][0]["video"]["rateNum"]=json!(num);
    body["document"]["timelines"][0]["video"]["rateDen"]=json!(den);
    if keyed {
        for clip in body["document"]["timelines"][0]["clips"]
            .as_array_mut()
            .unwrap()
        {
            // Original gain keys replace the scalar; zero must not prune sound.
            clip["volume"]=json!(0);
            clip["videoEffects"] = json!([{"id":"concat.exposure","params":{"stops":0},"keys":{"stops":[{"at":0,"value":0},{"at":1,"value":1}]}}]);
            clip["keys"] = json!([
                ("scale", 0.6, 1.1),
                ("offsetX", -0.2, 0.25),
                ("offsetY", 0.15, -0.1),
                ("rotation", -25., 65.),
                ("opacity", 0.3, 0.9),
                ("volume", 0.2, 0.8)
            ]
            .into_iter()
            .flat_map(|(property, first, last)| [
                json!({"property":property,"at":0,"value":first}),
                json!({"property":property,"at":1,"value":last})
            ])
            .collect::<Vec<_>>());
        }
    }
    if held {
        body["mediaBindings"]["a"]["durationTicks"] = json!(3000000);
    }
    let (status, report, _) = support::send(
        &app,
        request(
            "/api/compositions/concat-project/import",
            body,
            &owner.token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["status"], "converted", "{report}");
    let project = &report["project"];
    let root = project["root"].as_str().unwrap();
    state
        .db
        .create_space_composition_project(
            "Overlap",
            project,
            &["a".into(), "b".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    if matches!(family,"fade-black"|"fade-white"){
        let marker=&project["compositions"][root]["content"]["tracks"][0]["transitions"][0]["concatOverlap"];
        assert_eq!(marker["leadTicks"],0);assert_ne!(marker["holdOutgoing"],true);
        // Capture the independent source reference before the authorized
        // preview resolves/re-homes its private media storage path.
        // Original reader floors Rational source time to a source-frame ordinal.
        // Select by ordinal, independently of the production seek/decoder path.
        let rate=(speed*1_000_000.).round() as i128;
        let local_frame=((probe_us as f64/1e6)*fps).round()as i128-(cut*fps).round()as i128;
        let source_frame=(((source_start*fps).round()as i128*1_000_000+local_frame*rate)*25*den as i128/(num as i128*1_000_000))as usize;
        let select=format!("select=eq(n\\,{source_frame})");
        let reference=if !keyed{let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(state.sources_dir().join("b.mov")).args(["-vf",&select,"-frames:v","1","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(decoded.status.success(),"colour source decode {family}/{held}: {}",String::from_utf8_lossy(&decoded.stderr));Some(decoded.stdout)}else{None};
        let audio_reference=if speed!=1.||offgrid{
            // Independent Concat tempo_stages + six-decimal speed_filters.
            let mut stages=Vec::new();let mut remaining=speed;
            while remaining>2.{stages.push(2.);remaining/=2.;}
            while remaining<0.5{stages.push(0.5);remaining/=0.5;}
            if speed!=1.{stages.push(remaining);}
            let mut graph=format!("atrim=start={source_start:.6}:duration={:.6},asetpts=PTS-STARTPTS",2.*speed);
            for factor in stages{graph.push_str(&format!(",atempo={factor:.6}"));}
            // Original audio filtergraph delays to the nearest millisecond.
            let first=(((probe_us as f64/1e6)-(cut*1000.).round()/1000.)*48000.).round() as usize;
            graph.push_str(&format!(",aresample=48000:async=1:first_pts=0,apad,atrim=start_sample={first}:end_sample={}",first+4800));
            let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(state.sources_dir().join("b.mov")).args(["-map","0:a:0","-af",&graph,"-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
            assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));
            assert_eq!(decoded.stdout.len(),9600*4);
            Some(decoded.stdout.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect::<Vec<_>>())
        }else{None};
        let preview_body=json!({"project":project,"compositionId":root,"timeUs":probe_us,"renderedOutputs":{}});
        let denied=app.clone().oneshot(request("/api/compositions/nested-preview",preview_body.clone(),&other.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
        let response=app.clone().oneshot(request("/api/compositions/nested-preview",preview_body,&owner.token)).await.unwrap();let status=response.status();let pixels=to_bytes(response.into_body(),1000000).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&pixels));assert_eq!(pixels.len(),1024);
        if let Some(reference)=reference.as_ref(){
            for (i,pixel) in reference.chunks_exact(4).enumerate(){for c in 0..3{let source=(pixel[c]as f64/255.).powf(2.4);let progress=local_frame as f64/(0.5*fps).round().max(1.);let linear=source*progress+if family=="fade-white"{1.-progress}else{0.};let want=(linear.powf(1./2.4)*255.).round()as i16;assert!((pixels[i*4+c]as i16-want).abs()<=1,"real original linear colour {family}/{held}/{cut}/{source_start}/{speed}/{i}/{c}: {}/{want}",pixels[i*4+c]);}}
        }
        let audio_body=json!({"project":project,"startSample":probe_us*48000/1000000,"frames":4800});
        let denied=app.clone().oneshot(request("/api/compositions/audio-preview",audio_body.clone(),&other.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
        let response=app.clone().oneshot(request("/api/compositions/audio-preview",audio_body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let audio=to_bytes(response.into_body(),1000000).await.unwrap();assert_eq!(audio.len(),9600*4);
        let expected=|i:usize|{if let Some(reference)=&audio_reference{return reference[i];}let time=probe_us+(i/2)as u64*1000000/48000;let gain=if keyed{0.2+0.6*((time as f64-2000000.)/2000000.)}else{1.};(gain*if i%2==0{-0.125}else{0.25})as f32};
        for(i,sample)in audio.chunks_exact(4).enumerate(){assert!((f32::from_le_bytes(sample.try_into().unwrap())-expected(i)).abs()<=2e-7,"colour preview sound {family}/{speed}/{i}: {} / {}",f32::from_le_bytes(sample.try_into().unwrap()),expected(i));}
        for av in [false,true]{let(path,body)=if av{("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":root,"startUs":probe_us,"endUs":probe_us+100000}))}else{("/api/compositions/render",json!({"schemaVersion":1,"composition":project["compositions"][root]["content"],"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":probe_us,"endTicks":probe_us+100000}}}))};
            let(status,accepted,_)=support::send(&app,request(path,body,&owner.token)).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let job=terminal(&app,accepted["jobId"].as_str().unwrap(),&owner.token).await;let path=state.outputs_dir().join(job["result"]["filename"].as_str().unwrap());
            if av{let video=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&path).args(["-map","0:v:0","-frames:v","1","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(video.status.success());assert_eq!(video.stdout.as_slice(),pixels.as_ref(),"colour encoded monitor frame {family}/{held}/{keyed}");}
            let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(decoded.status.success());assert!(decoded.stdout.len()>=9600*4);
            for(i,sample)in decoded.stdout.chunks_exact(4).enumerate(){let want=if i<9600{expected(i)}else{0.};let value=f32::from_le_bytes(sample.try_into().unwrap());assert!((value-want).abs()<=if av{2e-7}else{1./32768.+2e-7},"colour delivery sound {family}/{held}/{keyed}/{av}/{i}: {value}/{want}");}
        }
        return;
    }
    if held || (!keyed&&matches!(family,"push"|"zoom"|"wipe-left"|"wipe-right")) {
        let outgoing = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-ss",if held{"2.96"}else{"3.2"}, "-i"])
            .arg(state.sources_dir().join("a.mov"))
            .args([
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(outgoing.status.success());
        assert_eq!(outgoing.stdout.len(), 1024);
        let incoming = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-ss", "1.2", "-i"])
            .arg(state.sources_dir().join("b.mov"))
            .args([
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(incoming.status.success());
        let response=app.clone().oneshot(request("/api/compositions/nested-preview",json!({"project":project,"compositionId":root,"timeUs":2200000,"renderedOutputs":{}}),&owner.token)).await.unwrap();
        let status = response.status();
        let pixels = to_bytes(response.into_body(), 1000000).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&pixels)
        );
        assert_eq!(pixels.len(), 1024);
        // Original Concat upload/resolve use BT.1886 gamma 2.4, not sRGB.
        let linear = |v: u8| (v as f64 / 255.).powf(2.4);
        let display = |v: f64| v.powf(1. / 2.4);
        if !keyed&&matches!(family,"wipe-left"|"wipe-right"){
            let expected=if family=="wipe-left"{&outgoing.stdout}else{&incoming.stdout};
            for c in 0..3{assert!((pixels[c]as i16-expected[c]as i16).abs()<=1,"real legacy picture-space {family}/{held}/{c}");}
        }else if !keyed&&held {
            for c in 0..3 {
                let expected =
                    (display(linear(outgoing.stdout[c]) * 0.32 + linear(incoming.stdout[c]) * 0.68)
                        * 255.)
                        .round() as i16;
                assert!(
                    (pixels[c] as i16 - expected).abs() <= 1,
                    "Held real frame channel {c}: {}/{}",
                    pixels[c],
                    expected
                );
            }
        } else if keyed {
            let mut reference = project.clone();
            // Both authored offsetX and scale rides force these legacy
            // transitions to the original dissolve fallback. Freeze that
            // fallback explicitly before replacing the authored keyed poses.
            if matches!(family,"push"|"zoom"){reference["compositions"][root]["content"]["tracks"][0]["transitions"][0]["concatOverlap"]["legacy"]=json!("cross-fade");}
            for (index, clip) in reference["compositions"][root]["content"]["tracks"][0]["clips"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                let p = if index == 0 {
                    1.
                } else {
                    ((55. - 38.) / 63. - 0.2) / 0.8
                };
                clip["transform"]
                    .as_object_mut()
                    .unwrap()
                    .remove("concatAnimation");
                for (channel, value) in [
                    ("scaleX", 0.6 + (1.1 - 0.6) * p),
                    ("scaleY", 0.6 + (1.1 - 0.6) * p),
                    ("x", (-0.2 + (0.25 + 0.2) * p) * 16.),
                    ("y", (0.15 + (-0.1 - 0.15) * p) * 16.),
                    ("rotationDegrees", -25. + 90. * p),
                ] {
                    clip["transform"][channel] = json!({"mode":"constant","value":value});
                }
                clip["opacity"] = json!({"mode":"constant","value":0.3+0.6*p});
                clip["effects"] = json!([{"kind":"concat_exposure","parameters":{"stops":if index==0{5./13.}else{17./63.}}}]);
            }
            let response=app.clone().oneshot(request("/api/compositions/nested-preview",json!({"project":reference,"compositionId":root,"timeUs":2200000,"renderedOutputs":{}}),&owner.token)).await.unwrap();
            let status=response.status();
            let expected = to_bytes(response.into_body(), 1000000).await.unwrap();
            assert_eq!(status,StatusCode::OK,"frozen source {family}/{held}/{keyed}: {}",String::from_utf8_lossy(&expected));
            assert_eq!(pixels, expected, "original frame-grid source key pose {family}");
        }
        if !held&&!keyed{
            let mut still=project.clone();still["compositions"][root]["content"]["tracks"][0]["transitions"][0]["concatOverlap"]["legacy"]=json!("cross-fade");
            let response=app.clone().oneshot(request("/api/compositions/nested-preview",json!({"project":still,"compositionId":root,"timeUs":2200000,"renderedOutputs":{}}),&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
            let baseline=to_bytes(response.into_body(),1000000).await.unwrap();assert_ne!(pixels.as_ref(),baseline.as_ref(),"automatic source {family} movement must change the real frame");
        }
        let(status,accepted,_)=support::send(&app,request("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":root,"startUs":2200000,"endUs":2300000}),&owner.token)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
        let id = accepted["jobId"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let done = loop {
            let req = Request::builder()
                .uri(format!("/api/jobs/{id}"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap();
            let (_, job, _) = support::send(&app, req).await;
            if matches!(
                job["status"].as_str(),
                Some("done" | "error" | "cancelled" | "interrupted")
            ) {
                break job;
            }
            assert!(Instant::now() < deadline, "held AV export: {job}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(done["status"], "done", "{done}");
        let path = state
            .outputs_dir()
            .join(done["result"]["filename"].as_str().unwrap());
        let video = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(&path)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(video.status.success());
        assert_eq!(
            video.stdout.as_slice(),
            pixels.as_ref(),
            "lossless encoded held frame differs from native monitor"
        );
        let audio = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(audio.status.success());
        assert!(audio.stdout.len() >= 4800 * 8);
        for (i, sample) in audio.stdout.chunks_exact(4).enumerate() {
            let actual = f32::from_le_bytes(sample.try_into().unwrap());
            let expected = if i < 9600 {
                let time = 2200000 + (i / 2) as u64 * 1000000 / 48000;
                let gain = (time - 1500000) as f64 / 1e6
                    * if keyed {
                        0.2 + 0.6 * ((time as f64 - 2000000.) / 2000000.).clamp(0., 1.)
                    } else {
                        1.
                    };
                let outgoing=if held{0.}else{(2500000-time)as f64/1e6*if keyed{0.2+0.6*(time as f64/2000000.).clamp(0.,1.)}else{1.}};
                if i % 2 == 0 {
                    (0.25*outgoing-0.125 * gain) as f32
                } else {
                    (0.125*outgoing+0.25 * gain) as f32
                }
            } else {
                0.
            };
            assert!(
                (actual - expected).abs() <= 2e-7,
                "held encoded AV audio sample{i}: {actual}/{expected}"
            );
        }
    }
    for start in [1480000u64, 1960000, 2480000] {
        let preview = json!({"project":project,"startSample":start*48000/1000000,"frames":4800});
        let denied = app
            .clone()
            .oneshot(request(
                "/api/compositions/audio-preview",
                preview.clone(),
                &other.token,
            ))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(request(
                "/api/compositions/audio-preview",
                preview,
                &owner.token,
            ))
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1000000).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(bytes.len(), 4800 * 8);
        let expected = |i: usize| {
            let time = (start * 48000 / 1000000 + i as u64 / 2) * 1000000 / 48000;
            let ga = if held {
                if time < 2000000 {
                    1.
                } else {
                    0.
                }
            } else if time < 2500000 {
                ((2500000 - time) as f64 / 1000000.).min(1.)
            } else {
                0.
            };
            let gb = if time >= 1500000 {
                ((time - 1500000) as f64 / 1000000.).min(1.)
            } else {
                0.
            };
            let ga = ga
                * if keyed {
                    0.2 + 0.6 * (time as f64 / 2000000.).clamp(0., 1.)
                } else {
                    1.
                };
            let gb = gb
                * if keyed {
                    0.2 + 0.6 * ((time as f64 - 2000000.) / 2000000.).clamp(0., 1.)
                } else {
                    1.
                };
            if i % 2 == 0 {
                (ga * 0.25 - gb * 0.125) as f32
            } else {
                (ga * 0.125 + gb * 0.25) as f32
            }
        };
        for (i, sample) in bytes.chunks_exact(4).enumerate() {
            let actual = f32::from_le_bytes(sample.try_into().unwrap());
            assert!(
                (actual - expected(i)).abs() <= 2e-7,
                "preview start={start} sample={i}: {actual}/{}",
                expected(i)
            );
        }
        let(status,accepted,_)=support::send(&app,request("/api/compositions/render",json!({"schemaVersion":1,"composition":project["compositions"][root]["content"],"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":start,"endTicks":start+100000}}}),&owner.token)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
        let id = accepted["jobId"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let done = loop {
            let req = Request::builder()
                .uri(format!("/api/jobs/{id}"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap();
            let (_, job, _) = support::send(&app, req).await;
            if matches!(
                job["status"].as_str(),
                Some("done" | "error" | "cancelled" | "interrupted")
            ) {
                break job;
            }
            assert!(Instant::now() < deadline, "WAV job {id}: {job}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(done["status"], "done", "{done}");
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(
                state
                    .outputs_dir()
                    .join(done["result"]["filename"].as_str().unwrap()),
            )
            .args(["-f", "f32le", "-c:a", "pcm_f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 4800 * 8);
        for (i, sample) in decoded.stdout.chunks_exact(4).enumerate() {
            let actual = f32::from_le_bytes(sample.try_into().unwrap());
            assert!(
                (actual - expected(i)).abs() <= 1. / 32768. + 2e-7,
                "WAV start={start} sample={i}: {actual}/{}",
                expected(i)
            );
        }
    }
}

async fn terminal(app: &axum::Router, id: &str, token: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let req = Request::builder()
            .uri(format!("/api/jobs/{id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (_, job, _) = support::send(app, req).await;
        if matches!(
            job["status"].as_str(),
            Some("done" | "error" | "cancelled" | "interrupted")
        ) {
            assert_eq!(job["status"], "done", "{job}");
            return job;
        }
        assert!(Instant::now() < deadline, "chain export job {id}: {job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and ordinary native export workers"]
async fn sequential_chain_keeps_source_order_through_private_preview_wav_and_lossless_av() {
    run_source_chain(false,false).await;
    run_source_chain(true,false).await;
    run_source_chain(true,true).await;
}
async fn run_source_chain(overlapping:bool,treated:bool) {

    let (mut state, _directory) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools
        .ffmpeg_encoders
        .extend(["pcm_s16le".into(), "pcm_f32le".into(), "ffv1".into()]);
    tools
        .ffmpeg_muxers
        .extend(["wav".into(), "matroska".into()]);
    for (id, color, signal, duration) in [
        ("a", "red", "aevalsrc=0.25|0.25:s=48000:d=6", 6),
        ("b", "green", "aevalsrc=-0.125|-0.125:s=48000:d=6", 6),
        ("c", "blue", "aevalsrc=0.5|0.5:s=48000:d=10", 10),
    ] {
        let filename = format!("{id}.mov");
        let video = format!("color=c={color}:s=16x16:r=25:d={duration}");
        let encoded = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &video,
                "-f",
                "lavfi",
                "-i",
                signal,
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "qtrle",
                "-c:a",
                "pcm_f32le",
                "-threads",
                "1",
            ])
            .arg(state.sources_dir().join(&filename))
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":16,"height":16,"duration":duration,"fps":25,"hasAudio":true,"audioChannels":2}))).await;
    }
    let owner = state
        .db
        .register_auth_user("concat-chain-owner", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("concat-chain-other", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Chain")
        .await
        .unwrap();
    let app = support::router(state.clone());
    let bindings = json!({"a":{"id":"a","kind":"video","durationTicks":6000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2},"b":{"id":"b","kind":"video","durationTicks":6000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2},"c":{"id":"c","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2}});
    for reversed in [false, true] {
        let mut clips = vec![
            json!({"id":"a","trackId":"v","kind":"video","mediaId":"a","start":0,"duration":2,"sourceStart":1}),
            json!({"id":"b","trackId":"v","kind":"video","mediaId":"b","start":2,"duration":2,"sourceStart":1,"transitionIn":{"id":"concat.dissolve","duration":1}}),
            json!({"id":"c","trackId":"v","kind":"video","mediaId":"c","start":4,"duration":6,"sourceStart":2,"transitionIn":{"id":"concat.dissolve","duration":3}}),
        ];
        if overlapping {
            clips[1]["duration"]=json!(0.4);clips[1]["transitionIn"]=json!({"id":"concat.fade-black","duration":2});
            clips[2]["start"]=json!(2.4);clips[2]["duration"]=json!(2);clips[2]["sourceStart"]=json!(1);clips[2]["transitionIn"]["duration"]=json!(2);
        }
        if reversed { clips.swap(1, 2); }
        let sample_time=if overlapping{if reversed{2240000}else{2160000}}else{3200000};
        let mut document = json!({"version":1,"media":[{"id":"a","kind":"video","path":"/missing/a.mov"},{"id":"b","kind":"video","path":"/missing/b.mov"},{"id":"c","kind":"video","path":"/missing/c.mov"}],"timelines":[{"id":"TL","video":{"width":16,"height":16,"rateNum":25,"rateDen":1},"tracks":[{"id":"v"}],"clips":clips}]});
        if treated {
            document["timelines"][0]["tracks"].as_array_mut().unwrap().push(json!({"id":"look"}));
            document["timelines"][0]["clips"].as_array_mut().unwrap().push(json!({"id":"look","trackId":"look","kind":"layer","start":1.53,"duration":0.9,"opacity":0,"videoEffects":[{"id":"concat.exposure","params":{"stops":2}}]}));
            document["timelines"][0]["clips"].as_array_mut().unwrap().push(json!({"id":"look-two","trackId":"look","kind":"layer","start":1.7,"duration":0.8,"opacity":0,"videoEffects":[{"id":"concat.exposure","params":{"stops":1}}]}));
        }
        let (status, imported, _) = support::send(
            &app,
            request(
                "/api/compositions/concat-project/import",
                json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}),
                &owner.token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(imported["status"], "converted", "{imported}");
        let project = &imported["project"];
        let root = project["root"].as_str().unwrap();
        if !reversed {
            state
                .db
                .create_space_composition_project(
                    "Chain",
                    project,
                    &["a".into(), "b".into(), "c".into()],
                    &owner.user.username,
                    &space.id,
                )
                .await
                .unwrap();
        }
        let(status,exported,_)=support::send(&app,request("/api/compositions/concat-project/export",json!({"project":project,"mediaPaths":{"a":"/portable/a.mov","b":"/portable/b.mov","c":"/portable/c.mov"}}),&owner.token)).await;
        assert_eq!(status, StatusCode::OK, "{exported}");
        let (status, restored, _) = support::send(
            &app,
            request(
                "/api/compositions/concat-project/import",
                json!({"schemaVersion":1,"document":exported["document"],"mediaBindings":bindings}),
                &owner.token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(restored["status"], "converted", "{restored}");
        assert_eq!(
            project["compositions"][root]["content"]["tracks"],
            restored["project"]["compositions"][restored["project"]["root"].as_str().unwrap()]
                ["content"]["tracks"]
        );
        let audio_body = json!({"project":project,"startSample":sample_time*48000/1000000,"frames":4800});
        let denied = app
            .clone()
            .oneshot(request(
                "/api/compositions/audio-preview",
                audio_body.clone(),
                &other.token,
            ))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(request(
                "/api/compositions/audio-preview",
                audio_body,
                &owner.token,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let audio = to_bytes(response.into_body(), 1000000).await.unwrap();
        assert_eq!(audio.len(), 4800 * 8);
        let expected = |i: usize| {
            let time = sample_time + (i / 2) as u64 * 1000000 / 48000;
            if overlapping {
                let windows=if reversed{[(0,2300000,0,600000),(1700000,900000,600000,400000),(2200000,2200000,400000,0)]}else{[(0,2200000,0,400000),(1800000,900000,400000,600000),(2100000,2300000,600000,0)]};
                return windows.into_iter().zip([0.25,-0.125,0.5]).map(|((start,duration,fade_in,fade_out),signal)|{
                    if time<start||time>=start+duration{return 0.;}
                    let local=(time-start)as f64;
                    let incoming=if fade_in>0{(local/fade_in as f64).min(1.)}else{1.};
                    let outgoing=if fade_out>0{((duration as f64-local)/fade_out as f64).min(1.)}else{1.};
                    signal*incoming*outgoing
                }).sum::<f64>()as f32;
            }
            let (begin, duration) = if reversed {
                (3000000, 2000000)
            } else {
                (2750000, 2500000)
            };
            let gain = (time - begin) as f64 / duration as f64;
            (-0.125 * (1. - gain) + 0.5 * gain) as f32
        };
        for (i, sample) in audio.chunks_exact(4).enumerate() {
            let actual = f32::from_le_bytes(sample.try_into().unwrap());
            assert!(
                (actual - expected(i)).abs() <= 2e-7,
                "source chain preview {reversed}/{i}: {actual}/{}",
                expected(i)
            );
        }
        let response=app.clone().oneshot(request("/api/compositions/nested-preview",json!({"project":project,"compositionId":root,"timeUs":sample_time,"renderedOutputs":{}}),&owner.token)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let pixels = to_bytes(response.into_body(), 1000000).await.unwrap();
        assert_eq!(pixels.len(), 1024);
        for av in [false, true] {
            let (path, body) = if av {
                (
                    "/api/compositions/nested-export",
                    json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":root,"startUs":sample_time,"endUs":sample_time+100000}),
                )
            } else {
                (
                    "/api/compositions/render",
                    json!({"schemaVersion":1,"composition":project["compositions"][root]["content"],"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":sample_time,"endTicks":sample_time+100000}}}),
                )
            };

            let (status, accepted, _) =
                support::send(&app, request(path, body, &owner.token)).await;
            assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
            let job = terminal(&app, accepted["jobId"].as_str().unwrap(), &owner.token).await;
            let path = state
                .outputs_dir()
                .join(job["result"]["filename"].as_str().unwrap());
            if av {
                let video = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(&path)
                    .args([
                        "-map",
                        "0:v:0",
                        "-frames:v",
                        "1",
                        "-pix_fmt",
                        "rgba",
                        "-f",
                        "rawvideo",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(video.status.success());
                assert_eq!(video.stdout.as_slice(), pixels.as_ref());
            }
            let decoded = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args(["-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            assert!(decoded.stdout.len() >= 4800 * 8);
            for (i, sample) in decoded.stdout.chunks_exact(4).enumerate() {
                let actual = f32::from_le_bytes(sample.try_into().unwrap());
                let expected = if i < 9600 { expected(i) } else { 0. };
                assert!(
                    (actual - expected).abs() <= if av { 2e-7 } else { 1. / 32768. + 2e-7 },
                    "chain delivery {reversed}/{av}/{i}: {actual}/{expected}"
                );
            }
        }
    }
}
