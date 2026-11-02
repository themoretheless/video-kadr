use video_kadr_backend::domain::{concat_cutout_mask::{apply,MaskPlane},concat_picture::Preparation,group_surface::GroupSurface};
fn mapping()->Preparation{Preparation{crop:[0.;4],canvas:[2,1],flip_h:false,flip_v:false}}
#[test]
fn original_mapping_multiplies_existing_alpha_without_changing_rgb(){
 let before=GroupSurface{width:2,height:1,rgba:vec![10,20,30,255,40,50,60,128]};let mut frame=before.clone();
 apply(&mut frame,MaskPlane{width:2,height:1,probabilities:&[0,255]},&mapping(),&mut||Ok(())).unwrap();assert_eq!(frame.rgba,[10,20,30,0,40,50,60,128]);
 let mut frame=before;apply(&mut frame,MaskPlane{width:2,height:1,probabilities:&[0,255]},&Preparation{flip_h:true,..mapping()},&mut||Ok(())).unwrap();assert_eq!(frame.rgba,[10,20,30,255,40,50,60,0]);
}
#[test]
fn source_crop_mapping_and_fractional_mask_interpolation_match_original(){
 let mut frame=GroupSurface{width:2,height:1,rgba:[17,33,55,255].repeat(2)};
 apply(&mut frame,MaskPlane{width:4,height:1,probabilities:&[0,0,255,255]},&Preparation{crop:[0.5,0.,0.,0.],..mapping()},&mut||Ok(())).unwrap();assert_eq!(frame.rgba,[17,33,55,255].repeat(2));
 let mut frame=GroupSurface{width:1,height:1,rgba:vec![99,88,77,128]};apply(&mut frame,MaskPlane{width:2,height:1,probabilities:&[0,255]},&mapping(),&mut||Ok(())).unwrap();assert_eq!(frame.rgba,[99,88,77,64]);
}
#[test]
fn malformed_mask_and_early_cancellation_leave_frame_untouched(){
 let before=GroupSurface{width:2,height:1,rgba:[17,33,55,77].repeat(2)};let mut frame=before.clone();
 assert!(apply(&mut frame,MaskPlane{width:2,height:1,probabilities:&[255]},&mapping(),&mut||Ok(())).is_err());assert_eq!(frame,before);
 assert!(apply(&mut frame,MaskPlane{width:2,height:1,probabilities:&[255,255]},&mapping(),&mut||Err(anyhow::anyhow!("cancelled"))).is_err());assert_eq!(frame,before);
}

#[test]
fn raw_and_already_treated_decode_use_distinct_source_mappings(){
 use video_kadr_backend::domain::concat_cutout_mask::apply_for_decode;
 let geometry=Preparation{crop:[0.5,0.,0.,0.],flip_h:true,..mapping()};
 let mut raw=GroupSurface{width:4,height:1,rgba:[99,88,77,255].repeat(4)};
 apply_for_decode(&mut raw,MaskPlane{width:4,height:1,probabilities:&[0,0,255,255]},&geometry,false,&mut||Ok(())).unwrap();assert_eq!(raw.rgba.chunks_exact(4).map(|p|p[3]).collect::<Vec<_>>(),[0,0,255,255]);
 let mut treated=GroupSurface{width:2,height:1,rgba:[99,88,77,255].repeat(2)};
 apply_for_decode(&mut treated,MaskPlane{width:4,height:1,probabilities:&[0,0,255,255]},&geometry,true,&mut||Ok(())).unwrap();assert_eq!(treated.rgba,[99,88,77,255].repeat(2));
}
