use super::*;

#[tokio::test]
async fn concat_split_join_preserves_real_video_pixels_and_outer_transitions() {
    use std::process::Command;
    let (mut state, _directory) = make_state(true, true).await;
    std::sync::Arc::make_mut(&mut state.tools).ffmpeg_encoders.push("ffv1".into());
    std::sync::Arc::make_mut(&mut state.tools).ffmpeg_muxers.push("matroska".into());
    for (id, seed) in [("source", 0u32), ("next-source", 97u32)] {
        let raw = state.sources_dir().join(format!("{id}.rgba"));
        let rgba: Vec<u8> = (0..100u32).flat_map(|frame| (0..16u32).flat_map(move |i| [
            ((i * 37 + frame * 29 + 11 + seed) % 256) as u8,
            ((i * 73 + frame * 17 + 51 + seed * 2) % 256) as u8,
            ((i * 19 + frame * 43 + 83 + seed * 3) % 256) as u8, 255,
        ])).collect();
        std::fs::write(&raw, rgba).unwrap();
        let filename = format!("{id}.mov");
        let movie = state.sources_dir().join(&filename);
        let encoded = Command::new("ffmpeg").args(["-nostdin", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "4x4", "-r", "10", "-i"])
            .arg(&raw).args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo", "-t", "10", "-c:v", "qtrle", "-pix_fmt", "argb", "-c:a", "pcm_s16le", "-threads", "1", "-shortest"])
            .arg(&movie).output().unwrap();
        assert!(encoded.status.success(), "{}", String::from_utf8_lossy(&encoded.stderr));
        assert!(state.library.add(MediaEntry::from_result("source", &json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video"}))).await);
    }
    let raw_still=state.sources_dir().join("duplicate-still.rgba");
    let rgba_still:Vec<u8>=(0..16u8).flat_map(|i|[i.wrapping_mul(53),255-i*13,i*11,255]).collect();
    std::fs::write(&raw_still,rgba_still).unwrap();
    let still=state.sources_dir().join("duplicate-still.png");
    let encoded_still=Command::new("ffmpeg").args(["-nostdin","-v","error","-f","rawvideo","-pix_fmt","rgba","-s","4x4","-i"]).arg(&raw_still).args(["-frames:v","1","-threads","1"]).arg(&still).output().unwrap();
    assert!(encoded_still.status.success(),"{}",String::from_utf8_lossy(&encoded_still.stderr));
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"still","filename":"duplicate-still.png","url":"/files/sources/duplicate-still.png","mediaType":"image"}))).await);
    let app = router(state.clone());
    let token = register_token(&app, "concat-join-owner").await;
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let mut comparisons = 0;
    for case in cases {
        for time in case["times"].as_array().unwrap() {
            let mut baseline = None;
            for version in ["original", "split", "joined"] {
                let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":8000000,"content":case[version],"instances":[]}}});
                let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":project,"timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status, StatusCode::OK, "{} {version} {time}: {}", case["name"], String::from_utf8_lossy(&bytes));
                assert_eq!(bytes.len(), 16 * 16 * 4);
                if let Some(expected) = &baseline { assert_eq!(&bytes, expected, "{} {version} {time}", case["name"]); comparisons += 1; }
                else { baseline = Some(bytes); }
            }
        }
    }
    assert_eq!(comparisons, 128);
    let holds = fixture["holds"].as_array().unwrap();
    assert_eq!(holds.len(), 2);
    let mut held_comparisons = 0;
    for case in holds {
        for time in case["times"].as_array().unwrap() {
            let at = time.as_i64().unwrap();
            let reference_time = if at < 1_500_000 { at } else if at < 2_500_000 { 1_500_000 } else { at - 1_000_000 };
            let mut baseline = None;
            for (version, sample, duration) in [("original", reference_time, 8_000_000), ("inserted", at, 9_000_000)] {
                let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":duration,"content":case[version],"instances":[]}}});
                let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":project,"timeUs":sample,"renderedOutputs":{}})), &token)).await.unwrap();
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status, StatusCode::OK, "hold reverse{} {version}/{at}: {}", case["reverse"], String::from_utf8_lossy(&bytes));
                assert_eq!(bytes.len(), 16 * 16 * 4);
                if let Some(expected) = &baseline { assert_eq!(&bytes, expected, "hold reverse{} at{at}", case["reverse"]); held_comparisons += 1; }
                else { baseline = Some(bytes); }
            }
        }
    }
    assert_eq!(held_comparisons, 20);
    let versions = &fixture["versions"];
    let mut version_comparisons = 0;
    let mut different = 0;
    for time in versions["times"].as_array().unwrap() {
        let mut original_pixels = None;
        for group in [&["original", "off"][..], &["reference", "processed", "on"][..]] {
            let mut baseline = None;
            for (index, version) in group.iter().enumerate() {
                let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":8000000,"content":versions[*version],"instances":[]}}});
                let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":project,"timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status, StatusCode::OK, "version {version}/{time}: {}", String::from_utf8_lossy(&bytes));
                assert_eq!(bytes.len(), 16 * 16 * 4);
                if let Some(expected) = &baseline { assert_eq!(&bytes, expected, "media version {version}/{time}"); version_comparisons += 1; }
                else { baseline = Some(bytes.clone()); }
                if index == 0 && *version == "original" { original_pixels = Some(bytes.clone()); }
                if index == 0 && *version == "reference" && Some(&bytes) != original_pixels.as_ref() { different += 1; }
            }
        }
    }
    assert_eq!(version_comparisons, 18);
    assert!(different >= 4, "processed reference must visibly differ from original");
    let mut move_comparisons = 0;
    for case in cases {
        for reverse in [false, true] {
            let mut project = fixture["createdTimeline"].clone();
            project["compositions"]["root"]["content"] = case["original"].clone();
            if reverse {
                project["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["playbackMode"] = json!({"mode":"reverse"});
                project["compositions"]["root"]["content"]["tracks"][1]["clips"][0]["reversed"] = json!(true);
            }
            let moves = json!([
                {"fromComposition":"root","fromTrackId":"video","clipId":"picture","toComposition":"new-timeline","toTrackId":"video-1","startUs":2000000},
                {"fromComposition":"root","fromTrackId":"video","clipId":"next","toComposition":"new-timeline","toTrackId":"video-1","startUs":6000000},
                {"fromComposition":"root","fromTrackId":"audio","clipId":"sound","toComposition":"new-timeline","toTrackId":"audio-1","startUs":2000000}
            ]);
            let (status, moved, _) = send(&app, post_json("/api/compositions/nested-move-clips", json!({"project":project,"moves":moves}))).await;
            assert_eq!(status, StatusCode::OK, "move {} reverse{reverse}: {moved}", case["name"]);
            assert_eq!(moved["changed"], true);
            assert_eq!(moved["affectedCompositions"], json!(["new-timeline","root"]));
            for at in [750000u64,1499999,1500000,3500000,3750000,4250000,4500000,7999999] {
                let mut expected = None;
                for (document, composition, time) in [(&project,"root",at),(&moved["project"],"new-timeline",at+2_000_000)] {
                    let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":document,"compositionId":composition,"timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
                    let status = response.status();
                    let bytes = response.into_body().collect().await.unwrap().to_bytes();
                    assert_eq!(status, StatusCode::OK, "move {} reverse{reverse} at{at}: {}",case["name"],String::from_utf8_lossy(&bytes));
                    if let Some(expected) = &expected { assert_eq!(&bytes,expected,"move {} reverse{reverse} at{at}",case["name"]);move_comparisons+=1; }
                    else { assert_eq!(bytes.len(),16*16*4);expected=Some(bytes); }
                }
            }
        }
    }
    assert_eq!(move_comparisons,64);
    let video_settings = &fixture["videoSettings"];
    let resized_project = &video_settings["resized"];
    assert_eq!(resized_project["compositions"]["root"]["content"]["canvas"]["fpsRate"], json!({"numerator":60000,"denominator":1001}));
    assert_eq!(resized_project["compositions"]["new-timeline"], video_settings["original"]["compositions"]["new-timeline"]);
    let mut frames = Vec::new();
    for version in ["original", "resized"] {
        let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":video_settings[version],"timeUs":750000,"renderedOutputs":{}})), &token)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "canvas preview {version}");
        frames.push(response.into_body().collect().await.unwrap().to_bytes());
    }
    assert_eq!(frames[0].len(), 16 * 16 * 4);
    assert_eq!(frames[1].len(), 32 * 24 * 4);
    let mut visible = false;
    for y in 0..4 { for x in 0..4 {
        let before = ((y + 6) * 16 + x + 6) * 4;
        let after = ((y + 10) * 32 + x + 14) * 4;
        assert_eq!(&frames[0][before..before + 4], &frames[1][after..after + 4], "canvas source pixel {x}/{y}");
        visible |= frames[0][before..before + 3].iter().any(|byte| *byte > 0);
    }}
    assert!(visible, "canvas comparison must include visible source pixels");
    let (status, accepted, _) = send(&app, bearer(post_json("/api/compositions/nested-export", json!({"schemaVersion":1,"mode":"video_only","project":resized_project,"compositionId":"root","startUs":750000,"endUs":850100,"renderedOutputs":{}})), &token)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let done = poll_terminal(&app, accepted["jobId"].as_str().unwrap()).await;
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(done["result"]["fpsRate"], json!({"numerator":60000,"denominator":1001}));
    let path = state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
    let probed = Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-count_frames","-show_entries","stream=width,height,avg_frame_rate,time_base,nb_read_frames:stream_tags=VIDEO_KADR_FRAME_RATE:frame=pts","-of","json"]).arg(&path).output().unwrap();
    assert!(probed.status.success(), "{}", String::from_utf8_lossy(&probed.stderr));
    let probe: Value = serde_json::from_slice(&probed.stdout).unwrap();
    let stream = &probe["streams"][0];
    assert_eq!(stream["width"], 32); assert_eq!(stream["height"], 24);
    assert_eq!(stream["tags"]["VIDEO_KADR_FRAME_RATE"], "60000/1001");
    // Matroska's integer clock quantizes PTS; check the exact nearest-tick projection.
    let clock: Vec<u128> = stream["time_base"].as_str().unwrap().split('/').map(|part| part.parse().unwrap()).collect();
    assert_eq!(clock.len(), 2);
    assert!(clock.iter().all(|part| *part > 0));
    let decoded_frames = probe["frames"].as_array().unwrap();
    assert_eq!(decoded_frames.len(), 6);
    for (index, frame) in decoded_frames.iter().enumerate() {
        let divisor = 60000 * clock[0];
        let scaled = index as u128 * 1001 * clock[1];
        let expected_pts = (scaled * 2 + divisor) / (divisor * 2);
        assert_eq!(frame["pts"].as_u64().unwrap() as u128, expected_pts, "frame PTS {index}");
    }
    assert_eq!(stream["nb_read_frames"], "6");
    let decoded = Command::new("ffmpeg").args(["-v","error","-i"]).arg(path).args(["-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout.len(), 32 * 24 * 4 * 6);
    assert_eq!(&decoded.stdout[..32 * 24 * 4], frames[1].as_ref(), "first exported frame must match resized native preview");
    let insertions=fixture["nativeInsertions"].as_array().unwrap();assert_eq!(insertions.len(),3);
    for case in insertions.iter().filter(|case|case["kind"]!="image") {
        for version in ["inserted","reference"] {
            let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":case[version],"timeUs":0}))).await;
            assert_eq!(status,StatusCode::OK,"insert {} {version}: {result}",case["kind"]);
        }
        for time in case["times"].as_array().unwrap() {
            let mut baseline=None;
            for version in ["inserted","reference"] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case[version],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"insert {} {version}: {}",case["kind"],String::from_utf8_lossy(&bytes));
                if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"insert {} at{time}",case["kind"]);}else{baseline=Some(bytes);}
            }
        }
    }
    let first_free=fixture["nativeFirstFree"].as_array().unwrap();assert_eq!(first_free.len(),2);
    for case in first_free {
        let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":case["after"],"timeUs":2000000}))).await;
        assert_eq!(status,StatusCode::OK,"first-free: {result}");
        let tracks=case["after"]["compositions"]["new-timeline"]["content"]["tracks"].as_array().unwrap();
        let clips:Vec<_>=tracks.iter().flat_map(|track|track["clips"].as_array().unwrap()).collect();
        let video=clips.iter().find(|clip|clip["id"]=="auto-first").unwrap();let audio=clips.iter().find(|clip|clip["id"]=="auto-first-audio").unwrap();
        assert_eq!(video["sourceAudioEnabled"],false);assert_eq!(video["placement"],audio["placement"]);
        for time in case["times"].as_array().unwrap() {
            let mut baseline=None;
            for version in ["after","reference"] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case[version],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"first-free: {}",String::from_utf8_lossy(&bytes));
                if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"first-free occupied{} at{time}",case["occupied"]);}else{baseline=Some(bytes);}
            }
        }
    }
    let trims = fixture["nativeTrims"].as_array().unwrap();
    assert_eq!(trims.len(), 11);
    let splits = fixture["nativeSplits"].as_array().unwrap();
    assert_eq!(splits.len(), 11);
    let heads = fixture["nativeHeads"].as_array().unwrap();
    assert_eq!(heads.len(), 11);
    let extensions = fixture["nativeExtendedHeads"].as_array().unwrap();
    assert_eq!(extensions.len(), 22);
    for (index, case) in trims.iter().chain(splits.iter()).chain(heads.iter()).chain(extensions.iter()).enumerate() {
        let (status, result, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":case["after"],"timeUs":2000000}))).await;
        assert_eq!(status, StatusCode::OK, "trim {index}: {result}");
        let before_links = &case["before"]["compositions"]["new-timeline"]["content"]["metadata"]["clipLinks"];
        let after_links = &case["after"]["compositions"]["new-timeline"]["content"]["metadata"]["clipLinks"];
        for (id, members) in before_links.as_object().unwrap() {
            for member in members.as_array().unwrap() { assert!(after_links[id].as_array().unwrap().contains(member)); }
        }
        for time in case["times"].as_array().unwrap() {
            let mut baseline = None;
            for version in ["before", "after"] {
                let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":case[version],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status, StatusCode::OK, "trim {index} {version} {time}: {}", String::from_utf8_lossy(&bytes));
                assert_eq!(bytes.len(), 16 * 16 * 4);
                if let Some(expected) = &baseline { assert_eq!(&bytes, expected, "trim {index} retained frame {time}"); }
                else { baseline = Some(bytes); }
            }
        }
    }
    let replacements = fixture["nativeReplacements"].as_array().unwrap();
    assert_eq!(replacements.len(), 13);
    let source_rebases = fixture["nativeSourceRebases"].as_array().unwrap();
    assert_eq!(source_rebases.len(), 13);
    for (index, case) in replacements.iter().chain(source_rebases.iter()).enumerate() {
        let (status, result, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":case["after"],"timeUs":2000000}))).await;
        assert_eq!(status, StatusCode::OK, "replace {index}: {result}");
        for time in case["times"].as_array().unwrap() {
            let mut baseline = None;
            for version in ["after", "reference"] {
                let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":case[version],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                if let Some(expected) = &baseline { assert_eq!(&bytes, expected, "replace {index}/{time}"); } else { baseline = Some(bytes); }
            }
            if time.as_u64() == Some(2250000) {
                let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":case["before"],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert_ne!(Some(bytes), baseline, "replacement {index} must actually change source picture");
            }
        }
    }
    let version_cases = fixture["nativeVersions"].as_array().unwrap();
    assert_eq!(version_cases.len(), 13);
    for (index, case) in version_cases.iter().enumerate() {
        assert_eq!(case["original"]["compositions"]["new-timeline"]["content"]["metadata"],case["copy"]["compositions"]["new-timeline"]["content"]["metadata"]);
        for (version, reference) in [("original","referenceOriginal"),("copy","referenceCopy")] {
            for time in case["times"].as_array().unwrap() {
                let mut baseline=None;
                for key in [version,reference] {
                    let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case[key],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                    let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                    assert_eq!(status,StatusCode::OK,"version{index}/{key}: {}",String::from_utf8_lossy(&bytes));
                    if let Some(expected)=&baseline {assert_eq!(&bytes,expected,"version{index}/{version}/{time}");}else{baseline=Some(bytes);}
                }
            }
        }
    }
    let duplicates=fixture["nativeDuplicates"].as_array().unwrap();assert_eq!(duplicates.len(),43);
    for (index,case) in duplicates.iter().enumerate() {
        let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":case["after"],"timeUs":case["start"]}))).await;
        assert_eq!(status,StatusCode::OK,"duplicate{index}: {result}");
        let delta=case["delta"].as_u64().unwrap();
        for time in case["times"].as_array().unwrap() {
            let time=time.as_u64().unwrap();let mut baseline=None;
            for (key,sample) in [("before",time),("after",time+delta),("reference",time+delta)] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case[key],"compositionId":case["compositionId"],"timeUs":sample,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"duplicate{index}/{key}/{sample}: {}",String::from_utf8_lossy(&bytes));assert_eq!(bytes.len(),16*16*4);
                if let Some(expected)=&baseline {assert_eq!(&bytes,expected,"duplicate{index}/{key}/{sample}");}else{baseline=Some(bytes);}
            }
        }
    }
    let cross_copies=fixture["nativeCrossCopies"].as_array().unwrap();assert_eq!(cross_copies.len(),29);
    let clipboard_pastes=fixture["nativeClipboardPastes"].as_array().unwrap();assert_eq!(clipboard_pastes.len(),32);
    let caption_pastes=fixture["nativeCaptionPastes"].as_array().unwrap();assert_eq!(caption_pastes.len(),5);
    for (index,case) in cross_copies.iter().chain(clipboard_pastes.iter()).chain(caption_pastes.iter()).enumerate() {
        let mut copied=case["before"].clone();
        if !case["moves"].as_array().unwrap().is_empty() {
            let (status,result,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":case["staged"],"moves":case["moves"]}))).await;
            assert_eq!(status,StatusCode::OK,"cross-copy{index}: {result}");assert_eq!(result["changed"],true);
            copied["compositions"][case["to"].as_str().unwrap()]=result["project"]["compositions"][case["to"].as_str().unwrap()].clone();
        }
        if case["captionTracksAfter"].is_array() {
            let node=&mut copied["compositions"][case["to"].as_str().unwrap()];node["content"]["captionTracks"]=case["captionTracksAfter"].clone();
            if let Some(end)=case["captionDurationUs"].as_u64(){node["durationUs"]=json!(end.max(node["durationUs"].as_u64().unwrap_or(0)));}
        }
        for (id,node) in case["before"]["compositions"].as_object().unwrap() {if id!=case["to"].as_str().unwrap(){assert_eq!(&copied["compositions"][id],node,"cross-copy must retain original timeline{id}");}}
        let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":copied,"timeUs":0}))).await;
        assert_eq!(status,StatusCode::OK,"cross-copy final{index}: {result}");
        for time in case["times"].as_array().unwrap() {
            let time=time.as_u64().unwrap();let mut baseline=None;
            for (project,composition,sample) in [(if case["reference"].is_object(){&case["reference"]}else{&case["before"]},&case["from"],time),(&copied,&case["to"],time+case["delta"].as_u64().unwrap())] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":project,"compositionId":composition,"timeUs":sample,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();assert_eq!(status,StatusCode::OK,"cross-copy{index}/{sample}: {}",String::from_utf8_lossy(&bytes));
                if case["captionTracksAfter"].is_array() && case["mixed"]==false && sample==2250000 {assert!(bytes.chunks_exact(4).any(|pixel|pixel[0]>0),"caption fixture must actually render visible text");}
                if let Some(expected)=&baseline {assert_eq!(&bytes,expected,"cross-copy{index}/{sample}");}else{baseline=Some(bytes);}
            }
        }
    }
    let deletions=fixture["nativeDeletions"].as_array().unwrap();assert_eq!(deletions.len(),4);
    for case in deletions {
        let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":case["after"],"timeUs":0}))).await;
        assert_eq!(status,StatusCode::OK,"delete {}: {result}",case["name"]);
        let content=&case["after"]["compositions"]["root"]["content"];
        assert!(content["tracks"][0]["transitions"].as_array().unwrap().is_empty());
        assert!(content["metadata"]["mediaVersions"].as_object().unwrap().is_empty());
        assert_eq!(content["metadata"]["clipNames"]["next"],"Next");
        for time in [4750000u64,7999999] {
            let mut baseline=None;
            for version in ["before","after"] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case[version],"compositionId":"root","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"delete {} {version}: {}",case["name"],String::from_utf8_lossy(&bytes));
                if let Some(baseline)=&baseline{assert_eq!(&bytes,baseline,"delete {} surviving frame{time}",case["name"]);}else{baseline=Some(bytes);}
            }
        }
        let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case["after"],"compositionId":"root","timeUs":1000000,"renderedOutputs":{}})),&token)).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);let bytes=response.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.chunks_exact(4).all(|pixel|pixel==[0,0,0,255]),"deleted picture must be absent");
    }
    for version in ["before","after"] {
        let project=&fixture["nativeDeleteCrossfade"][version];
        let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":project,"timeUs":4500000}))).await;
        assert_eq!(status,StatusCode::OK,"audio delete {version}: {result}");
    }
    assert_eq!(fixture["nativeDeleteCrossfade"]["after"]["compositions"]["root"]["content"]["tracks"][1]["clips"][0]["crossfadeInTicks"],0);
    // The incoming handle is needed before its nominal clip start, and remains private.
    let owner = state.db.register_auth_user("concat-private-owner", "shared test password").await.unwrap().unwrap();
    let space = state.db.create_space(&owner.user.username, "Concat transition handles").await.unwrap();
    state.db.create_space_composition_project("Private incoming source", &json!({"schemaVersion":1,"sources":{"next-source":{"id":"next-source"}}}), &["next-source".into()], &owner.user.username, &space.id).await.unwrap();
    for (version, expected) in [("off", StatusCode::OK), ("on", StatusCode::FORBIDDEN)] {
        let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":8000000,"content":versions[version],"instances":[]}}});
        let body = json!({"project":project,"timeUs":1_500_000,"renderedOutputs":{}});
        let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", body.clone()), &token)).await.unwrap();
        assert_eq!(response.status(), expected, "version privacy {version}");
        let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", body), &owner.token)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "version owner {version}");
    }
    let implicit = cases.iter().find(|case| case["name"] == "implicit").unwrap();
    for version in ["original", "split", "joined"] {
        let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":8000000,"content":implicit[version],"instances":[]}}});
        for (time, expected) in [(3_499_999, StatusCode::OK), (3_500_000, StatusCode::FORBIDDEN)] {
            let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":project,"timeUs":time,"renderedOutputs":{}})), &token)).await.unwrap();
            assert_eq!(response.status(), expected, "incoming privacy {version}/{time}");
        }
        let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":project,"timeUs":3_500_000,"renderedOutputs":{}})), &owner.token)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "incoming owner {version}");
    }
}

