use std::sync::Arc;
use video_kadr_backend::domain::{concat_cutout_clock::{MaskTimeline,ResolvedMask,Dispatch,apply_video},composition::{ClipPlacement,PlaybackMode},concat_picture::Preparation,group_surface::GroupSurface};
fn placement()->ClipPlacement{serde_json::from_value(serde_json::json!({"timelineStartTick":1000000,"sourceInTick":200000,"sourceOutTick":1200000,"speed":2})).unwrap()}
fn geometry()->Preparation{Preparation{crop:[0.;4],canvas:[2,1],flip_h:false,flip_v:false}}
#[test]
fn original_nearest_time_reach_ties_still_and_rounding(){
 let t=MaskTimeline::new(vec![400,0,200,200]).unwrap();assert_eq!(t.nearest_millis(0.1).unwrap(),Some(0));assert_eq!(t.nearest_millis(0.3).unwrap(),Some(200));
 let gap=MaskTimeline::new(vec![0,1000]).unwrap();for(seconds,expected)in[(0.25,Some(0)),(0.251,None),(0.75,Some(1000)),(0.749,None),(-1.,Some(0))]{assert_eq!(gap.nearest_millis(seconds).unwrap(),expected);}
 assert_eq!(MaskTimeline::new(vec![77]).unwrap().nearest_millis(9000.).unwrap(),Some(77));assert_eq!(MaskTimeline::new(vec![]).unwrap().nearest_millis(0.).unwrap(),None);
 assert!(t.nearest_millis(f64::NAN).is_err());let rounding=MaskTimeline::new(vec![0,2]).unwrap();assert_eq!(rounding.nearest_millis(0.0005).unwrap(),Some(0));assert_eq!(rounding.nearest_millis(0.0015).unwrap(),Some(2));
}
#[test]
fn retimed_clip_uses_source_time_and_applies_the_selected_mask(){
 let mut frame=GroupSurface{width:2,height:1,rgba:vec![10,20,30,255,40,50,60,128]};let mut requested=vec![];
 let status=apply_video(&mut frame,&placement(),PlaybackMode::Forward,1100000,&MaskTimeline::new(vec![200,400,600]).unwrap(),&geometry(),false,&mut|millis|{requested.push(millis);Ok(Some(Arc::new(ResolvedMask{width:2,height:1,probabilities:vec![0,255]})))},&mut||Ok(())).unwrap();
 assert_eq!(requested,[400]);assert_eq!(status,Dispatch::Applied{source_us:400000,mask_millis:400});assert_eq!(frame.rgba,[10,20,30,0,40,50,60,128]);
}
#[test]
fn gaps_missing_masks_and_inactive_clips_leave_picture_whole(){
 let before=GroupSurface{width:2,height:1,rgba:[17,33,55,77].repeat(2)};
 for(time,times,expected)in[(900000,vec![200,400],Dispatch::Inactive),(1100000,vec![0,1000],Dispatch::Unanalysed{source_us:400000}),(1100000,vec![200,400],Dispatch::Missing{source_us:400000,mask_millis:400})]{
  let mut frame=before.clone();let mut calls=0;
  let actual=apply_video(&mut frame,&placement(),PlaybackMode::Forward,time,&MaskTimeline::new(times).unwrap(),&geometry(),false,&mut|_|{calls+=1;Ok(None)},&mut||Ok(())).unwrap();assert_eq!(actual,expected);assert_eq!(frame,before);assert_eq!(calls,if matches!(expected,Dispatch::Missing{..}){1}else{0});
 }
}
