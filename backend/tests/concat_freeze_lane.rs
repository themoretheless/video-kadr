use serde_json::Value;
use video_kadr_backend::domain::concat_retime_edit::{freeze_lane, Clock, LanePlacement};
fn placement(value: &Value) -> LanePlacement {
    LanePlacement { id:value["id"].as_str().unwrap().into(), track_id:value["trackId"].as_str().unwrap().into(), clock:serde_json::from_value(value.clone()).unwrap() }
}
#[test]
#[ignore = "requires scripts/qualify-concat-freeze-lane.py"]
fn matches_original_freeze_lane_including_overlaps_other_tracks_and_clamps() {
    let data=std::fs::read(std::env::var("CONCAT_FREEZE_LANE_ORACLE").unwrap()).unwrap();
    let rows:Vec<Value>=serde_json::from_slice(&data).unwrap();
    assert_eq!(rows.len(),50);
    let mut applied=0;
    for row in rows {
        assert_eq!(row["undoRedoVerified"],true);
        let before:Vec<_>=row["before"].as_array().unwrap().iter().map(placement).collect();
        let after=row["after"].as_array().unwrap();
        let cmd=&row["command"];
        let result=freeze_lane(&before,"target",cmd["time"].as_f64().unwrap(),cmd["duration"].as_f64()).unwrap();
        assert_eq!(result.is_some(),row["applied"].as_bool().unwrap(),"{}",row["name"]);
        let Some(plan)=result else { assert_eq!(row["before"],row["after"]); continue };
        applied+=1;
        for item in &before {
            let expected:Clock=serde_json::from_value(after.iter().find(|c|c["id"]==item.id).unwrap().clone()).unwrap();
            let actual=if item.id=="target" { &plan.freeze.head } else { &plan.shifted.iter().find(|c|c.id==item.id).unwrap_or(item).clock };
            assert_eq!(*actual,expected,"{} {}",row["name"],item.id);
        }
        let created:Vec<_>=after.iter().filter(|c|!before.iter().any(|old|c["id"]==old.id)).collect();
        assert_eq!(created.len(),2);
        for (kind,clock) in [("video",&plan.freeze.tail),("image",&plan.freeze.still)] {
            let actual:Clock=serde_json::from_value((*created.iter().find(|c|c["kind"]==kind).unwrap()).clone()).unwrap();
            assert_eq!(*clock,actual,"{} {kind}",row["name"]);
        }
    }
    assert_eq!(applied,10);
}
#[test]
fn invalid_identity_is_rejected_before_planning() {
    let c=LanePlacement{id:"c".into(),track_id:"v".into(),clock:Clock{start:0.,duration:3.,source_start:0.,speed:1.,speed_curve:None}};
    assert!(freeze_lane(&[c.clone(),c.clone()],"c",1.,None).is_err());
    assert!(freeze_lane(&[c],"missing",1.,None).is_err());
}
