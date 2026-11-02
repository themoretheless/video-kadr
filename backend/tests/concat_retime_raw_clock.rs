use video_kadr_backend::domain::{
    composition::AudioClip,
    concat_retime::{expand_audio, SourceClock},
    concat_retime_edit::{Clock, Point},
};
fn raw(index: usize) -> Clock {
    let (start, duration, source_start) = [
        (0.0199996, 2.01900049, 0.0199996),
        (0.65065049, 2.01900049, 1.01900049),
        (0.02100049, 2.0199996, 1.01900049),
    ][index];
    Clock {
        start,
        duration,
        source_start,
        speed: 1.75,
        speed_curve: Some(vec![
            Point { at: 0., speed: 1. },
            Point { at: 1., speed: 3. },
        ]),
    }
}
#[test]
fn original_seconds_survive_wire_and_stale_edits_refuse() {
    let p = raw(0).placement(25, 1).unwrap();
    let wire = serde_json::to_string(&p).unwrap();
    let decoded: video_kadr_backend::domain::composition::ClipPlacement =
        serde_json::from_str(&wire).unwrap();
    assert_eq!(
        decoded.concat_retime.as_ref().unwrap().document_clock,
        Some(raw(0))
    );
    assert_eq!(
        decoded.timeline_duration_ticks().unwrap(),
        p.timeline_duration_ticks().unwrap()
    );
    for kind in 0..5 {
        let mut changed = serde_json::to_value(&p).unwrap();
        match kind {
            0 => changed["timelineStartTick"] = serde_json::json!(p.timeline_start_tick + 1),
            1 => changed["sourceInTick"] = serde_json::json!(p.source_in_tick + 1),
            2 => changed["speed"] = serde_json::json!(2.),
            3 => changed["concatRetime"]["documentClock"]["duration"] = serde_json::json!(3.),
            _ => {
                changed["concatRetime"]["documentClock"]["speedCurve"][0]["speed"] =
                    serde_json::json!(2.)
            }
        }
        let changed: video_kadr_backend::domain::composition::ClipPlacement =
            serde_json::from_value(changed).unwrap();
        assert!(
            changed.timeline_duration_ticks().is_err(),
            "stale edit {kind}"
        );
    }
    let mut legacy = serde_json::to_value(&p).unwrap();
    legacy["concatRetime"]
        .as_object_mut()
        .unwrap()
        .remove("documentClock");
    legacy["sourceOutTick"] = serde_json::json!(p.source_in_tick + 4_038_000);
    let legacy: video_kadr_backend::domain::composition::ClipPlacement =
        serde_json::from_value(legacy).unwrap();
    assert!(legacy.timeline_duration_ticks().is_ok());
}
#[test]
#[ignore = "requires updated scripts/qualify-concat-retime.py raw clock records"]
fn raw_video_samples_and_audio_pieces_match_original() {
    let text = std::fs::read_to_string(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).unwrap();
    let mut frames = 0;
    let mut audio = 0;
    let mut premature_rounding_changes = 0;
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if !matches!(fields[0], "8" | "9") {
            continue;
        }
        let index: usize = fields[1].parse().unwrap();
        let document = raw(index);
        if fields[0] == "8" {
            let p = document
                .placement(fields[2].parse().unwrap(), fields[3].parse().unwrap())
                .unwrap();
            let clock = p.concat_retime.as_ref().unwrap();
            let exact = fields[5].parse().unwrap();
            let expected = fields[6].parse().unwrap();
            assert_eq!(
                clock
                    .sample(p.timeline_start_tick, p.source_in_tick, exact)
                    .unwrap(),
                Some(expected),
                "{line}"
            );
            let mut legacy: SourceClock = clock.clone();
            legacy.document_clock = None;
            premature_rounding_changes += usize::from(
                legacy
                    .sample(p.timeline_start_tick, p.source_in_tick, exact)
                    .unwrap()
                    != Some(expected),
            );
            frames += 1;
        } else {
            let clip:AudioClip=serde_json::from_value(serde_json::json!({"id":"raw-clock","sourceId":"source","placement":document.placement(25,1).unwrap(),"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":350000,"fadeOutTicks":270000})).unwrap();
            let pieces = expand_audio(&clip).unwrap();
            let piece = pieces[fields[2].parse::<usize>().unwrap()].source;
            for (actual, expected) in [
                piece.start_seconds,
                piece.duration_seconds,
                piece.source_start_seconds,
                piece.speed,
                piece.fade_in_seconds,
                piece.fade_out_seconds,
            ]
            .into_iter()
            .zip(&fields[3..])
            {
                assert_eq!(
                    actual.to_bits(),
                    expected.parse::<f64>().unwrap().to_bits(),
                    "{line}: {actual}"
                );
            }
            audio += 1;
        }
    }
    assert_eq!(frames, 477);
    assert_eq!(audio, 63);
    assert!(premature_rounding_changes > 0);
    println!("{frames} original video samples and {audio} audio pieces exact; {premature_rounding_changes} negative-control frame mismatches after premature microsecond rounding");
}

#[test]
fn signed_zero_in_duplicate_positions_is_part_of_curve_ownership() {
    let mut document = raw(0);
    document.speed_curve = Some(vec![
        Point { at: 0., speed: 1. },
        Point { at: -0., speed: 3. },
        Point { at: 1., speed: 2. },
    ]);
    let placement = document.placement(25, 1).unwrap();
    assert_eq!(
        placement
            .concat_retime
            .as_ref()
            .unwrap()
            .curve()
            .unwrap()
            .mean(),
        1.5
    );
    let mut stale = placement;
    stale
        .concat_retime
        .as_mut()
        .unwrap()
        .document_clock
        .as_mut()
        .unwrap()
        .speed_curve
        .as_mut()
        .unwrap()[0]
        .at = -0.;
    let points = stale
        .concat_retime
        .as_ref()
        .unwrap()
        .document_clock
        .as_ref()
        .unwrap()
        .speed_curve
        .as_ref()
        .unwrap();
    assert_eq!(
        video_kadr_backend::domain::concat_retime::TimelineCurve::new(
            &points.iter().map(|p| (p.at, p.speed)).collect::<Vec<_>>()
        )
        .unwrap()
        .mean(),
        2.5
    );
    assert!(stale.timeline_duration_ticks().is_err());
}
