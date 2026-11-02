use serde_json::json;
use video_kadr_backend::db::{composition_document_byte_limit,MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES};
#[test]
fn only_validated_spherical_motion_and_capture_tracks_extend_project_budget(){
 let orientation=json!([{"timeSeconds":0.,"quaternion":[1.,0.,0.,0.]},{"timeSeconds":1.,"quaternion":[1.,0.,0.,0.]}]);
 let clock=json!([{"timeSeconds":0.,"captureSeconds":2.},{"timeSeconds":1.,"captureSeconds":3.}]);
 let mut document=json!({"tracks":[{"kind":"video","clips":[{"effects":[{"kind":"spherical_video","parameters":{"input":"dual_fisheye","orientation":orientation,"rollingShutter":{"readoutSeconds":0.01,"captureClock":clock}}}]}]}]});
 let expected=MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES+serde_json::to_vec(&orientation).unwrap().len()+serde_json::to_vec(&clock).unwrap().len();assert_eq!(composition_document_byte_limit(&document).unwrap(),expected);
 document["unrelated"]=json!("x".repeat(MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES+1));assert_eq!(composition_document_byte_limit(&document).unwrap(),expected);assert!(serde_json::to_vec(&document).unwrap().len()>expected);
 document["tracks"][0]["clips"][0]["effects"][0]["parameters"]["rollingShutter"]["captureClock"][1]["captureSeconds"]=json!(1.);assert!(composition_document_byte_limit(&document).is_err());
}
#[test]
fn dense_realistic_exposure_tracks_fit_but_aggregate_motion_budget_stays_bounded(){
 let clock:Vec<_>=(0..145000).map(|i|json!({"timeSeconds":i as f64/240.,"captureSeconds":1.+i as f64/240.})).collect();
 let clip=json!({"effects":[{"kind":"spherical_video","parameters":{"input":"dual_fisheye","orientation":[{"timeSeconds":0.,"quaternion":[1.,0.,0.,0.]},{"timeSeconds":605.,"quaternion":[1.,0.,0.,0.]}],"rollingShutter":{"readoutSeconds":0.01536,"captureClock":clock}}}]});
 let mut doc=json!({"tracks":[{"kind":"video","clips":[clip.clone()]}]});let bytes=serde_json::to_vec(&doc).unwrap().len();assert!(bytes>MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES);assert!(bytes<composition_document_byte_limit(&doc).unwrap());
 doc["tracks"][0]["clips"]=json!([clip.clone(),clip.clone(),clip]);assert!(composition_document_byte_limit(&doc).is_err());
}
