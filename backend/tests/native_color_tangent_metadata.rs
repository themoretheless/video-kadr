use serde_json::json;
use video_kadr_backend::domain::animated_color::{ColorSampleRequest,sample_colors};
#[test]
fn color_authoring_modes_round_trip_without_changing_explicit_render_coordinates(){
 let base=json!({"track":{"keys":[{"t_us":-0.5,"value":{"r":255,"g":0,"b":0,"a":255},"in_":[0.6,0.6],"out":[0.3,0.2],"segment":{"kind":"Spline"}},{"t_us":1000000,"value":{"r":0,"g":0,"b":255,"a":255},"in_":[0.7,0.9],"out":[0.3,0.3],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}},"default":{"r":0,"g":0,"b":0,"a":255},"times_us":[0,250000,500000,1000000]});
 let old:ColorSampleRequest=serde_json::from_value(base.clone()).unwrap();
 let mut authored=base.clone();
 authored["track"]["keys"][0]["tangent_modes"]=json!({"incoming":"auto","outgoing":"free","continuity":"smooth"});
 let request:ColorSampleRequest=serde_json::from_value(authored.clone()).unwrap();
 assert_eq!(serde_json::to_value(&request.track).unwrap()["keys"][0]["tangent_modes"],authored["track"]["keys"][0]["tangent_modes"]);
 assert_eq!(sample_colors(&old).unwrap(),sample_colors(&request).unwrap());
 authored["track"]["keys"][0]["tangent_modes"]["incoming"]=json!("unknown");
 assert!(serde_json::from_value::<ColorSampleRequest>(authored).is_err());
}
