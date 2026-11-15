use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_poster_frame::{edit, stored_tick};

#[test]
fn poster_target_source_clock_selection_and_clear_preserve_snapshot() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-options-oracle.json")).unwrap();
    let original =
        json!({"format":"filmcraft.project","schema_version":12,"project":fixture["projects"][0]});
    let mut document = original.clone();
    let media = &mut document["project"]["items"]["5"]["kind"]["Media"];
    media["interpret"]["frame_rate"] = json!({"num":30000,"den":1001});
    let context = json!({"sourceItem":"5","sourcePlayhead":"101677220000","projectSelection":["5"],"activeSequence":"21"});
    let set = edit(
        "clip.setPosterFrame",
        json!({"document":document,"context":context,"time":"2"}),
    )
    .unwrap();
    assert_eq!(
        set["result"],
        json!({"item":5,"posterFrame":93_232_339_200_i64})
    );
    assert_eq!(
        stored_tick(&set["document"]["project"]["items"]["5"]).as_deref(),
        Some("93232339200")
    );
    let clear = edit(
        "clip.clearPosterFrame",
        json!({"document":set["documentText"],"context":context}),
    )
    .unwrap();
    assert_eq!(clear["result"], json!({"item":5,"posterFrame":null}));
    assert_eq!(clear["document"], document);

    let selected = edit("clip.setPosterFrame", json!({"document":document,"time":"-1","context":{"sourceItem":"5","sourcePlayhead":"0","projectSelection":["49"]}})).unwrap();
    assert_eq!(
        selected["result"],
        json!({"item":49,"posterFrame":-10_594_584_000_i64})
    );
    assert!(edit("clip.setPosterFrame", json!({"document":document})).is_err());
    assert!(edit(
        "clip.setPosterFrame",
        json!({"document":document,"item":"999"})
    )
    .is_err());
}
