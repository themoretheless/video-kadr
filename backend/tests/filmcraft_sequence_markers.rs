use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_sequence_markers::run;

#[test]
fn sequence_markers_use_exact_clocks_and_keep_other_sequence_data() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let frame = filmcraft_time::TICKS_PER_SECOND / 24;
    sequence["mark_in"] = json!(frame * 2);
    sequence["mark_out"] = json!(frame * 5);
    let sequence_path = "/project/items/6/kind/Sequence";
    let added = run(
        "markers.addRangeInOut",
        json!({"document":document,"sequence":"6","name":"Range"}),
    )
    .unwrap();
    let marker = &added["result"]["marker"];
    let saved: Value = serde_json::from_str(added["documentText"].as_str().unwrap()).unwrap();
    let q = saved.pointer(sequence_path).unwrap();
    assert_eq!(q["markers"][0]["start"], frame * 2);
    assert_eq!(q["markers"][0]["duration"], frame * 4);
    assert_eq!(
        q["video_tracks"],
        document.pointer(sequence_path).unwrap()["video_tracks"]
    );
    let exact = "9007199254740993";
    let add = run(
        "markers.addChapter",
        json!({"document":saved,"sequence":"6","time":exact}),
    )
    .unwrap();
    let edit = run(
        "markers.edit",
        json!({"document":add["documentText"],"sequence":"6","marker":marker,
        "comment":"Reviewed","color":"mango","durationFrames":"6"}),
    )
    .unwrap();
    let goto = run(
        "markers.goNext",
        json!({"document":edit["documentText"],"sequence":"6","frame":"2"}),
    )
    .unwrap();
    assert_eq!(goto["result"]["time"], exact);
    assert!(goto.get("documentText").is_none());
    let saved: Value = serde_json::from_str(edit["documentText"].as_str().unwrap()).unwrap();
    let markers = &saved.pointer(sequence_path).unwrap()["markers"];
    assert_eq!(markers[0]["color"], "Mango");
    assert_eq!(markers[0]["duration"], frame * 6);
    assert_eq!(markers[1]["name"], "Chapter 1");
    let clear = run(
        "markers.clearCurrent",
        json!({"document":saved,"sequence":"6","marker":marker}),
    )
    .unwrap();
    let cleared = run(
        "markers.clearAll",
        json!({"document":clear["documentText"],"sequence":"6"}),
    )
    .unwrap();
    let saved: Value = serde_json::from_str(cleared["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(saved.pointer(sequence_path).unwrap()["markers"], json!([]));
}
