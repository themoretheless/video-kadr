use serde_json::json;
use video_kadr_backend::domain::{composition::VideoEffect,concat_exposure::{sample_concat_keys,ConcatExposure},concat_effect_upgrade::{ParamKey,AppliedFilter,native_effect}};
#[test]
fn computed_values_roundtrip_outside_sliders_but_authored_values_keep_bounds(){
 for(kind,keys)in [("concat_exposure",json!([{ "at":0,"value":9.5,"derived":true}])),("concat_contrast",json!([{ "at":0,"value":237.5,"derived":true}])),("concat_saturation",json!([{ "at":0,"value":237.5,"derived":true}])),("concat_white_balance",json!({"tint":[{"at":0,"value":237.5,"derived":true}]})),("concat_color_wheels",json!({"lift.x":[{"at":0,"value":2.375,"derived":true}]})),("concat_adjust",json!({"lift.x":[{"at":0,"value":2.375,"derived":true}]}))]{
  let value=json!({"kind":kind,"parameters":{},"concatKeys":keys});let effect:VideoEffect=serde_json::from_value(value.clone()).unwrap();native_effect(&source(&value)).unwrap().unwrap();let encoded=serde_json::to_value(&effect).unwrap()["concatKeys"].clone();if encoded.is_array(){assert_eq!(serde_json::from_value::<Vec<ParamKey>>(encoded).unwrap(),serde_json::from_value::<Vec<ParamKey>>(value["concatKeys"].clone()).unwrap());}else{assert_eq!(serde_json::from_value::<std::collections::BTreeMap<String,Vec<ParamKey>>>(encoded).unwrap(),serde_json::from_value::<std::collections::BTreeMap<String,Vec<ParamKey>>>(value["concatKeys"].clone()).unwrap());}
  let mut authored=value;let keys=&mut authored["concatKeys"];if let Some(run)=keys.as_array_mut(){run[0].as_object_mut().unwrap().remove("derived");}else{for run in keys.as_object_mut().unwrap().values_mut(){run[0].as_object_mut().unwrap().remove("derived");}}
  assert!(native_effect(&source(&authored)).is_err(),"{kind}");
 }
}
#[test]
fn derived_flag_does_not_disable_finite_runtime_admission(){
 let key=ParamKey{at:0.,value:1e300,ease:[0.,0.,1.,1.],derived:true};assert!(sample_concat_keys(ConcatExposure::default(),&[key],0.).is_err());
 let key=ParamKey{at:0.,value:9.5,ease:[0.,0.,1.,1.],derived:true};assert_eq!(sample_concat_keys(ConcatExposure::default(),&[key],0.).unwrap(),9.5);
}

fn source(value:&serde_json::Value)->AppliedFilter{let kind=value["kind"].as_str().unwrap();let id=format!("concat.{}",kind.strip_prefix("concat_").unwrap().replace('_',"-"));let keys=if value["concatKeys"].is_array(){let channel=if kind=="concat_exposure"{"stops"}else{"amount"};json!({channel:value["concatKeys"]})}else{value["concatKeys"].clone()};serde_json::from_value(json!({"id":id,"keys":keys})).unwrap()}