#[tokio::test]
async fn timeline_order_native_frame_route_preserves_plan_and_rejects_invalid_orders() {
    use video_kadr_backend::domain::composition::{CanvasSpec, Composition, TransformSpec};
    let (state, _directory) = make_state(false, false).await;
    let app = router(state);
    let content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    let mut project = json!({"schemaVersion":1,"root":"root","compositions":{
        "root":{"durationUs":1000000,"content":content,"instances":[{"id":"placed","timing":{"composition":"child","timelineStartUs":0,"sourceInUs":0,"sourceOutUs":1000000},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]},
        "child":{"durationUs":1000000,"content":content,"instances":[]}
    }});
    let (status, baseline, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":project,"timeUs":500000}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(baseline["entries"].as_array().unwrap().len(), 2);
    project["timelineOrder"] = json!(["root", "child"]);
    let (status, ordered, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":project,"timeUs":500000}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ordered, baseline);
    for order in [json!(["root"]), json!(["root","root"]), json!(["root","missing"])] {
        project["timelineOrder"] = order;
        let (status, _, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":project,"timeUs":500000}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn frontend_created_timeline_is_native_valid_and_previews_empty_without_media_bindings() {
    let (state, _directory) = make_state(false, false).await;
    let app = router(state);
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let project = &fixture["createdTimeline"];
    assert_eq!(project["timelineOrder"], json!(["root", "new-timeline"]));
    let content = &project["compositions"]["new-timeline"]["content"];
    assert_eq!(content["canvas"], project["compositions"]["root"]["content"]["canvas"]);
    assert_eq!(content["canvas"]["fpsRate"], json!({"numerator":30000,"denominator":1001}));
    assert_eq!(content["tracks"].as_array().unwrap().len(), 4);
    let (status, _, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":project,"timeUs":0}))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, durations, _) = send(&app, post_json("/api/compositions/nested-durations", json!({"project":project}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(durations["new-timeline"], 0);
    assert_eq!(durations["root"], 8_000_000);
    let token = register_token(&app, "new-timeline-owner").await;
    let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":project,"compositionId":"new-timeline","timeUs":0,"renderedOutputs":{}})), &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.len(), 16 * 16 * 4);
    assert!(bytes.iter().all(|byte| *byte == 0), "empty timeline must be transparent at its zero duration, without decoding inactive media");
}

#[tokio::test]
async fn frontend_removed_timelines_keep_valid_roots_and_requested_parent_placements() {
    let (state, _directory) = make_state(false, false).await;
    let app = router(state);
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let removed = &fixture["removedTimelines"];
    assert_eq!(removed["invalidatedIds"], json!(["target", "parent", "root"]));
    let root = &removed["root"];
    assert_eq!(root["root"], "new-timeline");
    assert_eq!(root["timelineOrder"], json!(["new-timeline"]));
    let (status, frame, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":root,"timeUs":0}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(frame["entries"].as_array().unwrap().len(), 1);
    assert_eq!(frame["entries"][0]["composition"], "new-timeline");
    let token = register_token(&app, "removed-timeline-owner").await;
    let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":root,"timeUs":0,"renderedOutputs":{}})), &token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.len(), 16 * 16 * 4);
    assert!(bytes.iter().all(|byte| *byte == 0));
    let dependent = &removed["dependent"];
    assert!(dependent["compositions"].get("target").is_none());
    assert!(dependent["compositions"]["parent"]["instances"].as_array().unwrap().is_empty());
    assert_eq!(dependent["compositions"]["root"]["instances"][0]["timing"]["composition"], "parent");
    let (status, frame, _) = send(&app, post_json("/api/compositions/nested-frame", json!({"project":dependent,"timeUs":0}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(frame["entries"].as_array().unwrap().iter().map(|entry| entry["composition"].as_str().unwrap()).collect::<Vec<_>>(), ["root", "parent"]);
    let (status, durations, _) = send(&app, post_json("/api/compositions/nested-durations", json!({"project":dependent}))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(durations.get("target").is_none());
    assert_eq!(durations["root"], 8_000_000);
    assert_eq!(durations["parent"], 1_000_000);
}


#[tokio::test]
async fn native_move_clips_route_refuses_invalid_batches_and_reports_noop() {
    let (state,_directory)=make_state(false,false).await;
    let app=router(state);
    let fixture:Value=serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let project=&fixture["createdTimeline"];
    let first=json!({"fromComposition":"root","fromTrackId":"video","clipId":"picture","toComposition":"new-timeline","toTrackId":"video-1","startUs":2000000});
    let (status,changed,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":project,"moves":[first.clone()]}))).await;
    assert_eq!(status,StatusCode::OK,"{changed}");
    assert_eq!(changed["changed"],true);
    let mut same=first.clone();same["toComposition"]=json!("root");same["toTrackId"]=json!("video");same["startUs"]=json!(0);
    let (status,unchanged,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":project,"moves":[same]}))).await;
    assert_eq!(status,StatusCode::OK);assert_eq!(unchanged["changed"],false);
    for mode in 0..5 {
        let mut document=project.clone();let mut item=first.clone();
        let moves=match mode {
            0=>json!([]),
            1=>json!([item.clone(),item.clone()]),
            2=>{document["compositions"]["root"]["content"]["tracks"][0]["locked"]=json!(true);json!([item])},
            3=>{item["toTrackId"]=json!("audio-1");json!([item])},
            _=>{item["startUs"]=json!(9007199254740992u64);json!([item])},
        };
        let (status,_,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":document,"moves":moves}))).await;
        assert_eq!(status,StatusCode::BAD_REQUEST,"move refusal {mode}");
    }
}

#[tokio::test]
async fn moved_native_visual_variants_keep_real_preview_pixels() {
    use video_kadr_backend::domain::{composition::TransformSpec,graphic_shape::GraphicShape};
    let (mut state,_directory)=make_state(true,true).await;
    std::sync::Arc::make_mut(&mut state.tools).ffmpeg_encoders.push("ffv1".into());
    std::sync::Arc::make_mut(&mut state.tools).ffmpeg_muxers.push("matroska".into());
    let path=state.sources_dir().join("move-still.png");
    let encoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=c=0x29b573:s=4x4","-frames:v","1","-threads","1"]).arg(&path).output().unwrap();
    assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"still","filename":"move-still.png","url":"/files/sources/move-still.png","mediaType":"image"}))).await);
    let app=router(state.clone());
    let token=register_token(&app,"concat-visual-move-owner").await;
    let fixture:Value=serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let insertion=fixture["nativeInsertions"].as_array().unwrap().iter().find(|case|case["kind"]=="image").unwrap();
    for time in insertion["times"].as_array().unwrap() {
        let mut baseline=None;
        for version in ["inserted","reference"] {
            let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":insertion[version],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
            let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(status,StatusCode::OK,"image insertion: {}",String::from_utf8_lossy(&bytes));
            if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"image insertion at{time}");}else{baseline=Some(bytes);}
        }
    }
    for kind in ["image","color","graphic","text"] {
        let mut project=fixture["createdTimeline"].clone();
        // Compare the clip's end inside both canvases, not an ended composition against a live one.
        project["compositions"]["new-timeline"]["durationUs"]=json!(12000000);
        let mut clip=json!({"id":"visual","timelineStartTick":1000000,"timelineEndTick":3000000,"transform":TransformSpec::default(),"opacity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"ease_in_out","keyframes":[{"tick":0,"value":0.2},{"tick":2000000,"value":0.9}]}},"enabled":true});
        match kind {
            "image"=>{
                clip.as_object_mut().unwrap().remove("timelineEndTick");clip["durationTicks"]=json!(2000000);clip["sourceId"]=json!("still");clip["blendMode"]=json!("normal");
                project["compositions"]["root"]["content"]["sources"]=json!({"still":{"id":"still","kind":"image","durationTicks":0,"width":4,"height":4,"hasAudio":false}});
            },
            "text"=>{clip["text"]=json!("T");clip["style"]=json!({"fontFamily":"Noto Sans","fontSize":8,"color":{"red":0.2,"green":0.8,"blue":0.4,"alpha":1},"background":{"red":0.1,"green":0.1,"blue":0.3,"alpha":0.6},"stroke":{"red":1,"green":0,"blue":0,"alpha":1},"strokeWidth":1,"shadow":{"red":0,"green":0,"blue":1,"alpha":0.7},"shadowX":2,"shadowY":1});},
            "color"=>{clip["width"]=json!(16);clip["height"]=json!(16);clip["color"]=json!({"r":29,"g":181,"b":73,"a":221});},
            _=>{clip["width"]=json!(16);clip["height"]=json!(16);clip["animationOffsetUs"]=json!(250000);clip["graphic"]=json!({"layers":[{"uid":7,"enabled":true,"content":{"kind":"shape","parameters":GraphicShape::default()}}]});}
        }
        if kind=="text" {
            clip["colorAnimation"]=json!({"keys":[{"t_us":0,"value":{"r":255,"g":0,"b":0,"a":255},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}},{"t_us":2000000,"value":{"r":0,"g":0,"b":255,"a":128},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}});
            for id in ["root","new-timeline"] {project["compositions"][id]["content"]["canvas"]["fpsMilli"]=json!(25000);project["compositions"][id]["content"]["canvas"]["fpsRate"]=json!({"numerator":25,"denominator":1});}
        }
        project["compositions"]["root"]["content"]["tracks"]=json!([{"kind":kind,"id":"visual-track","name":"Visual","clips":[clip]}]);
        let moves=json!([{"fromComposition":"root","fromTrackId":"visual-track","clipId":"visual","toComposition":"new-timeline","toTrackId":"moved-visual","startUs":5000000,"createTrack":true}]);
        let (status,moved,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":project,"moves":moves}))).await;
        assert_eq!(status,StatusCode::OK,"{kind}: {moved}");
        let mut visible=false;
        for delta in [0u64,1,250000,999999,1750000,1999999,2000000] {
            let mut expected=None;
            for (document,id,start) in [(&project,"root",1000000),(&moved["project"],"new-timeline",5000000)] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":document,"compositionId":id,"timeUs":start+delta,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"{kind} delta{delta}: {}",String::from_utf8_lossy(&bytes));assert_eq!(bytes.len(),16*16*4);
                if let Some(expected)=&expected {assert_eq!(&bytes,expected,"{kind} delta{delta}");}else{if delta<2000000 {visible|=bytes.chunks_exact(4).any(|p|p[0]!=0||p[1]!=0||p[2]!=0);}expected=Some(bytes);}
            }
        }
        assert!(visible,"{kind} must render visible content");
        if kind=="text" {
            let (status,accepted,_)=send(&app,bearer(post_json("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_only","project":moved["project"],"compositionId":"new-timeline","startUs":5250000,"endUs":5450000,"renderedOutputs":{}})),&token)).await;
            assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");
            let done=poll_terminal(&app,accepted["jobId"].as_str().unwrap()).await;
            assert_eq!(done["status"],"done","{done}");
            let path=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
            let decoded=std::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().unwrap();
            assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));
            assert_eq!(decoded.stdout.len(),16*16*4*5);
            for (index,frame) in decoded.stdout.chunks_exact(16*16*4).enumerate() {
                let time=5250000+index as u64*40000;
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":moved["project"],"compositionId":"new-timeline","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                assert_eq!(response.status(),StatusCode::OK);
                let preview=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(frame,preview.as_ref(),"exported animated text frame{index}");
            }
            assert_ne!(&decoded.stdout[..16*16*4],&decoded.stdout[16*16*4*4..],"animated export must change");
        }
    }
}

#[tokio::test]
async fn frontend_native_track_commands_validate_all_lane_kinds_and_flags() {
    let (state,_directory)=make_state(false,false).await;
    let app=router(state);
    let fixture:Value=serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let cases=fixture["nativeTracks"].as_array().unwrap();assert_eq!(cases.len(),6);
    for case in cases {
        for version in ["added","removed"] {
            let project=&case[version];
            let (status,result,_)=send(&app,post_json("/api/compositions/nested-frame",json!({"project":project,"timeUs":0}))).await;
            assert_eq!(status,StatusCode::OK,"{} {version}: {result}",case["trackKind"]);
            let tracks=project["compositions"]["new-timeline"]["content"]["tracks"].as_array().unwrap();
            if version=="added" {let track=tracks.last().unwrap();assert_eq!(track["kind"],case["trackKind"]);assert_eq!(track["name"],"Renamed lane");assert_eq!(track["locked"],true);for flag in ["hidden","muted","solo"] {if let Some(value)=track.get(flag){assert_eq!(value,true);}}}
            else {assert!(!tracks.iter().any(|track|track["id"]=="manual-lane"));}
        }
    }
}

#[tokio::test]
async fn frontend_track_removal_rebinds_effect_layers_without_changing_pixels() {
    let (state,_directory)=make_state(true,true).await;
    let raw=state.sources_dir().join("rebind.rgba");
    let rgba:Vec<u8>=(0..16u8).flat_map(|i|[i.wrapping_mul(53),255-i*13,i*11,255]).collect();
    std::fs::write(&raw,rgba).unwrap();
    let png=state.sources_dir().join("rebind.png");
    let encoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","rawvideo","-pix_fmt","rgba","-s","4x4","-i"]).arg(&raw).args(["-frames:v","1","-threads","1"]).arg(&png).output().unwrap();
    assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"still","filename":"rebind.png","url":"/files/sources/rebind.png","mediaType":"image"}))).await);
    let app=router(state);let token=register_token(&app,"rebind-owner").await;
    let fixture:Value=serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let base=fixture["nativeRebinds"].as_array().unwrap();assert_eq!(base.len(),3);
    let animation=json!({"keys":[{"t_us":0,"value":{"r":255,"g":255,"b":255,"a":192},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}},{"t_us":2000000,"value":{"r":0,"g":0,"b":0,"a":64},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}});
    let mut cases=base.clone();
    for original in base {
        let mut case=original.clone();case["animated"]=json!(true);
        for version in ["before","after"] {
            let control=case[version]["compositions"]["root"]["content"]["tracks"].as_array_mut().unwrap().iter_mut().find(|track|track["id"]=="control").unwrap();
            control["clips"][0]["colorAnimation"]=animation.clone();
        }
        cases.push(case);
    }
    assert_eq!(cases.len(),6);
    for case in &cases {
        let mut active=false;
        for time in [0u64,500000,1999999] {
            let mut baseline=None;
            for version in ["before","after"] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":case[version],"compositionId":"root","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"{} {version}: {}",case["kind"],String::from_utf8_lossy(&bytes));
                assert_eq!(bytes.len(),16*16*4);
                if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"{} at{time}",case["kind"]);}else{baseline=Some(bytes);}
            }
            let mut plain=case["before"].clone();plain["compositions"]["root"]["content"]["tracks"][1]["clips"][0]["effects"]=json!([]);
            let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":plain,"compositionId":"root","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
            assert_eq!(response.status(),StatusCode::OK);let bytes=response.into_body().collect().await.unwrap().to_bytes();
            let baseline=baseline.unwrap();active|=bytes!=baseline;
            if !case["animated"].as_bool().unwrap_or(false) {assert_ne!(bytes,baseline,"{} must actively alter pixels",case["kind"]);}
        }
        assert!(active,"{} must actively alter at least one sampled frame",case["kind"]);
        let moves=json!([
            {"fromComposition":"root","fromTrackId":"dependent","clipId":"picture","toComposition":"new-timeline","toTrackId":"new-picture","startUs":4000000,"createTrack":true},
            {"fromComposition":"root","fromTrackId":"control","clipId":"control-clip","toComposition":"new-timeline","toTrackId":"new-control","startUs":4000000,"createTrack":true}
        ]);
        let (status,moved,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":case["before"],"moves":moves}))).await;
        assert_eq!(status,StatusCode::OK,"{} dependency move: {moved}",case["kind"]);
        for time in [0u64,500000,1999999] {
            let mut baseline=None;
            for (document,id,time) in [(&case["before"],"root",time),(&moved["project"],"new-timeline",time+4000000)] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":document,"compositionId":id,"timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"{} moved frame: {}",case["kind"],String::from_utf8_lossy(&bytes));
                if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"{} moved at{time}",case["kind"]);}else{baseline=Some(bytes);}
            }
        }
        let mut empty=case["before"].clone();empty["compositions"]["root"]["content"]["tracks"][2]["clips"]=json!([]);
        let (status,moved,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":empty,"moves":[{"fromComposition":"root","fromTrackId":"dependent","clipId":"picture","toComposition":"new-timeline","toTrackId":"new-picture","startUs":4000000,"createTrack":true}]}))).await;
        assert_eq!(status,StatusCode::OK,"{} empty control: {moved}",case["kind"]);
        for time in [0u64,500000,1999999] {
            let mut baseline=None;
            for (document,id,time) in [(&empty,"root",time),(&moved["project"],"new-timeline",time+4000000)] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":document,"compositionId":id,"timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"{} empty control: {}",case["kind"],String::from_utf8_lossy(&bytes));
                if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"{} empty control at{time}",case["kind"]);}else{baseline=Some(bytes);}
            }
        }
        let moves=json!([
            {"fromComposition":"root","fromTrackId":"dependent","clipId":"picture","toComposition":"root","toTrackId":"empty","startUs":4000000},
            {"fromComposition":"root","fromTrackId":"control","clipId":"control-clip","toComposition":"root","toTrackId":"new-control","startUs":4000000,"createTrack":true}
        ]);
        let (status,moved,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":case["before"],"moves":moves}))).await;
        assert_eq!(status,StatusCode::OK,"{} local control: {moved}",case["kind"]);
        for time in [0u64,500000,1999999] {
            let mut baseline=None;
            for (document,time) in [(&case["before"],time),(&moved["project"],time+4000000)] {
                let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":document,"compositionId":"root","timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
                let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status,StatusCode::OK,"{} local control: {}",case["kind"],String::from_utf8_lossy(&bytes));
                if let Some(baseline)=&baseline {assert_eq!(&bytes,baseline,"{} local control at{time}",case["kind"]);}else{baseline=Some(bytes);}
            }
        }
    }
}

