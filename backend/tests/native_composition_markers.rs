use serde_json::json;
use video_kadr_backend::domain::metadata_burnin::{CompositionMetadata,validate_markers};
#[test]
fn native_markers_round_trip_and_validate_without_impacting_old_metadata(){
 let old:CompositionMetadata=serde_json::from_value(json!({})).unwrap();assert!(old.markers.is_empty());assert!(serde_json::to_value(old).unwrap().get("markers").is_none());
 let value=json!({"sequenceName":"Native","markers":[{"id":"beat","tick":1250000,"label":"Точка 🎬","color":"#aabbcc","origin":"auto_beat"}]});
 let metadata:CompositionMetadata=serde_json::from_value(value.clone()).unwrap();validate_markers(&metadata.markers).unwrap();assert_eq!(serde_json::to_value(&metadata).unwrap()["markers"],value["markers"]);
 for field in [json!({"id":"a","tick":-1,"label":"Bad"}),json!({"id":"a","tick":0,"label":" "}),json!({"id":"a","tick":0,"label":"Bad","color":"red"})]{let m:CompositionMetadata=serde_json::from_value(json!({"markers":[field]})).unwrap();assert!(validate_markers(&m.markers).is_err());}
 let duplicate:CompositionMetadata=serde_json::from_value(json!({"markers":[{"id":"a","tick":0,"label":"A"},{"id":"a","tick":1,"label":"B"}]})).unwrap();assert!(validate_markers(&duplicate.markers).is_err());
}
