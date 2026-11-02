use serde_json::json;
use video_kadr_backend::domain::{concat_cutout_import::{plan,Subject},cutout::Tool};
#[test]
fn default_auto_retains_original_defaults_and_native_correction(){
 let p=plan(&json!({"mode":"auto"})).unwrap();assert_eq!(p.subject,Subject::Auto);assert!(!p.correction.custom);assert_eq!(p.correction.feather,0.01);assert!(p.correction.strokes.is_empty());assert!(p.region_source_seconds.is_empty());p.correction.validate().unwrap();
}
#[test]
fn source_tidying_preserves_order_source_times_and_requires_smart_regions(){
 let p=plan(&json!({"mode":"custom","subject":"object","feather":0.5,"strokes":[{"tool":"brush","size":0,"points":[[-4,3]],"at":-1},{"tool":"smartBrush","size":0.2,"points":[[0.5,0.5]],"at":2},{"tool":"smartEraser","size":0.8,"points":[[0.6,0.7]],"at":2},{"tool":"eraser","size":0.2,"points":[]}]})).unwrap();
 assert_eq!(p.subject,Subject::Object);assert_eq!(p.correction.feather,0.1);assert_eq!(p.correction.strokes.len(),3);assert_eq!(p.correction.strokes[0].tool,Tool::Brush);assert_eq!(p.correction.strokes[0].size,0.005);assert_eq!(p.correction.strokes[0].points,[[-0.5,1.5]]);assert_eq!(p.correction.strokes[0].at,Some(0.));assert_eq!(p.correction.strokes[2].size,0.5);assert_eq!(p.region_source_seconds,[2.]);assert!(p.correction.strokes.iter().all(|s|s.region.is_none()));p.correction.validate().unwrap();
}
#[test]
fn automatic_preserves_saved_strokes_without_requesting_inactive_regions_and_refuses_unknown_data(){
 let p=plan(&json!({"mode":"auto","subject":"person","strokes":[{"tool":"smartBrush","size":0.1,"points":[[0,0]],"at":3}]})).unwrap();assert!(!p.correction.custom);assert_eq!(p.correction.strokes.len(),1);assert!(p.region_source_seconds.is_empty());
 for bad in [json!({"mode":"future"}),json!({"mode":"auto","subject":"future"}),json!({"mode":"custom","unknown":true}),json!({"mode":"custom","strokes":[{"tool":"brush","size":0.1,"points":[[0,0]],"region":{}}]})]{assert!(plan(&bad).is_err());}
}
