use std::collections::BTreeMap;
use serde_json::json;
use video_kadr_backend::domain::concat_clip_animation::{ClipAnimation,Pose};
fn rest()->Pose{Pose{scale_x:1.,scale_y:1.,x:0.,y:0.,rotation_degrees:0.,opacity:1.}}
#[test]
fn source_pose_uses_clip_clock_stretch_canvas_and_unwrapped_rotation(){
 let keys:BTreeMap<_,_>=serde_json::from_value(json!({"scale":[{"at":0,"value":1},{"at":1,"value":3}],"offsetX":[{"at":0,"value":-1},{"at":1,"value":1}],"offsetY":[{"at":0,"value":0},{"at":1,"value":1}],"rotation":[{"at":0,"value":0},{"at":1,"value":1080}],"opacity":[{"at":0,"value":0},{"at":1,"value":1}]})).unwrap();
 let animation=ClipAnimation::new(&keys).unwrap();
 assert_eq!(animation.sample(500000,1000000,rest(),[2.,0.5],[1920.,1080.]).unwrap(),Pose{scale_x:4.,scale_y:1.,x:0.,y:540.,rotation_degrees:540.,opacity:0.5});
 assert_eq!(animation.sample(-1,1000000,rest(),[1.,1.],[1920.,1080.]).unwrap().rotation_degrees,0.);
 assert_eq!(animation.sample(2000000,1000000,rest(),[1.,1.],[1920.,1080.]).unwrap().rotation_degrees,1080.);
 assert!(animation.sample(0,0,rest(),[1.,1.],[1920.,1080.]).is_err());
}
#[test]
fn source_easing_and_property_tidy_keep_signed_overshoot(){
 let keys=serde_json::from_value(json!({"offsetX":[{"at":0,"value":-1},{"at":1,"value":1,"ease":[0.42,3,0.58,3]}],"opacity":[{"at":0,"value":-9},{"at":1,"value":9}]})).unwrap();
 let pose=ClipAnimation::new(&keys).unwrap().sample(500,1000,rest(),[1.,1.],[100.,100.]).unwrap();
 assert_eq!(pose.x,375.);assert_eq!(pose.opacity,0.5);
 let unknown=serde_json::from_value(json!({"volume":[{"at":0,"value":1}]})).unwrap();assert!(ClipAnimation::new(&unknown).is_err());
}

#[test]
fn persisted_source_pose_renders_opacity_and_roundtrips(){
 use video_kadr_backend::domain::{composition::{Composition,CanvasSpec},flat_surface::{render_media_layers,requires_native_video},group_surface::GroupSurface};
 let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
 let mut composition=Composition::new(canvas);
 composition.sources=serde_json::from_value(json!({"source":{"id":"source","kind":"image","durationTicks":1000000,"width":2,"height":2,"hasAudio":false}})).unwrap();
 composition.tracks=serde_json::from_value(json!([{"kind":"image","id":"image","name":"Image","clips":[{"id":"clip","sourceId":"source","timelineStartTick":1000000,"durationTicks":1000000,"transform":{"x":{"mode":"constant","value":0},"y":{"mode":"constant","value":0},"scaleX":{"mode":"constant","value":1},"scaleY":{"mode":"constant","value":1},"rotationDegrees":{"mode":"constant","value":0},"anchorX":0.5,"anchorY":0.5,"concatAnimation":{"durationTicks":1000000,"keys":{"opacity":[{"at":0,"value":0},{"at":1,"value":1}]},"rest":{"scaleX":1,"scaleY":1,"x":0,"y":0,"rotationDegrees":0,"opacity":0},"stretch":[1,1],"canvas":[2,2]}},"opacity":{"mode":"constant","value":0},"blendMode":"normal"}]}])).unwrap();
 composition.validate().unwrap();assert!(requires_native_video(&composition));
 let roundtrip:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(composition,roundtrip);
 for (time,expected) in [(1000000,0u8),(1500000,191u8),(1999999,255u8)]{
  let mut decode=|_:&video_kadr_backend::domain::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[255,255,255,255].repeat(4)});
  let frame=render_media_layers(&composition,time,&mut||Ok(()),Some(&mut decode),None).unwrap().unwrap();
  for pixel in frame.rgba.chunks_exact(4){assert_eq!(&pixel[..3],&[expected;3],"time {time}");}
 }
}
