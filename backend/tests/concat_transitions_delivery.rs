mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition, TransformSpec},
    library::MediaEntry,
};
fn command(args: &[&str]) -> Vec<u8> {
    let result = std::process::Command::new("ffmpeg")
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}
async fn terminal(app: &axum::Router, id: &str) -> Value {
    for _ in 0..3000 {
        let (status, body, _) = support::send(app, support::get(&format!("/api/jobs/{id}"))).await;
        assert_eq!(status, StatusCode::OK);
        if matches!(
            body["status"].as_str(),
            Some("done" | "error" | "cancelled")
        ) {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Concat transition export did not finish")
}
fn decode(path: &Path) -> Vec<u8> {
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-map",
        "0:v:0",
        "-an",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ])
}
#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn concat_dips_and_dissolve_use_native_independent_window_export() {
    let (state, _directory) = support::make_state(true, false).await;
    for (id, color) in [("outgoing", "red"), ("incoming", "blue")] {
        let file = format!("{id}.mov");
        let path = state.sources_dir().join(&file);
        command(&[
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={color}:s=64x32:r=4:d=3"),
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-threads",
            "1",
            path.to_str().unwrap(),
        ]);
        state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    }
    let app = support::router(state.clone());
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 32,
        fps_milli: 4000,
        ..Default::default()
    });
    composition.sources=serde_json::from_value(json!({"outgoing":{"id":"outgoing","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"incoming":{"id":"incoming","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
    let mut shape_midpoints=Vec::new();
    for (kind,original_id,choice) in [("concat_dissolve","concat.dissolve",None),("concat_fade_black","concat.fade-black",None),("concat_fade_white","concat.fade-white",None),("procedural","concat.linear-wipe",None),("procedural","concat.clock-wipe",None),("procedural","concat.iris",None),("procedural","concat.shape-wipe",Some(0)),("procedural","concat.shape-wipe",Some(1)),("procedural","concat.shape-wipe",Some(2)),("procedural","concat.push",Some(0)),("procedural","concat.push",Some(1)),("procedural","concat.push",Some(2)),("procedural","concat.push",Some(3)),("procedural","concat.slide",Some(0)),("procedural","concat.slide",Some(1)),("procedural","concat.slide",Some(2)),("procedural","concat.slide",Some(3)),("procedural","concat.spin",None),("procedural","concat.swirl",None)] {
        let params=if original_id=="concat.linear-wipe"{json!({"direction":0,"softness":20})}else if original_id=="concat.shape-wipe"{json!({"shape":choice.unwrap(),"softness":15})}else if matches!(original_id,"concat.push"|"concat.slide"){json!({"direction":choice.unwrap()})}else{json!({})};
        let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":original_id,"params":params}]}).to_string())).unwrap();
        let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
        let native=&converted["nativeTransitions"][0];assert_eq!(native["kind"],kind);
        composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Transition","clips":[{"id":"a","sourceId":"outgoing","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"incoming","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native.get("kernel").cloned().unwrap_or(Value::Null),"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();
        composition.validate().unwrap();
        for (start, phase) in [(750000, 0), (1000000, 1), (1250000, 2)] {
            let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":start,"endTicks":start+250000}}}).to_string())).unwrap();
            let (status, accepted, _) = support::send(&app, request).await;
            assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
            let done = terminal(&app, accepted["jobId"].as_str().unwrap()).await;
            assert_eq!(done["status"], "done", "{done}");
            let pixels = decode(
                &state
                    .outputs_dir()
                    .join(done["result"]["filename"].as_str().unwrap()),
            );
            assert_eq!(pixels.len(), 64 * 32 * 3);
            let p = &pixels[(16 * 64 + 32) * 3..(16 * 64 + 32) * 3 + 3];
            match (kind, phase) {
                ("procedural",1) if matches!(original_id,"concat.spin"|"concat.swirl")=>{
                    assert!(p[0]>=170&&p[2]>=170&&p[1]<=10,"rotation mixed centre {p:?}");
                    let corner=&pixels[..3];
                    if original_id=="concat.spin"{assert!(corner.iter().all(|v|*v<=5),"spin clear corner {corner:?}");}
                    else{assert!(corner[0]>=170&&corner[2]>=170&&corner[1]<=10,"swirl opaque corner {corner:?}");}
                },
                ("procedural",1) if matches!(original_id,"concat.push"|"concat.slide")=>{
                    let direction=choice.unwrap();let vertical=direction>=2;
                    let at=|x:usize,y:usize|&pixels[(y*64+x)*3..(y*64+x)*3+3];
                    let(first,last)=if vertical{(at(32,8),at(32,24))}else{(at(16,16),at(48,16))};
                    let incoming_first=if original_id=="concat.push"{direction==1||direction==3}else{direction==0||direction==2};
                    let(red,blue)=if incoming_first{(last,first)}else{(first,last)};
                    assert!(red[0]>=240&&red[2]<=10,"{original_id} direction{direction} outgoing {red:?}");
                    assert!(blue[2]>=240&&blue[0]<=10,"{original_id} direction{direction} incoming {blue:?}");
                },
                ("procedural",1) if original_id=="concat.shape-wipe"=>{
                    assert!(pixels[0]>=240&&pixels[2]<=10,"shape outgoing corner {:?}",&pixels[..3]);
                    assert!(p[2]>=240&&p[0]<=10,"shape incoming centre {p:?}");
                    shape_midpoints.push(pixels.clone());
                },
                ("procedural",1) if original_id=="concat.iris"=>{
                    let corner=&pixels[..3];
                    assert!(corner[0]>=240&&corner[2]<=10,"iris outgoing corner {corner:?}");
                    assert!(p[2]>=240&&p[0]<=10,"iris incoming centre {p:?}");
                },
                ("procedural",1) if original_id=="concat.clock-wipe"=>{
                    let left=&pixels[(16*64+16)*3..(16*64+16)*3+3];
                    let right=&pixels[(16*64+48)*3..(16*64+48)*3+3];
                    assert!(left[0]>=240&&left[2]<=10,"clock outgoing left {left:?}");
                    assert!(right[2]>=240&&right[0]<=10,"clock incoming right {right:?}");
                },
                ("procedural",1)=>{
                    let left=&pixels[(16*64)*3..(16*64)*3+3];
                    let right=&pixels[(16*64+63)*3..(16*64+63)*3+3];
                    assert!(left[2]>=240&&left[0]<=10,"wipe incoming left {left:?}");
                    assert!(right[0]>=240&&right[2]<=10,"wipe outgoing right {right:?}");
                    assert!(p[0]>100&&p[2]>100,"wipe soft centre {p:?}");
                },
                ("concat_fade_black", 1) => {
                    assert!(p.iter().all(|v| *v <= 5), "black midpoint {p:?}")
                }
                ("concat_fade_white", 1) => {
                    assert!(p.iter().all(|v| *v >= 245), "white midpoint {p:?}")
                }
                ("concat_dissolve", 1) => assert!(
                    p[0] >= 170 && p[2] >= 170 && p[1] <= 10,
                    "linear-light cross dissolve {p:?}"
                ),
                ("concat_fade_black", 0) => assert!(
                    (110..=140).contains(&p[0]) && p[2] <= 10,
                    "display quarter red {p:?}"
                ),
                ("concat_fade_black", 2) => assert!(
                    (110..=140).contains(&p[2]) && p[0] <= 10,
                    "display three-quarter blue {p:?}"
                ),
                _ => {}
            }
        }
    }
    assert_eq!(shape_midpoints.len(),3);
    assert_ne!(shape_midpoints[0],shape_midpoints[1]);assert_ne!(shape_midpoints[0],shape_midpoints[2]);assert_ne!(shape_midpoints[1],shape_midpoints[2]);

}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn rgb_split_exports_actual_opposite_colour_edges_against_zero_spread(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for spread in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.rgb-split-wipe","params":{"spread":spread}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"RGB Split","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(32).iter().all(|v|*v>=170),"grey source centre {:?}",at(32));
  if spread==100{
   let left=at(23);assert!(left[0]>=140&&left[1]<=70&&left[2]<=70,"red leading edge {left:?}");
   let yellow=at(25);assert!(yellow[0]>=140&&yellow[1]>=140&&yellow[2]<=70,"delayed blue edge {yellow:?}");
   let right=at(38);assert!(right[0]<=70&&right[1]>=140&&right[2]>=140,"cyan trailing edge {right:?}");
  }else{assert!(at(23).iter().all(|v|*v<=15),"zero-spread black control {:?}",at(23));}
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn blur_dissolve_exports_actual_spatial_blur_against_zero_strength(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for strength in [10,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.blur-dissolve","params":{"strength":strength}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Blur Dissolve","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(32).iter().all(|v|*v>=170),"grey source centre {:?}",at(32));
  if strength==10{
   assert!(at(21).iter().all(|v|*v>=70),"blur halo outside stripe {:?}",at(21));
   assert!(at(24).iter().all(|v|*v>=130&&*v<=175),"blurred edge {:?}",at(24));
  }else{
   assert!(at(21).iter().all(|v|*v<=15),"zero-strength black control {:?}",at(21));
   assert!(at(24).iter().all(|v|*v>=175),"zero-strength sharp edge {:?}",at(24));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn blur_dissolve_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("blur-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("blur-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Blur preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for strength in [10,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.blur-dissolve","params":{"strength":strength}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Blur Dissolve","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Blur controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(32).iter().all(|v|*v>=170),"grey source centre {:?}",at(32));
  if strength==10{
   assert!(at(21).iter().all(|v|*v>=70),"blur halo outside stripe {:?}",at(21));
   assert!(at(24).iter().all(|v|*v>=130&&*v<=175),"blurred edge {:?}",at(24));
  }else{
   assert!(at(21).iter().all(|v|*v<=15),"zero-strength black control {:?}",at(21));
   assert!(at(24).iter().all(|v|*v>=175),"zero-strength sharp edge {:?}",at(24));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn whip_pan_exports_horizontal_blur_against_low_speed(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for speed in [100,20]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.whip-pan","params":{"speed":speed}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Whip Pan","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(48).iter().all(|v|*v<=15),"incoming black side {:?}",at(48));
  if speed==100{
   assert!(at(12).iter().all(|v|*v>=70),"high-speed horizontal halo {:?}",at(12));
  }else{
   assert!(at(12).iter().all(|v|*v<=15),"low-speed dark control {:?}",at(12));
   assert!(at(2).iter().all(|v|*v>=230),"shifted outgoing stripe {:?}",at(2));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn whip_pan_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("whip-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("whip-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Whip preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for speed in [100,20]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.whip-pan","params":{"speed":speed}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Whip Pan","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Whip controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(48).iter().all(|v|*v<=15),"incoming black side {:?}",at(48));
  if speed==100{
   assert!(at(12).iter().all(|v|*v>=70),"high-speed horizontal halo {:?}",at(12));
  }else{
   assert!(at(12).iter().all(|v|*v<=15),"low-speed dark control {:?}",at(12));
   assert!(at(2).iter().all(|v|*v>=230),"shifted outgoing stripe {:?}",at(2));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn zoom_exports_actual_in_and_out_scale_geometry(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for direction in [0,1]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.zoom-transition","params":{"direction":direction}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Zoom","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed central stripe {:?}",at(32));
  if direction==0{
   assert!(at(20).iter().all(|v|*v>=170),"In expands outgoing stripe {:?}",at(20));
  }else{
   assert!(at(20).iter().all(|v|*v<=15),"Out contracts outgoing stripe {:?}",at(20));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn zoom_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("zoom-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("zoom-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Zoom preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for direction in [0,1]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.zoom-transition","params":{"direction":direction}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Zoom","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Zoom controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed central stripe {:?}",at(32));
  if direction==0{
   assert!(at(20).iter().all(|v|*v>=170),"In expands outgoing stripe {:?}",at(20));
  }else{
   assert!(at(20).iter().all(|v|*v<=15),"Out contracts outgoing stripe {:?}",at(20));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn light_leak_flash_exports_warm_burn_against_zero_strength(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for strength in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.light-leak-flash","params":{"strength":strength}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Light Leak Flash","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  if strength==100{
   assert!(at(12).iter().all(|v|*v>=150),"flash lifts black background {:?}",at(12));
   assert!(at(12)[0]>at(12)[2]+15,"warm flash chroma {:?}",at(12));
   assert!(at(32).iter().all(|v|*v>=240),"burned stripe caps near white {:?}",at(32));
  }else{
   assert!(at(12).iter().all(|v|*v<=15),"zero-strength black control {:?}",at(12));
   assert!(at(32).iter().all(|v|*v>=170&&*v<=200),"zero-strength mixed stripe {:?}",at(32));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn light_leak_flash_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("flash-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("flash-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Flash preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for strength in [100,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.light-leak-flash","params":{"strength":strength}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Light Leak Flash","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Flash controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  if strength==100{
   assert!(at(12).iter().all(|v|*v>=150),"flash lifts black background {:?}",at(12));
   assert!(at(12)[0]>at(12)[2]+15,"warm flash chroma {:?}",at(12));
   assert!(at(32).iter().all(|v|*v>=240),"burned stripe caps near white {:?}",at(32));
  }else{
   assert!(at(12).iter().all(|v|*v<=15),"zero-strength black control {:?}",at(12));
   assert!(at(32).iter().all(|v|*v>=170&&*v<=200),"zero-strength mixed stripe {:?}",at(32));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn light_leak_transition_exports_hot_core_against_zero_warmth(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for warmth in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.light-leak-transition","params":{"warmth":warmth}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Light Leak","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  if warmth==100{
   assert!(at(4).iter().all(|v|*v>=100),"warm halo lifts black background {:?}",at(4));
   assert!(at(4)[0]>at(4)[2]+15,"warm halo chroma {:?}",at(4));
   assert!(at(24).iter().all(|v|*v>=240),"hot core caps near white {:?}",at(24));
  }else{
   assert!(at(4).iter().all(|v|*v<=15),"zero-warmth black control {:?}",at(4));
   assert!(at(24).iter().all(|v|*v>=170&&*v<=200),"zero-warmth mixed stripe {:?}",at(24));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn light_leak_transition_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("leak-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("leak-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Leak preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for warmth in [100,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.light-leak-transition","params":{"warmth":warmth}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Light Leak","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Leak controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  if warmth==100{
   assert!(at(4).iter().all(|v|*v>=100),"warm halo lifts black background {:?}",at(4));
   assert!(at(4)[0]>at(4)[2]+15,"warm halo chroma {:?}",at(4));
   assert!(at(24).iter().all(|v|*v>=240),"hot core caps near white {:?}",at(24));
  }else{
   assert!(at(4).iter().all(|v|*v<=15),"zero-warmth black control {:?}",at(4));
   assert!(at(24).iter().all(|v|*v>=170&&*v<=200),"zero-warmth mixed stripe {:?}",at(24));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn zoom_blur_transition_exports_radial_blur_against_zero_strength(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for strength in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.zoom-blur-transition","params":{"strength":strength}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Zoom Blur","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed centre {:?}",at(32));
  if strength==100{
   assert!(at(21).iter().all(|v|*v>=70&&*v<=170),"radial partial stripe edge {:?}",at(21));
  }else{
   assert!(at(21).iter().all(|v|*v>=175),"zero-strength zoomed sharp edge {:?}",at(21));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn zoom_blur_transition_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("radial-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("radial-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Radial preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for strength in [100,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.zoom-blur-transition","params":{"strength":strength}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Zoom Blur","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Radial controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed centre {:?}",at(32));
  if strength==100{
   assert!(at(21).iter().all(|v|*v>=70&&*v<=170),"radial partial stripe edge {:?}",at(21));
  }else{
   assert!(at(21).iter().all(|v|*v>=175),"zero-strength zoomed sharp edge {:?}",at(21));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn film_burn_exports_white_peak_against_zero_heat(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for heat in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.film-burn","params":{"heat":heat}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Film Burn","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  if heat==100{
   assert!(pixels.iter().enumerate().all(|(i,v)|i%3==3||*v>=240),"full midpoint burn must be white");
  }else{
   assert!(at(12).iter().all(|v|*v<=15),"zero-heat black control {:?}",at(12));
   assert!(at(32).iter().all(|v|*v>=170&&*v<=200),"zero-heat mixed stripe {:?}",at(32));
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn film_burn_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("burn-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("burn-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Burn preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for heat in [100,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.film-burn","params":{"heat":heat}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Film Burn","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Burn controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  if heat==100{
   assert!(pixels.iter().enumerate().all(|(i,v)|i%4==3||*v>=240),"full midpoint burn must be white");
  }else{
   assert!(at(12).iter().all(|v|*v<=15),"zero-heat black control {:?}",at(12));
   assert!(at(32).iter().all(|v|*v>=170&&*v<=200),"zero-heat mixed stripe {:?}",at(32));
  }
 }
}

#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn cube_exports_actual_shaded_faces_and_midpoint_gap() {
    let (state, _directory) = support::make_state(true, false).await;
    for (id, color) in [("outgoing", "red"), ("incoming", "blue")] {
        let file = format!("{id}.mov");
        let path = state.sources_dir().join(&file);
        command(&[
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={color}:s=64x32:r=4:d=3"),
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-threads",
            "1",
            path.to_str().unwrap(),
        ]);
        state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
    }
    let app = support::router(state.clone());
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 32,
        fps_milli: 4000,
        ..Default::default()
    });
    composition.sources=serde_json::from_value(json!({"outgoing":{"id":"outgoing","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"incoming":{"id":"incoming","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
    for (kind,original_id,choice) in [("procedural","concat.cube",None::<i32>)] {
        let params=if original_id=="concat.linear-wipe"{json!({"direction":0,"softness":20})}else if original_id=="concat.shape-wipe"{json!({"shape":choice.unwrap(),"softness":15})}else if matches!(original_id,"concat.push"|"concat.slide"){json!({"direction":choice.unwrap()})}else{json!({})};
        let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":original_id,"params":params}]}).to_string())).unwrap();
        let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
        let native=&converted["nativeTransitions"][0];assert_eq!(native["kind"],kind);
        composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Transition","clips":[{"id":"a","sourceId":"outgoing","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"incoming","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native.get("kernel").cloned().unwrap_or(Value::Null),"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();
        composition.validate().unwrap();
        for (start, phase) in [(750000, 0), (1000000, 1), (1250000, 2)] {
            let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":start,"endTicks":start+250000}}}).to_string())).unwrap();
            let (status, accepted, _) = support::send(&app, request).await;
            assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
            let done = terminal(&app, accepted["jobId"].as_str().unwrap()).await;
            assert_eq!(done["status"], "done", "{done}");
            let pixels = decode(
                &state
                    .outputs_dir()
                    .join(done["result"]["filename"].as_str().unwrap()),
            );
            assert_eq!(pixels.len(), 64 * 32 * 3);
            let p = &pixels[(16 * 64 + 32) * 3..(16 * 64 + 32) * 3 + 3];
            if phase==1{assert!(pixels.iter().all(|v|*v<=15),"midpoint opaque dark gap");}
            else{
                let channel=if phase==0{0}else{2};
                assert!((180..=220).contains(&p[channel]),"shaded face {p:?}");
                assert!(pixels[..3].iter().all(|v|*v<=15),"gap corner {:?}",&pixels[..3]);
            }

        }
    }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn cube_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("cube-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("cube-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Cube preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for time in [750000,1000000,1250000]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.cube","params":{}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Cube","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Cube controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":time,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(0).iter().all(|v|*v<=15),"opaque gap corner {:?}",at(0));
  if time==750000{assert!(at(32).iter().all(|v|*v>=180&&*v<=220),"shaded outgoing stripe {:?}",at(32));}
  else if time==1000000{assert!(pixels.chunks_exact(4).all(|p|p[..3].iter().all(|v|*v<=15)&&p[3]==255),"opaque midpoint gap");}
  else{assert!(at(32).iter().all(|v|*v<=5),"incoming black face {:?}",at(32));}
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn glitch_transition_exports_split_channels_against_zero_amount(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for amount in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.glitch-transition","params":{"amount":amount}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Glitch","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed centre {:?}",at(32));
  if amount==100{
   assert!(pixels.chunks_exact(3).any(|p|p[0].abs_diff(p[2])>=20),"split red and blue channels");
   let edge:std::collections::BTreeSet<_>=(0..32).map(|y|pixels[(y*64+23)*3]).collect();assert!(edge.len()>1,"slice-dependent tearing");
  }else{
   assert!(at(23).iter().all(|v|*v<=15),"zero-amount black edge {:?}",at(23));
   assert!(pixels.chunks_exact(3).all(|p|p[0].abs_diff(p[1])<=10&&p[1].abs_diff(p[2])<=10),"zero-amount neutral channels");
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn glitch_transition_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("glitch-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("glitch-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Glitch preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for amount in [100,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.glitch-transition","params":{"amount":amount}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Glitch","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Glitch controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed centre {:?}",at(32));
  if amount==100{
   assert!(pixels.chunks_exact(4).any(|p|p[0].abs_diff(p[2])>=20),"split red and blue channels");
   let edge:std::collections::BTreeSet<_>=(0..32).map(|y|pixels[(y*64+23)*4]).collect();assert!(edge.len()>1,"slice-dependent tearing");
  }else{
   assert!(at(23).iter().all(|v|*v<=15),"zero-amount black edge {:?}",at(23));
   assert!(pixels.chunks_exact(4).all(|p|p[0].abs_diff(p[1])<=10&&p[1].abs_diff(p[2])<=10),"zero-amount neutral channels");
  }
 }
}

#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn glitch_warp_exports_split_channels_against_zero_amount(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for amount in [100,0]{
  let request=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.glitch-warp","params":{"amount":amount}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Glitch Warp","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1000000,"endTicks":1250000}}}).to_string())).unwrap();
  let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");let done=terminal(&app,accepted["jobId"].as_str().unwrap()).await;assert_eq!(done["status"],"done","{done}");
  let pixels=decode(&state.outputs_dir().join(done["result"]["filename"].as_str().unwrap()));assert_eq!(pixels.len(),64*32*3);
  let at=|x:usize|&pixels[(16*64+x)*3..(16*64+x)*3+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed centre {:?}",at(32));
  if amount==100{
   assert!(pixels.chunks_exact(3).any(|p|p[0].abs_diff(p[2])>=20),"split red and blue channels");
   let edge:std::collections::BTreeSet<_>=(0..32).map(|y|pixels[(y*64+23)*3]).collect();assert!(edge.len()>1,"slice-dependent tearing");
  }else{
   assert!(at(23).iter().all(|v|*v<=15),"zero-amount black edge {:?}",at(23));
   assert!(pixels.chunks_exact(3).all(|p|p[0].abs_diff(p[1])<=10&&p[1].abs_diff(p[2])<=10),"zero-amount neutral channels");
  }
 }
}


#[tokio::test]
#[ignore="requires actual FFmpeg/FFprobe/qtrle/libx264 and ordinary export worker"]
async fn glitch_warp_private_preview_preserves_saved_controls_and_source_acl(){
 let(state,_directory)=support::make_state(true,false).await;
 for(id,filter) in [("rgb-stripe","color=c=black:s=64x32:r=4:d=3,drawbox=x=24:y=0:w=16:h=32:color=white:t=fill"),("rgb-black","color=c=black:s=64x32:r=4:d=3")]{
  let file=format!("{id}.mov");let path=state.sources_dir().join(&file);
  command(&["-nostdin","-v","error","-f","lavfi","-i",filter,"-c:v","qtrle","-pix_fmt","argb","-threads","1",path.to_str().unwrap()]);
  state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":file,"url":format!("/files/sources/{file}"),"mediaType":"video","width":64,"height":32,"duration":3,"fps":4,"vcodec":"qtrle","hasAudio":false}))).await;
 }
 let owner=state.db.register_auth_user("warp-preview-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("warp-preview-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Warp preview").await.unwrap();
 state.db.create_space_composition_project("Private sources",&json!({"schemaVersion":1,"sources":{"rgb-stripe":{"id":"rgb-stripe"},"rgb-black":{"id":"rgb-black"}}}),&["rgb-black".into(),"rgb-stripe".into()],&owner.user.username,&space.id).await.unwrap();
 let request=|method:&str,path:&str,body:Value,token:&str|Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 let app=support::router(state.clone());
 let mut composition=Composition::new(CanvasSpec{width:64,height:32,fps_milli:4000,..Default::default()});
 composition.sources=serde_json::from_value(json!({"rgb-stripe":{"id":"rgb-stripe","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false},"rgb-black":{"id":"rgb-black","kind":"video","durationTicks":3000000,"width":64,"height":32,"hasAudio":false}})).unwrap();
 for amount in [100,0]{
  let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[{"id":"concat.glitch-warp","params":{"amount":amount}}]}).to_string())).unwrap();
  let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");let native=&converted["nativeTransitions"][0];
  composition.tracks=serde_json::from_value(json!([{"id":"video","kind":"video","name":"Glitch Warp","clips":[{"id":"a","sourceId":"rgb-stripe","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"},{"id":"b","sourceId":"rgb-black","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"sourceAudioEnabled":false,"blendMode":"normal"}],"transitions":[{"id":"cut","kind":native["kind"],"kernel":native["kernel"],"fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"}}]}])).unwrap();composition.validate().unwrap();
  let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":2000000,"content":composition,"instances":[]}}});
  let(status,saved,_)=support::send(&app,request("POST","/api/composition-projects",json!({"schemaVersion":2,"mode":"composition","name":"Warp controls","spaceId":space.id,"document":project}),&owner.token)).await;assert_eq!(status,StatusCode::CREATED,"{saved}");
  let path=format!("/api/composition-projects/{}",saved["id"].as_str().unwrap());
  let(status,loaded,_)=support::send(&app,request("GET",&path,Value::Null,&owner.token)).await;assert_eq!(status,StatusCode::OK);assert_eq!(loaded["document"],project);
  let body=json!({"project":loaded["document"],"timeUs":1000000,"renderedOutputs":{}});
  use tower::ServiceExt;
  let denied=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body.clone(),&outsider.token)).await.unwrap();assert_eq!(denied.status(),StatusCode::FORBIDDEN);
  let response=app.clone().oneshot(request("POST","/api/compositions/nested-preview",body,&owner.token)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["x-frame-width"],"64");assert_eq!(response.headers()["x-frame-height"],"32");
  let pixels=axum::body::to_bytes(response.into_body(),64*32*4).await.unwrap();assert_eq!(pixels.len(),64*32*4);
  let at=|x:usize|&pixels[(16*64+x)*4..(16*64+x)*4+3];
  assert!(at(32).iter().all(|v|*v>=170),"mixed centre {:?}",at(32));
  if amount==100{
   assert!(pixels.chunks_exact(4).any(|p|p[0].abs_diff(p[2])>=20),"split red and blue channels");
   let edge:std::collections::BTreeSet<_>=(0..32).map(|y|pixels[(y*64+23)*4]).collect();assert!(edge.len()>1,"slice-dependent tearing");
  }else{
   assert!(at(23).iter().all(|v|*v<=15),"zero-amount black edge {:?}",at(23));
   assert!(pixels.chunks_exact(4).all(|p|p[0].abs_diff(p[1])<=10&&p[1].abs_diff(p[2])<=10),"zero-amount neutral channels");
  }
 }
}

