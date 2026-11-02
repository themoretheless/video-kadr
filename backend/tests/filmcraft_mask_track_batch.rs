use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::{
    filmcraft_mask_track_batch::{execute, Plan},
    group_surface::GroupSurface,
};
mod inputs {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scripts/reference/filmcraft_mask_tracking_frames.rs"
    ));
}
const FD: i64 = 10_584_000_000;
fn oracle() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/filmcraft-mask-track-batch-oracle.json"
    ))
    .unwrap()
}
fn params(o: &Value, c: &Value) -> Value {
    let mut d = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
    let track = &mut d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]
        ["video_tracks"][0];
    track["locked"] = c["locked"].clone();
    track["items"][0] = c["clip"].clone();
    let mut p = c["params"].clone();
    p["document"] = d;
    p["sequence"] = o["sequence"].clone();
    p["context"] = json!({"playhead":c["playhead"].as_i64().unwrap().to_string()});
    p
}
#[test]
fn sequential_paths_and_partial_results_match_actual_original_engine_jobs() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 66);
    for (i, c) in o["cases"].as_array().unwrap().iter().enumerate() {
        let p = params(&o, c);
        let plan = Plan::read(&p).unwrap();
        assert_eq!(plan.steps, c["steps"].as_u64().unwrap() as usize);
        let mode = c["mode"].as_str().unwrap();
        let (w, h) = (
            c["width"].as_u64().unwrap() as usize,
            c["height"].as_u64().unwrap() as usize,
        );
        let result = execute(
            plan,
            |t| {
                let step = t.div_euclid(FD);
                if mode == "firstDecode" || (mode == "decode" && step >= 2) {
                    anyhow::bail!("decoder failed");
                }
                Ok(GroupSurface {
                    width: w,
                    height: h,
                    rgba: inputs::frame(w, h, step, mode),
                })
            },
            |done| mode == "cancel" && done >= 2,
        )
        .unwrap();
        assert_eq!(
            result.done,
            c["done"].as_u64().unwrap() as usize,
            "done case {i}"
        );
        assert_eq!(
            result.error,
            c["error"].as_str().map(str::to_owned),
            "error case {i}"
        );
        assert_eq!(result.stopped, mode == "cancel");
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&result.document.envelope["project"]).unwrap())
        );
        assert_eq!(hash, c["projectSha256"], "case {i}: {mode} {}", c["params"]);
    }
}
#[test]
fn tracking_bounds_context_method_overrides_and_cancellation_preserve_source_snapshots() {
    let o = oracle();
    let p = params(&o, &o["cases"][0]);
    let mut zero = p.clone();
    zero["frames"] = json!(0);
    assert!(Plan::read(&zero).is_err());
    let mut invalid = p.clone();
    invalid["direction"] = json!("FORWARD");
    assert!(Plan::read(&invalid).is_err());
    invalid = p.clone();
    invalid["method"] = json!("wrong");
    assert!(Plan::read(&invalid).is_err());
    let mut inferred = p.clone();
    inferred.as_object_mut().unwrap().remove("clip");
    inferred.as_object_mut().unwrap().remove("effect");
    inferred.as_object_mut().unwrap().remove("mask");
    inferred["context"]["selectedMask"] =
        json!({"clip":o["cases"][0]["clip"]["id"],"effect":0,"mask":0});
    assert_eq!(Plan::read(&inferred).unwrap().steps, 4);
    let plan = Plan::read(&p).unwrap();
    let from = plan.from;
    let path = plan.parameter_path.clone();
    let result = execute(
        plan,
        |t| {
            Ok(GroupSurface {
                width: 240,
                height: 180,
                rgba: inputs::frame(240, 180, t / FD, "translation"),
            })
        },
        |_| true,
    )
    .unwrap();
    assert_eq!(result.done, 0);
    assert!(result.stopped);
    assert!(result.error.is_none());
    assert_eq!(
        result.document.envelope.pointer(&path).unwrap()["keyframes"][0]["time"],
        from
    );
}
