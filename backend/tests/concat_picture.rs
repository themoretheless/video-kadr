use video_kadr_backend::domain::{concat_picture::Preparation,group_surface::GroupSurface};
#[test]
fn original_reference_crop_keeps_blue_half_and_fits_before_effects(){
 let source=GroupSurface{width:4,height:2,rgba:[[255,0,0,255],[255,0,0,255],[0,0,255,255],[0,0,255,255]].repeat(2).into_iter().flatten().collect()};
 let actual=Preparation{crop:[0.5,0.,0.,0.],canvas:[2,2],flip_h:false,flip_v:false}.prepare(&source,&mut||Ok(())).unwrap();
 assert_eq!(actual.surface,GroupSurface{width:2,height:2,rgba:[0,0,255,255].repeat(4)});assert_eq!(actual.center_offset,[0.,0.]);
}
#[test]
fn fractional_crop_preserves_original_straight_rgba_bilinear_rounding(){
 let source=GroupSurface{width:2,height:1,rgba:vec![0,0,0,0,100,200,40,200]};
 let p=Preparation{crop:[0.25,0.,0.,0.],canvas:[3,2],flip_h:false,flip_v:false};
 let frame=p.prepare(&source,&mut||Ok(())).unwrap().surface;
 assert_eq!(frame.rgba,[25,50,10,50,75,150,30,150,100,200,40,200].repeat(2));
 let mirrored=Preparation{flip_h:true,..p}.prepare(&source,&mut||Ok(())).unwrap().surface;
 assert_eq!(mirrored.rgba,[100,200,40,200,75,150,30,150,25,50,10,50].repeat(2));
}
#[test]
fn invalid_input_and_cancellation_do_not_modify_source(){
 let source=GroupSurface{width:3,height:2,rgba:[17,33,55,77].repeat(6)};let before=source.clone();
 let p=Preparation{crop:[0.;4],canvas:[4,2],flip_h:false,flip_v:false};
 let fitted=p.prepare(&source,&mut||Ok(())).unwrap();assert_eq!(fitted.surface.width,3);assert_eq!(fitted.center_offset,[-0.5,0.]);
 assert!(p.prepare(&source,&mut||Err(anyhow::anyhow!("cancelled"))).is_err());assert_eq!(source,before);
 assert!(Preparation{crop:[0.6,0.,0.4,0.],..p}.prepare(&source,&mut||Ok(())).is_err());
}

#[test]
fn image_and_video_prepare_crop_before_effects_and_temporal_prefixes(){
 use video_kadr_backend::domain::{composition::{CanvasSpec,Composition,TransformSpec},flat_surface::{render_media_layers,requires_native_video}};
 use serde_json::json;
 for kind in ["image","video"]{
  for temporal in [false,true]{
   let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;let mut c=Composition::new(canvas);
   c.sources=serde_json::from_value(json!({"source":{"id":"source","kind":kind,"durationTicks":1000000,"width":4,"height":2,"hasAudio":false}})).unwrap();
   let mut transform=TransformSpec::default();transform.concat_preparation=Some(Preparation{crop:[0.5,0.,0.,0.],canvas:[2,2],flip_h:false,flip_v:false});
   let mut clip=json!({"id":"clip","sourceId":"source","transform":transform,"opacity":{"mode":"constant","value":1},"blendMode":"normal","effects":[{"kind":"color_invert","parameters":{}}]});
   if kind=="video"{clip["placement"]=json!({"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1});}else{clip["timelineStartTick"]=json!(0);clip["durationTicks"]=json!(1000000);}
   if temporal{clip["effects"].as_array_mut().unwrap().push(json!({"kind":"posterize_time","parameters":{"rate":12}}));}
   c.tracks=serde_json::from_value(json!([{"kind":kind,"id":"track","name":"Track","clips":[clip]}])).unwrap();c.validate().unwrap();assert!(requires_native_video(&c));
   let picture=||GroupSurface{width:4,height:2,rgba:[[255,0,0,255],[255,0,0,255],[0,0,255,255],[0,0,255,255]].repeat(2).into_iter().flatten().collect()};
   let mut image_calls=0;let mut video_times=vec![];
   let mut image=|_:&video_kadr_backend::domain::composition::SourceId|{image_calls+=1;Ok(picture())};
   let mut video=|_:&video_kadr_backend::domain::composition::SourceId,time:u64,_:bool|{video_times.push(time);Ok(picture())};
   let frame=render_media_layers(&c,125000,&mut||Ok(()),Some(&mut image),Some(&mut video)).unwrap().unwrap();
   assert_eq!(frame.rgba,[255,255,0,255].repeat(4),"{kind} temporal={temporal}");
   if temporal{if kind=="video"{assert!(video_times.len()>1,"{video_times:?}");}else{assert!(image_calls>1);}}
   let _:Composition=serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap();
  }
 }
}

#[test]
fn original_reference_direct_compositor_uses_gamma24_and_one_source_sample(){
 use video_kadr_backend::domain::{concat_clip_animation::Pose,composition::BlendMode};
 let p=Preparation{crop:[0.;4],canvas:[1,1],flip_h:false,flip_v:false};
 let pose=Pose{scale_x:1.,scale_y:1.,x:0.,y:0.,rotation_degrees:0.,opacity:0.5};
 let white=GroupSurface{width:1,height:1,rgba:vec![255;4]};let mut black=GroupSurface{width:1,height:1,rgba:vec![0,0,0,255]};
 p.composite_reference(&white,&mut black,pose,BlendMode::Normal,&mut||Ok(())).unwrap();assert_eq!(black.rgba,[191,191,191,255]);
 let strip=GroupSurface{width:4,height:2,rgba:[[255,0,0,255],[255,0,0,255],[0,0,255,255],[0,0,255,255]].repeat(2).into_iter().flatten().collect()};
 let mut output=GroupSurface{width:2,height:2,rgba:[0,0,0,255].repeat(4)};
 Preparation{crop:[0.5,0.,0.,0.],canvas:[2,2],..p}.composite_reference(&strip,&mut output,Pose{opacity:1.,..pose},BlendMode::Normal,&mut||Ok(())).unwrap();
 assert_eq!(output.rgba,[0,0,255,255].repeat(4));
}