#[tokio::test]
async fn first_free_video_and_companion_audio_export_exact_source_clock_without_double_sound() {
    use std::process::Command;
    let (mut state,_directory)=make_state(true,false).await;
    // This render matrix submits more than the production per-window job allowance.
    // Queue admission has separate regressions; keep this fixture focused on media conformance.
    let mut workload = video_kadr_backend::config::WorkloadConfig::default();
    workload.queue_limits.max_new_jobs = 512;
    state = state.with_workload_config(workload);
    let tools=std::sync::Arc::make_mut(&mut state.tools);tools.ffmpeg_encoders.extend(["ffv1".into(),"pcm_f32le".into()]);tools.ffmpeg_muxers.push("matroska".into());
    let frames=480000u32;let bytes=frames*4;
    let mut wav=Vec::new();wav.extend(b"RIFF");wav.extend((36+bytes).to_le_bytes());wav.extend(b"WAVEfmt ");wav.extend(16u32.to_le_bytes());wav.extend(1u16.to_le_bytes());wav.extend(2u16.to_le_bytes());wav.extend(48000u32.to_le_bytes());wav.extend(192000u32.to_le_bytes());wav.extend(4u16.to_le_bytes());wav.extend(16u16.to_le_bytes());wav.extend(b"data");wav.extend(bytes.to_le_bytes());
    for frame in 0..frames {let sample=if frame<48000 {4096i16}else if frame<52800 {8192i16}else if (105600..110400).contains(&frame) {12288i16}else{16384i16};wav.extend(sample.to_le_bytes());wav.extend((-sample).to_le_bytes());}
    let wav_path=state.sources_dir().join("automatic-audio.wav");std::fs::write(&wav_path,wav).unwrap();
    let movie=state.sources_dir().join("automatic-video.mov");
    let encoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=c=blue:s=4x4:r=10:d=10,format=rgba","-i"]).arg(&wav_path).args(["-c:v","qtrle","-pix_fmt","argb","-c:a","pcm_s16le","-threads","1","-shortest"]).arg(&movie).output().unwrap();
    assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"automatic-video.mov","url":"/files/sources/automatic-video.mov","mediaType":"video"}))).await);
    let replacement_movie=state.sources_dir().join("replacement-video.mov");
    let replacement_encoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=c=red:s=4x4:r=10:d=10,format=rgba","-i"]).arg(&wav_path).args(["-filter:a","volume=0.5","-c:v","qtrle","-pix_fmt","argb","-c:a","pcm_s16le","-threads","1","-shortest"]).arg(&replacement_movie).output().unwrap();
    assert!(replacement_encoded.status.success(),"{}",String::from_utf8_lossy(&replacement_encoded.stderr));
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"next-source","filename":"replacement-video.mov","url":"/files/sources/replacement-video.mov","mediaType":"video"}))).await);
    let app=router(state.clone());let token=register_token(&app,"automatic-av-owner").await;
    let uploaded_movie=state.sources_dir().join("upload-distinct-video.mov");
    let uploaded_encoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=c=lime:s=4x4:r=10:d=10,format=rgba","-i"]).arg(&wav_path).args(["-filter:a","volume=0.25","-c:v","qtrle","-pix_fmt","argb","-c:a","pcm_s16le","-threads","1","-shortest"]).arg(&uploaded_movie).output().unwrap();
    assert!(uploaded_encoded.status.success(),"{}",String::from_utf8_lossy(&uploaded_encoded.stderr));
    let boundary="NATIVE_REPLACEMENT_UPLOAD";
    let mut upload_body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"local-replacement.mov\"\r\nContent-Type: video/quicktime\r\n\r\n").into_bytes();
    upload_body.extend(std::fs::read(&uploaded_movie).unwrap());
    upload_body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request=Request::builder().method("POST").uri("/api/upload").header("content-type",format!("multipart/form-data; boundary={boundary}")).body(Body::from(upload_body)).unwrap();
    let (status,uploaded,_)=send(&app,bearer(request,&token)).await;
    assert_eq!(status,StatusCode::OK,"real replacement upload: {uploaded}");
    assert_eq!(uploaded["mediaType"],"video");assert_eq!(uploaded["width"],4);assert_eq!(uploaded["height"],4);assert_eq!(uploaded["acodec"],"pcm_s16le");
    let uploaded_id=uploaded["id"].as_str().unwrap();
    let fixture:Value=serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let mut project=fixture["nativeFirstFree"][0]["after"].clone();
    project["compositions"]["new-timeline"]["content"]["canvas"]["fpsMilli"]=json!(25000);project["compositions"]["new-timeline"]["content"]["canvas"]["fpsRate"]=json!({"numerator":25,"denominator":1});
    let mut destination=project["compositions"]["new-timeline"].clone();
    for track in destination["content"]["tracks"].as_array_mut().unwrap(){track["clips"]=json!([]);}
    destination["content"]["sources"]=json!({});destination["content"]["metadata"]["clipLinks"]=json!({});
    project["compositions"]["destination"]=destination;project["timelineOrder"].as_array_mut().unwrap().push(json!("destination"));
    let first=json!({"fromComposition":"new-timeline","fromTrackId":"video-2","clipId":"auto-first","toComposition":"destination","toTrackId":"video-2","startUs":4000000});
    let second=json!({"fromComposition":"new-timeline","fromTrackId":"audio-2","clipId":"auto-first-audio","toComposition":"destination","toTrackId":"audio-2","startUs":4000000});
    let (status,_,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":project,"moves":[first.clone()]}))).await;assert_eq!(status,StatusCode::BAD_REQUEST);
    let (status,moved,_)=send(&app,post_json("/api/compositions/nested-move-clips",json!({"project":project,"moves":[first,second]}))).await;assert_eq!(status,StatusCode::OK,"{moved}");
    assert_eq!(moved["project"]["compositions"]["destination"]["content"]["metadata"]["clipLinks"]["auto-first-link"],json!(["auto-first","auto-first-audio"]));
    project=moved["project"].clone();
    let (status,accepted,_)=send(&app,bearer(post_json("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":"destination","startUs":4000000,"endUs":4200000,"renderedOutputs":{}})),&token)).await;
    assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");
    let done=poll_terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");assert_eq!(done["result"]["withAudio"],true);
    let output=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
    let decoded=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(decoded.status.success());
    let mut expected=Vec::new();for frame in 0..9600 {let sample=if frame<4800{0.25f32}else{0.5f32};expected.extend(sample.to_le_bytes());expected.extend((-sample).to_le_bytes());}
    assert_eq!(decoded.stdout,expected,"exact stereo samples must start at source1s with one audio owner");
    let decoded=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(decoded.status.success());
    assert_eq!(decoded.stdout,[0,0,255,255].repeat(16*16*5));
    for family in ["nativeSplits", "nativeHeads", "nativeExtendedHeads"] {
    let indices = if family == "nativeExtendedHeads" { vec![0usize,1,4,5,8,9,12,13,16,17,18,19] } else { vec![0usize,2,4,6,8,9] };
    for index in indices {
        let start = if family == "nativeExtendedHeads" && index%2==1 {1850000u64} else {3150000u64};
        let case = &fixture[family][index];
        let mut exported = Vec::new();
        for version in ["before", "after"] {
            let mut split = case[version].clone();
            split["compositions"]["new-timeline"]["content"]["canvas"]["fpsMilli"] = json!(25000);
            split["compositions"]["new-timeline"]["content"]["canvas"]["fpsRate"] = json!({"numerator":25,"denominator":1});
            let (status, accepted, _) = send(&app, bearer(post_json("/api/compositions/nested-export", json!({"schemaVersion":1,"mode":"video_audio","project":split,"compositionId":"new-timeline","startUs":start,"endUs":start+200000,"renderedOutputs":{}})), &token)).await;
            assert_eq!(status, StatusCode::ACCEPTED, "split {index} {version}: {accepted}");
            let done = poll_terminal(&app, accepted["jobId"].as_str().unwrap()).await;
            assert_eq!(done["status"], "done", "split {index} {version}: {done}");
            let path = state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
            let audio = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
            assert!(audio.status.success()); assert_eq!(audio.stdout.len(), 9600*2*4);
            if family == "nativeExtendedHeads" && matches!(index, 17 | 19) {
                let silent = audio.stdout.chunks_exact(8).take(7200).position(|frame| f32::from_le_bytes(frame[..4].try_into().unwrap()).abs() < 0.05);
                assert!(silent.is_none(), "slow extended head must not invent silence: case{index} {version} firstquiet{silent:?}");
            }
            if index == 0 || (family == "nativeExtendedHeads" && index == 1) {
                let mut expected = Vec::new();
                for frame in 0..9600 { let sample = if start==1850000 {if frame<7200 {0.125f32}else{0.25f32}} else if (2400..7200).contains(&frame) {0.375f32} else {0.5f32}; expected.extend(sample.to_le_bytes()); expected.extend((-sample).to_le_bytes()); }
                assert_eq!(audio.stdout, expected, "independent split source clock {version}");
            }
            let video = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();
            assert!(video.status.success()); assert_eq!(video.stdout.len(), 16*16*4*5);
            exported.push((audio.stdout, video.stdout));
        }
        assert_eq!(exported[0], exported[1], "split {index} actual PCM/picture continuity across cut");
    }
    }
    for index in [0usize,2,4,6,8,9,12,13,29,31,33,35,37,38,41,42] {
        let case=&fixture["nativeDuplicates"][index];let start=case["start"].as_u64().unwrap();let mut outputs=Vec::new();
        for version in ["after","reference"] {
            let mut project=case[version].clone();
            project["compositions"]["new-timeline"]["content"]["canvas"]["fpsMilli"]=json!(25000);project["compositions"]["new-timeline"]["content"]["canvas"]["fpsRate"]=json!({"numerator":25,"denominator":1});
            let (status,accepted,_)=send(&app,bearer(post_json("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":"new-timeline","startUs":start,"endUs":start+200000,"renderedOutputs":{}})),&token)).await;
            assert_eq!(status,StatusCode::ACCEPTED,"duplicate{index}/{version}: {accepted}");
            let done=poll_terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","duplicate{index}/{version}: {done}");
            let path=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
            let audio=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(audio.status.success());assert_eq!(audio.stdout.len(),9600*2*4);
            if index==0 || index==29 {let mut expected=Vec::new();for frame in 0..9600 {let sample=if frame<4800 {0.25f32}else{0.5f32};expected.extend(sample.to_le_bytes());expected.extend((-sample).to_le_bytes());}assert_eq!(audio.stdout,expected,"duplicate independent source1s/no doubled audio");}
            let video=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(video.status.success());assert_eq!(video.stdout.len(),16*16*4*5);
            outputs.push((audio.stdout,video.stdout));
        }
        assert_eq!(outputs[0],outputs[1],"duplicate{index} must match independently shifted authored reference on actual sample lattice");
    }
    for family in ["nativeCrossCopies","nativeClipboardPastes"] {
    for index in [0usize,2,4,6,8,9,12,13] {
        let case=&fixture[family][index];let destination=case["to"].as_str().unwrap();
        let (status,result,_)=send(&app,bearer(post_json("/api/compositions/nested-move-clips",json!({"project":case["staged"],"moves":case["moves"]})),&token)).await;
        assert_eq!(status,StatusCode::OK,"cross-copy export preparation{index}: {result}");
        let mut copied=case["before"].clone();copied["compositions"][destination]=result["project"]["compositions"][destination].clone();
        let mut outputs=Vec::new();
        for (mut project,composition) in [(copied,destination),(fixture["nativeDuplicates"][29+index]["reference"].clone(),"new-timeline")] {
            project["compositions"][composition]["content"]["canvas"]["fpsMilli"]=json!(25000);project["compositions"][composition]["content"]["canvas"]["fpsRate"]=json!({"numerator":25,"denominator":1});
            let (status,accepted,_)=send(&app,bearer(post_json("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":composition,"startUs":8000000,"endUs":8200000,"renderedOutputs":{}})),&token)).await;
            assert_eq!(status,StatusCode::ACCEPTED,"cross-copy export{index}: {accepted}");
            let done=poll_terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","cross-copy export{index}: {done}");
            let path=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
            let audio=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(audio.status.success());assert_eq!(audio.stdout.len(),9600*2*4);
            let video=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(video.status.success());assert_eq!(video.stdout.len(),16*16*4*5);
            outputs.push((audio.stdout,video.stdout));
        }
        assert_eq!(outputs[0],outputs[1],"{family}{index} PCM/RGBA must match independently shifted original on the same sample lattice");
    }
    }
    for case in fixture["nativeCaptionPastes"].as_array().unwrap() {
        let mixed=case["mixed"].as_bool().unwrap();let mut copied=case["before"].clone();
        if !case["moves"].as_array().unwrap().is_empty(){
            let(status,result,_)=send(&app,bearer(post_json("/api/compositions/nested-move-clips",json!({"project":case["staged"],"moves":case["moves"]})),&token)).await;
            assert_eq!(status,StatusCode::OK,"caption AV preparation: {result}");copied["compositions"]["new-timeline"]=result["project"]["compositions"]["new-timeline"].clone();
        }
        let node=&mut copied["compositions"]["new-timeline"];node["content"]["captionTracks"]=case["captionTracksAfter"].clone();
        node["durationUs"]=json!(node["durationUs"].as_u64().unwrap_or(0).max(case["captionDurationUs"].as_u64().unwrap()));
        for start in if mixed {vec![8250000u64,9250000]}else{vec![8250000u64]} {
            let mut outputs=Vec::new();
            for mut project in [copied.clone(),case["shiftedReference"].clone()] {
                project["compositions"]["new-timeline"]["content"]["canvas"]["fpsMilli"]=json!(25000);project["compositions"]["new-timeline"]["content"]["canvas"]["fpsRate"]=json!({"numerator":25,"denominator":1});
                let(status,accepted,_)=send(&app,bearer(post_json("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":if mixed{"video_audio"}else{"video_only"},"project":project,"compositionId":"new-timeline","startUs":start,"endUs":start+200000,"renderedOutputs":{}})),&token)).await;
                assert_eq!(status,StatusCode::ACCEPTED,"caption export: {accepted}");let done=poll_terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","caption export: {done}");
                let path=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
                let video=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(video.status.success());assert_eq!(video.stdout.len(),16*16*4*5);
                if !mixed {assert!(video.stdout.chunks_exact(4).any(|pixel|pixel[0]>0),"caption export must contain rendered text");}
                let audio=if mixed {
                    let audio=Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(audio.status.success());assert_eq!(audio.stdout.len(),9600*2*4);
                    if start==9250000 {let mut expected=Vec::new();for _ in 0..9600 {expected.extend(0.5f32.to_le_bytes());expected.extend((-0.5f32).to_le_bytes());}assert_eq!(audio.stdout,expected,"mixed clipboard common source clock/no doubled sound");}audio.stdout
                }else{Vec::new()};
                outputs.push((video.stdout,audio));
            }
            assert_eq!(outputs[0],outputs[1],"caption/mixed export must match independently shifted authored original");
        }
    }
    for (version, first, second, channel) in [("original",0.25f32,0.5f32,2usize),("copy",0.125f32,0.25f32,0usize),("uploaded",0.0625f32,0.125f32,1usize),("source-shifted",0.25f32,0.25f32,0usize)] {
    let mut replacement = if version=="source-shifted" {fixture["nativeSourceRebases"][0]["after"].clone()}else if version=="uploaded" {fixture["nativeReplacements"][0]["after"].clone()}else{fixture["nativeVersions"][0][version].clone()};
    if version=="uploaded" {
        let content=&mut replacement["compositions"]["new-timeline"]["content"];
        let mut source=content["sources"]["next-source"].clone();source["id"]=json!(uploaded_id);content["sources"][uploaded_id]=source;
        for track in content["tracks"].as_array_mut().unwrap() { for clip in track["clips"].as_array_mut().unwrap() {if clip["sourceId"]=="next-source" {clip["sourceId"]=json!(uploaded_id);}} }
    }
    replacement["compositions"]["new-timeline"]["content"]["canvas"]["fpsMilli"] = json!(25000);
    replacement["compositions"]["new-timeline"]["content"]["canvas"]["fpsRate"] = json!({"numerator":25,"denominator":1});
    let (status, accepted, _) = send(&app, bearer(post_json("/api/compositions/nested-export", json!({"schemaVersion":1,"mode":"video_audio","project":replacement,"compositionId":"new-timeline","startUs":2000000,"endUs":2200000,"renderedOutputs":{}})), &token)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "replace export: {accepted}");
    let done = poll_terminal(&app, accepted["jobId"].as_str().unwrap()).await;
    assert_eq!(done["status"], "done", "replace export: {done}");
    let path = state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
    let audio = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
    assert!(audio.status.success());
    let mut expected_audio = Vec::new();
    for frame in 0..9600 { let sample = if frame<4800 {first}else{second}; expected_audio.extend(sample.to_le_bytes()); expected_audio.extend((-sample).to_le_bytes()); }
    assert_eq!(audio.stdout, expected_audio, "replacement must use new source audio at preserved source1s, exactly once");
    let video = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();
    assert!(video.status.success()); assert_eq!(video.stdout.len(),16*16*4*5);
    assert!(video.stdout.chunks_exact(4).all(|pixel| pixel[channel]>0 && (0..3).all(|other| other==channel || pixel[other]==0) && pixel[3]==255),"replacement must show red source rather than old blue");
    }
    let mut trimmed = fixture["nativeTrims"][0]["after"].clone();
    trimmed["compositions"]["new-timeline"]["content"]["canvas"]["fpsMilli"] = json!(25000);
    trimmed["compositions"]["new-timeline"]["content"]["canvas"]["fpsRate"] = json!({"numerator":25,"denominator":1});
    for start in [2000000u64, 4300000] {
        let (status, accepted, _) = send(&app, bearer(post_json("/api/compositions/nested-export", json!({"schemaVersion":1,"mode":"video_audio","project":trimmed,"compositionId":"new-timeline","startUs":start,"endUs":start+200000,"renderedOutputs":{}})), &token)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "trim export: {accepted}");
        let done = poll_terminal(&app, accepted["jobId"].as_str().unwrap()).await;
        assert_eq!(done["status"], "done", "trim export: {done}");
        let path = state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
        let audio = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
        assert!(audio.status.success());
        let mut expected_audio = Vec::new();
        for frame in 0..9600 {
            let sample = if start == 2000000 && frame < 4800 { 0.25f32 } else { 0.5f32 };
            expected_audio.extend(sample.to_le_bytes()); expected_audio.extend((-sample).to_le_bytes());
        }
        assert_eq!(audio.stdout, expected_audio, "trimmed stereo source clock at {start}");
        let video = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:v:0","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();
        assert!(video.status.success()); assert_eq!(video.stdout.len(), 16*16*4*5);
        for frame in 0..5usize {
            let response = app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview", json!({"project":trimmed,"compositionId":"new-timeline","timeUs":start+frame as u64*40000,"renderedOutputs":{}})), &token)).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let preview = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(&video.stdout[frame*1024..(frame+1)*1024], preview.as_ref(), "trim animated frame {start}/{frame}");
        }
    }
}
