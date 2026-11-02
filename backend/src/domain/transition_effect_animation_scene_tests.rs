//! All first-scalar animation oracles through the shared stack and actual clip context.
//! Clock values are supplied from the source oracle: this is not project clock admission proof.
use super::*;
use crate::domain::{
    animation_sample_time::AnimationSampleTime,
    composition::{AnimatableValue, VideoEffect},
    effect_frame_clock::EffectFrameClock,
    filmcraft_video_project::{partition_adapted_effects, with_scalar_animation},
    linear_color_frame::LinearColorFrame,
    transition_spec::TransitionSpec,
};

#[test]
fn all_119_animated_effects_match_original_4284_scene_frames() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-transition-side-animation.json"
    ))
    .unwrap();
    assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4284);
    let adapters: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-video-default-adapters.json"
    ))
    .unwrap();
    let mut failures = std::collections::BTreeMap::<String, (usize, String)>::new();
    for case in cases {
        let run = || -> Result<()> {
            let wire = adapters["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["source"] == case["effect"])
                .unwrap()["native"]
                .clone();
            let curve: AnimatableValue = serde_json::from_value(
                serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":18000000,"value":case["low"]},{"tick":32000000,"value":case["high"]}]}}),
            )?;
            let effect = with_scalar_animation(
                serde_json::from_value::<VideoEffect>(wire)?,
                case["parameter"].as_str().unwrap(),
                curve,
            )?;
            let partition = partition_adapted_effects(vec![effect])?;
            let parse = |field: &str| case[field].as_str().unwrap().parse::<i64>().unwrap();
            let local = parse("localTime");
            let source = parse("sourceTime");
            let parameter = parse("effectTime");
            let at = parse("at");
            let mode = case["mode"].as_u64().unwrap();
            let hold = case["hold"].as_u64().unwrap();
            let mut composition = Composition::new(Default::default());
            composition.canvas.width = 4;
            composition.canvas.height = 4;
            composition.canvas.fps_milli = 23976;
        composition.canvas.fps_rate = Some(super::super::composition::FrameRateRatio { numerator: 24000, denominator: 1001 });
            composition.sources = serde_json::from_value(
                serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false}}),
            )?;
            let clip: crate::domain::composition::VideoClip = serde_json::from_value(
                serde_json::json!({"id":"clip","sourceId":"source","placement":{"timelineStartTick":mode*10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"playbackMode":if hold>0 {serde_json::json!({"mode":"freeze","sourceTick":25000000})} else {serde_json::json!({"mode":"forward"})},"transform":crate::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":partition.media_effects}),
            )?;
            composition.metadata = Some(crate::domain::metadata_burnin::CompositionMetadata {
            markers: Default::default(),
            media_versions: Default::default(),
            clip_links: Default::default(),
            concat_lane_groups: Default::default(),
                sequence_name: "s".into(),
                clip_names: [(clip.id.clone(), "matte".into())].into_iter().collect(),
                source_names: [(clip.source_id.clone(), "matte".into())]
                    .into_iter()
                    .collect(),
                drop_frame: false,
                sequence_start_frame: 0,
            });
            let rgba = if mode == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            let picture = || GroupSurface {
                width: 4,
                height: 4,
                rgba: rgba.repeat(16),
            };
            let mut decode =
                |_: &crate::domain::composition::SourceId, _: u64, _: bool, _: &crate::domain::composition::CompositionClipId| Ok(picture());
            let mut context = CompositionEffectContext {
                exact_sample: Some(EffectContextSample {
                    timeline_tick: at,
                    clocks: EffectFrameClock {
                        frame: source,
                        parameters: parameter,
                        generator: local,
                    },
                }),
                temporal_clip: Some(TemporalClip::Video(&clip)),
                clip_duration_us: 10000000,
                composition: &composition,
                time_us: (at / 254016) as u64,
                depth: 0,
                library: &[],
                matrix: GroupAffine::IDENTITY,
                image: None,
                video: Some(&mut decode),
            };
            let rendered = apply_effect_stack_seeded_at_time(
                &mut picture(),
                &clip.effects,
                0,
                AnimationSampleTime::FilmcraftSourceTicks(parameter),
                local as f64 / 254016000000.,
                &[],
                Some(&mut context),
                &mut || Ok(()),
                None,
            )?;
            let other = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: 4,
                    height: 4,
                    rgba: if mode == 0 {
                        [0, 0, 255, 255].repeat(16)
                    } else {
                        [255, 0, 0, 255].repeat(16)
                    },
                },
                &mut || Ok(()),
            )?;
            let (a, b) = if mode == 0 {
                (rendered, other)
            } else {
                (other, rendered)
            };
            let progress = (at - 9 * 254016000000) as f64 / (2 * 254016000000i64) as f64;
            let output = crate::domain::transition_procedural::render(
                &TransitionSpec {
                    effect: "cross_dissolve".into(),
                    params: Default::default(),
                },
                &a,
                &b,
                progress as f32,
                case["reverse"].as_bool().unwrap(),
                &mut || Ok(()),
            )?;
            let expected: Vec<u32> = serde_json::from_value(case["bits"].clone())?;
            anyhow::ensure!(
                output
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
                    == expected,
                "pixel mismatch hold{hold} side{mode} at{at}"
            );
            Ok(())
        };
        if let Err(error) = run() {
            let entry = failures
                .entry(case["effect"].as_str().unwrap().into())
                .or_insert((0, format!("{error:#}")));
            entry.0 += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "119animated source-effect failures: {failures:#?}"
    );
}
