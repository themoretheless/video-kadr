use serde_json::Value;
use video_kadr_backend::domain::concat_freeze_splice;
#[test]
#[ignore = "requires original retime and freeze lane command oracles"]
fn complete_clips_match_original_freeze_commands() {
    let mut scenes = 0;
    for var in ["CONCAT_RETIME_COMMAND_ORACLE", "CONCAT_FREEZE_LANE_ORACLE"] {
        let rows: Vec<Value> =
            serde_json::from_slice(&std::fs::read(std::env::var(var).unwrap()).unwrap()).unwrap();
        for row in rows {
            if row["command"]["op"] != "freezeFrame" {
                continue;
            }
            scenes += 1;
            let before = row["before"].as_array().unwrap();
            let after = row["after"].as_array().unwrap();
            let added: Vec<_> = after
                .iter()
                .filter(|clip| !before.iter().any(|old| old["id"] == clip["id"]))
                .collect();
            let (tail, still, media) = if added.is_empty() {
                ("unused-tail", "unused-still", "unused-media")
            } else {
                let tail = added.iter().find(|c| c["kind"] == "video").unwrap();
                let still = added.iter().find(|c| c["kind"] == "image").unwrap();
                (
                    tail["id"].as_str().unwrap(),
                    still["id"].as_str().unwrap(),
                    still["mediaId"].as_str().unwrap(),
                )
            };
            let cmd = &row["command"];
            let actual = concat_freeze_splice::freeze(
                before,
                cmd["clipId"].as_str().unwrap(),
                cmd["time"].as_f64().unwrap(),
                cmd["duration"].as_f64(),
                Some(media),
                tail,
                still,
            )
            .unwrap();
            assert_eq!(
                actual.is_some(),
                row["applied"].as_bool().unwrap(),
                "{}",
                row["name"]
            );
            if let Some(actual) = actual {
                assert_eq!(actual, *after, "{}", row["name"]);
            }
        }
    }
    assert_eq!(scenes, 56);
}

#[test]
fn image_reuses_media_and_preserves_opaque_look_without_mutating_input() {
    let clip = serde_json::json!({"id":"c","trackId":"v","mediaId":"m","kind":"image","start":0.,"duration":4.,"sourceStart":0.,"speed":1.,"fadeIn":0.2,"fadeOut":0.3,"volume":0.5,"filters":[],"videoEffects":[],"flipH":true,"crop":{"left":0.1},"futureLook":{"value":42},"muted":true,"detachedFrom":"audio","transitionIn":{"kind":"crossfade","duration":0.5}});
    let before = vec![clip];
    let unchanged = before.clone();
    let result = concat_freeze_splice::freeze(&before, "c", 1., None, None, "tail", "still")
        .unwrap()
        .unwrap();
    assert_eq!(before, unchanged);
    assert_eq!(result.len(), 3);
    let still = &result[2];
    assert_eq!(still["mediaId"], "m");
    assert_eq!(still["futureLook"], before[0]["futureLook"]);
    assert_eq!(still["crop"], before[0]["crop"]);
    assert_eq!(still["flipH"], true);
    assert_eq!(still["volume"], 1.);
    assert!(
        still.get("muted").is_none()
            && still.get("detachedFrom").is_none()
            && still.get("transitionIn").is_none()
    );
    assert!(result[1].get("transitionIn").is_none());
    assert_eq!(result[0]["transitionIn"], before[0]["transitionIn"]);
    assert!(
        concat_freeze_splice::freeze(&before, "c", 0., None, None, "tail", "still")
            .unwrap()
            .is_none()
    );
    assert!(concat_freeze_splice::freeze(&before, "c", 1., None, None, "c", "still").is_err());
    let mut video = before;
    video[0]["kind"] = serde_json::json!("video");
    assert!(
        concat_freeze_splice::freeze(&video, "c", 1., None, None, "tail", "still")
            .unwrap()
            .is_none()
    );
}
