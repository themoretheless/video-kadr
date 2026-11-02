use serde_json::Value;
use video_kadr_backend::domain::concat_retime_edit::{Clock, Point};
fn clock(value: &Value) -> Clock {
    serde_json::from_value(value.clone()).unwrap()
}
#[test]
#[ignore = "requires scripts/qualify-concat-retime-commands.py"]
fn native_clocks_match_actual_original_commands() {
    let path =
        std::env::var("CONCAT_RETIME_COMMAND_ORACLE").expect("build original command oracle");
    let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(rows.len(), 62);
    let mut changed = 0;
    for row in rows {
        assert_eq!(row["undoRedoVerified"], true);
        let before = row["before"].as_array().unwrap();
        let after = row["after"].as_array().unwrap();
        let original = clock(before.iter().find(|c| c["id"] == "c").unwrap());
        let expected = clock(after.iter().find(|c| c["id"] == "c").unwrap());
        let command = &row["command"];
        let actual = match command["op"].as_str().unwrap() {
            "setClipSpeedCurve" => original
                .set_curve(serde_json::from_value(command["curve"].clone()).unwrap())
                .unwrap(),
            "splitClips" => match original.split(command["time"].as_f64().unwrap()).unwrap() {
                None => {
                    assert_eq!(row["applied"], false);
                    original.clone()
                }
                Some((head, tail)) => {
                    let expected_tail = clock(
                        after
                            .iter()
                            .find(|c| c["id"] != "c" && c["id"] != "later")
                            .unwrap(),
                    );
                    assert_eq!(tail, expected_tail, "{} tail", row["name"]);
                    head
                }
            },
            "trimClip" => {
                let delta = command["delta"].as_f64().unwrap();
                let ripple = command["ripple"].as_bool().unwrap();
                let trim = if command["edge"] == "start" {
                    original.trim_start(delta, 0., ripple)
                } else {
                    original.trim_end(delta, Some(8.), ripple)
                }
                .unwrap();
                assert_eq!(
                    after.iter().find(|c| c["id"] == "later").unwrap()["start"]
                        .as_f64()
                        .unwrap(),
                    8. + trim.ripple_by,
                    "{} ripple",
                    row["name"]
                );
                trim.clock
            }
            "freezeFrame" => match original
                .freeze(
                    command["time"].as_f64().unwrap(),
                    command["duration"].as_f64(),
                )
                .unwrap()
            {
                None => {
                    assert_eq!(row["applied"], false);
                    original.clone()
                }
                Some(freeze) => {
                    let pieces: Vec<_> = after
                        .iter()
                        .filter(|c| c["id"] != "c" && c["id"] != "later")
                        .collect();
                    assert_eq!(pieces.len(), 2);
                    assert!(pieces.iter().any(|c| clock(c) == freeze.tail));
                    assert!(pieces.iter().any(|c| clock(c) == freeze.still));
                    assert_eq!(
                        after.iter().find(|c| c["id"] == "later").unwrap()["start"]
                            .as_f64()
                            .unwrap(),
                        8. + freeze.ripple_by
                    );
                    freeze.head
                }
            },
            _ => panic!("unknown command"),
        };
        assert_eq!(actual, expected, "{} clock", row["name"]);
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&expected).unwrap(),
            "{} serialized clock bits",
            row["name"]
        );
        changed += usize::from(row["applied"] == true);
    }
    assert!(changed > 20);
    println!(
        "62 actual original command clocks, including split/trim/curve/freeze and source undo/redo"
    );
}
#[test]
fn invalid_edits_are_atomic_and_noop_cuts_keep_the_curve() {
    let c = Clock {
        start: 2.,
        duration: 3.,
        source_start: 1.5,
        speed: 1.75,
        speed_curve: Some(vec![
            Point { at: 0., speed: 1. },
            Point { at: 1., speed: 3. },
        ]),
    };
    let before = c.clone();
    assert!(c.split(2.).unwrap().is_none());
    assert!(c.split(5.).unwrap().is_none());
    assert!(c.split(f64::NAN).is_err());
    assert!(c.trim_start(f64::INFINITY, 0., false).is_err());
    assert!(c.trim_end(1., Some(4.), false).is_err());
    assert!(c.freeze(3., Some(f64::NAN)).is_err());
    assert!(c
        .set_curve(Some(vec![Point {
            at: f64::NAN,
            speed: 1.
        }]))
        .is_err());
    assert_eq!(c, before);
    let trimmed = c.trim_end(-1., None, false).unwrap().clock;
    assert_eq!(trimmed.speed_curve, c.speed_curve);
    let (head, tail) = c.split(3.).unwrap().unwrap();
    assert!(head.speed_curve.is_none() && tail.speed_curve.is_none());
    assert_eq!(tail.source_start, 3.25);
}
