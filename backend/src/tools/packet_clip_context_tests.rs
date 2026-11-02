//! Actual media/export gates for retained source history and shared-source joins.
use super::*;
use crate::domain::{
    composition::{AudioClip, CompositionTrack},
    filmcraft_dsp::EffectSpec,
};
fn effect(id: &str) -> EffectSpec {
    EffectSpec { concat_source_keys: Default::default(),
        id: id.into(),
        params: if id == "concat.pitch" {
            BTreeMap::from([("semitones".into(), 7.)])
        } else {
            BTreeMap::new()
        },
        ..Default::default()
    }
}
fn clip(project: &mut NestedCompositionProject) -> &mut Vec<AudioClip> {
    let CompositionTrack::Audio { clips, .. } = &mut project
        .compositions
        .get_mut("child")
        .unwrap()
        .content
        .tracks[0]
    else {
        panic!()
    };
    clips
}
async fn fixture(
    rate: u32,
    channels: u32,
    codec: &str,
) -> (
    tempfile::TempDir,
    NestedCompositionProject,
    BTreeMap<SourceId, PcmSource>,
    ProcessRuntime,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.wav");
    let wave = if channels == 1 {
        format!("aevalsrc=1.3*sin(2*PI*317*t):s={rate}:d=1")
    } else {
        format!("aevalsrc=1.3*sin(2*PI*317*t)|0.7*cos(2*PI*997*t):s={rate}:d=1")
    };
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin", "-v", "error", "-f", "lavfi", "-i", &wave, "-c:a", codec,
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let runtime = ProcessRuntime::local_default();
    let probe = super::super::pcm_conform::probe_pcm_metadata(
        &runtime,
        Path::new("ffprobe"),
        &path,
        Duration::from_secs(10),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let (mut project, mut sources) = super::tests::fixture(path.clone());
    project
        .compositions
        .get_mut("child")
        .unwrap()
        .content
        .sources
        .get_mut(&SourceId::parse("tone").unwrap())
        .unwrap()
        .audio_channels = Some(channels);
    sources.insert(
        SourceId::parse("tone").unwrap(),
        PcmSource::from_probe(path, probe).unwrap(),
    );
    (dir, project, sources, runtime)
}
fn reference(
    source: &Path,
    rate: u32,
    channels: u32,
    start: u64,
    end: u64,
    id: &str,
) -> Vec<[f32; 2]> {
    let pitch = if id == "concat.deep" { -3. } else { 7. };
    let ratio = 2f64.powf(pitch / 12.);
    let suffix=match id {"concat.pitch"=>"","concat.chipmunk"=>",highpass=f=120,alimiter=limit=0.94",_=>",highpass=f=55,equalizer=f=140:t=q:w=1.0:g=3.00,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60"};
    let first = (u128::from(start) * u128::from(rate)).div_ceil(1_000_000);
    let last = (u128::from(end) * u128::from(rate)).div_ceil(1_000_000);
    let graph=format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}{suffix}",48000.*ratio,1./ratio);
    let output = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(source)
        .args(["-af", &graph, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .chunks_exact(channels as usize * 4)
        .map(|bytes| {
            let mut out = [0.; 2];
            for ch in 0..channels as usize {
                out[ch] = f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
            }
            out
        })
        .collect()
}
fn sample_tick(time: i64) -> i64 {
    let v = time * 48000;
    v.div_euclid(1_000_000) + i64::from(v.rem_euclid(1_000_000) != 0)
}
fn raw(reference: &[[f32; 2]], clock: i64, origin: i64, channels: u32) -> [f64; 2] {
    let age = clock - sample_tick(origin);
    let value = if age >= 0 {
        reference.get(age as usize).copied().unwrap_or([0.; 2])
    } else {
        [0.; 2]
    };
    if channels == 1 {
        let v = f64::from(value[0]);
        [v * std::f64::consts::FRAC_1_SQRT_2; 2]
    } else {
        value.map(f64::from)
    }
}
async fn mix(
    runtime: &ProcessRuntime,
    dir: &Path,
    project: &NestedCompositionProject,
    sources: &BTreeMap<SourceId, PcmSource>,
    range: Range<u64>,
    frames: u64,
) -> Vec<u8> {
    let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
    let pcm = mix_project_audio_tracks(
        runtime,
        Path::new("ffmpeg"),
        dir,
        project,
        "root",
        range,
        frames,
        sources,
        256 * 1024 * 1024,
        Duration::from_secs(20),
        &CancellationToken::new(),
        &progress,
    )
    .await
    .unwrap();
    std::fs::read(pcm.path()).unwrap()
}
#[tokio::test]
async fn retained_negative_phase_and_preparation_heads_match_original_export() {
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for id in ["concat.pitch", "concat.chipmunk", "concat.deep"] {
            for (phase, declared_head) in [
                (-50000i64, 0u64),
                (-25001, 75003),
                (57777, 0),
                (90001, 75003),
            ] {
                let mut authored = project.clone();
                let p = &mut clip(&mut authored)[0];
                p.dsp_effects = vec![effect(id)];
                p.placement.source_timeline_offset_tick = Some(phase);
                p.placement.source_head_timeline_tick = Some(declared_head);
                let head = declared_head.max(if phase < 0 { phase.unsigned_abs() } else { 0 });
                let original = reference(path, rate, channels, 100000 - head, 600000, id);
                let bytes = mix(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if time >= 250000 && time < (650000 - phase).min(750000) {
                        raw(&original, clock, 150000 - phase - head as i64, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"{id} rate{rate} channels{channels} phase{phase} head{head} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(
                    selected,
                    bytes[4800 * 8..9600 * 8],
                    "{id} retained source context selection"
                );
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("retained source history: {values} stereo output values");
}
#[tokio::test]
async fn odd_crossfade_handles_and_independent_packet_instances_match_original_export() {
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (a_id, b_id) in [
            ("concat.pitch", "concat.chipmunk"),
            ("concat.chipmunk", "concat.deep"),
            ("concat.deep", "concat.pitch"),
        ] {
            let a = reference(path, rate, channels, 100000, 625001, a_id);
            let b = reference(path, rate, channels, 75000, 600000, b_id);
            for curve in ["constant_gain", "constant_power", "exponential_fade"] {
                let mut authored = project.clone();
                let clips = clip(&mut authored);
                clips[0].dsp_effects = vec![effect(a_id)];
                let mut next = clips[0].clone();
                next.id = serde_json::from_value(serde_json::json!("next")).unwrap();
                next.placement.timeline_start_tick = 500000;
                next.dsp_effects = vec![effect(b_id)];
                next.crossfade_in_ticks = 50001;
                next.crossfade_curve = serde_json::from_value(serde_json::json!(curve)).unwrap();
                clips.push(next);
                let bytes = mix(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let p = ((time - 625000) as f64 / 50001.).clamp(0., 1.) as f32;
                    let (outgoing, incoming) = match curve {
                        "constant_power" => (
                            (p * std::f32::consts::FRAC_PI_2).cos(),
                            (p * std::f32::consts::FRAC_PI_2).sin(),
                        ),
                        "exponential_fade" => ((1. - p).powi(3), 1. - (1. - p).powi(3)),
                        _ => (1. - p, p),
                    };
                    let mut expected = [0.; 2];
                    if (250000..675001).contains(&time) {
                        let v = raw(&a, clock, 150000, channels);
                        for ch in 0..2 {
                            expected[ch] += v[ch] * f64::from(outgoing);
                        }
                    }
                    if (625000..750000).contains(&time) {
                        let v = raw(&b, clock, 625000, channels);
                        for ch in 0..2 {
                            expected[ch] += v[ch] * f64::from(incoming);
                        }
                    }
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"{a_id}/{b_id} {curve} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    620000..690000,
                    3360,
                )
                .await;
                assert_eq!(
                    selected,
                    bytes[20160 * 8..23520 * 8],
                    "crossfade selected export must preserve both original DSP clocks"
                );
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("crossfade independent packet instances: {values} stereo output values");
}

#[tokio::test]
async fn ordered_pitch_chains_use_complete_plan_and_preserve_selected_export() {
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for pitches in [
            vec![7f32, -7.],
            vec![-7., 7.],
            vec![12., -12., 3.],
            vec![0., 7., -3.],
            vec![0.; 16],
        ] {
            let mut authored = project.clone();
            clip(&mut authored)[0].dsp_effects = pitches
                .iter()
                .map(|&p| EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.pitch".into(),
                    params: BTreeMap::from([("semitones".into(), p)]),
                    ..Default::default()
                })
                .collect();
            if pitches.len() == 2 {
                clip(&mut authored)[0].dsp_effects.insert(
                    1,
                    EffectSpec { concat_source_keys: Default::default(),
                        id: "concat.deep".into(),
                        bypass: true,
                        ..Default::default()
                    },
                );
            }
            let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
            let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
            let mut graph =
                format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS");
            for &pitch in &pitches {
                let ratio = 2f64.powf(f64::from(pitch) / 12.);
                graph.push_str(&format!(
                    ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                    48000. * ratio,
                    1. / ratio
                ));
            }
            let original = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args(["-af", &graph, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                original.status.success(),
                "{}",
                String::from_utf8_lossy(&original.stderr)
            );
            let reference = original
                .stdout
                .chunks_exact(channels as usize * 4)
                .map(|bytes| {
                    let mut frame = [0.; 2];
                    for ch in 0..channels as usize {
                        frame[ch] =
                            f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                    }
                    frame
                })
                .collect::<Vec<_>>();
            let bytes = mix(
                &runtime,
                dir.path(),
                &authored,
                &sources,
                200000..800000,
                33600,
            )
            .await;
            for (i, frame) in bytes.chunks_exact(8).enumerate() {
                let clock = 9600 + i as i64;
                let time = clock * 1_000_000 / 48000;
                let expected = if (250000..650000).contains(&time) {
                    raw(&reference, clock, 150000, channels)
                } else {
                    [0.; 2]
                };
                for ch in 0..2 {
                    let actual = f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                    assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"ordered {pitches:?} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                    values += 1;
                }
            }
            let selected = mix(
                &runtime,
                dir.path(),
                &authored,
                &sources,
                300000..400000,
                4800,
            )
            .await;
            assert_eq!(
                selected,
                bytes[4800 * 8..9600 * 8],
                "ordered Pitch selected export"
            );
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    println!("ordered Pitch export: {values} stereo output values");
}

#[tokio::test]
async fn ordered_voice_presets_share_one_format_plan_and_match_original_export() {
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        let p = "concat.pitch";
        let c = "concat.chipmunk";
        let d = "concat.deep";
        for ids in [
            vec![p, c],
            vec![c, p],
            vec![d, c],
            vec![c, d],
            vec![d, d],
            vec![p, d, c],
            vec![c, p, d],
            vec![d; 16],
            vec![c; 16],
            (0..16).map(|i| [p, c, d][i % 3]).collect(),
        ] {
            let mut authored = project.clone();
            clip(&mut authored)[0].dsp_effects = ids.iter().map(|id| effect(id)).collect();
            let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
            let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
            let mut graph =
                format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS");
            for &id in &ids {
                let pitch = if id == d { -3. } else { 7. };
                let ratio = 2f64.powf(pitch / 12.);
                graph.push_str(&format!(
                    ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                    48000. * ratio,
                    1. / ratio
                ));
                if id == c {
                    graph.push_str(",highpass=f=120,alimiter=limit=0.94");
                }
                if id == d {
                    graph.push_str(",highpass=f=55,equalizer=f=140:t=q:w=1.0:g=3.00,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60");
                }
            }
            let original = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args(["-af", &graph, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                original.status.success(),
                "{}",
                String::from_utf8_lossy(&original.stderr)
            );
            let reference = original
                .stdout
                .chunks_exact(channels as usize * 4)
                .map(|bytes| {
                    let mut frame = [0.; 2];
                    for ch in 0..channels as usize {
                        frame[ch] =
                            f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                    }
                    frame
                })
                .collect::<Vec<_>>();
            let bytes = mix(
                &runtime,
                dir.path(),
                &authored,
                &sources,
                200000..800000,
                33600,
            )
            .await;
            for (i, frame) in bytes.chunks_exact(8).enumerate() {
                let clock = 9600 + i as i64;
                let time = clock * 1_000_000 / 48000;
                let expected = if (250000..650000).contains(&time) {
                    raw(&reference, clock, 150000, channels)
                } else {
                    [0.; 2]
                };
                for ch in 0..2 {
                    let actual = f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                    assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"ordered {ids:?} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                    values += 1;
                }
            }
            let selected = mix(
                &runtime,
                dir.path(),
                &authored,
                &sources,
                300000..400000,
                4800,
            )
            .await;
            assert_eq!(
                selected,
                bytes[4800 * 8..9600 * 8],
                "ordered voice selected export"
            );
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
    println!("ordered voice export: {values} stereo output values");
}

#[tokio::test]
async fn authored_sweet_original_plans_and_ordered_suffix_export_match_original() {
    let plans: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/concat-sweet-plans.json")).unwrap();
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
            for (copies, suffix) in if case == 4 {
                vec![(1, false), (1, true), (2, false), (16, false)]
            } else {
                vec![(1, false)]
            } {
                let mut authored = project.clone();
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.sweet".into(),
                    params: BTreeMap::from([
                        ("amount".into(), plan["amount"].as_f64().unwrap() as f32),
                        ("pitch".into(), plan["pitch"].as_f64().unwrap() as f32),
                    ]),
                    ..Default::default()
                };
                clip(&mut authored)[0].dsp_effects = vec![spec; copies];
                if suffix {
                    clip(&mut authored)[0]
                        .dsp_effects
                        .push(effect("concat.pitch"));
                }
                let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
                let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
                let repeated = std::iter::repeat(plan["chain"].as_str().unwrap())
                    .take(copies)
                    .collect::<Vec<_>>()
                    .join(",");
                let mut graph = format!(
                    "atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{repeated}"
                );
                if suffix {
                    let ratio = 2f64.powf(7. / 12.);
                    graph.push_str(&format!(
                        ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                        48000. * ratio,
                        1. / ratio
                    ));
                }
                let original = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(path)
                    .args(["-af", &graph, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let reference = original
                    .stdout
                    .chunks_exact(channels as usize * 4)
                    .map(|bytes| {
                        let mut frame = [0.; 2];
                        for ch in 0..channels as usize {
                            frame[ch] =
                                f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                        }
                        frame
                    })
                    .collect::<Vec<_>>();
                let bytes = mix(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if (250000..650000).contains(&time) {
                        raw(&reference, clock, 150000, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"Sweet case{case} copies{copies} suffix{suffix} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(selected, bytes[4800 * 8..9600 * 8], "Sweet selected export");
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("Sweet authored export: {values} stereo output values");
}

async fn mix_loudness(
    runtime: &ProcessRuntime,
    dir: &Path,
    project: &NestedCompositionProject,
    sources: &BTreeMap<SourceId, PcmSource>,
    range: Range<u64>,
    frames: u64,
) -> Vec<u8> {
    let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
    let pcm = mix_project_audio_tracks(
        runtime,
        Path::new("ffmpeg"),
        dir,
        project,
        "root",
        range,
        frames,
        sources,
        1024 * 1024 * 1024,
        Duration::from_secs(20),
        &CancellationToken::new(),
        &progress,
    )
    .await
    .unwrap();
    std::fs::read(pcm.path()).unwrap()
}

#[tokio::test]
async fn authored_loudness_default_and_targets_export_match_original() {
    let plans: serde_json::Value =
        serde_json::json!({"cases":[
            {"target":-30,"chain":"loudnorm=I=-30.0:TP=-1.5:LRA=11"},
            {"target":-23,"chain":"loudnorm=I=-23.0:TP=-1.5:LRA=11"},
            {"target":-12,"chain":"loudnorm=I=-12.0:TP=-1.5:LRA=11"},
            {"target":-23.14,"chain":"loudnorm=I=-23.1:TP=-1.5:LRA=11"}]});
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
            for (copies, suffix) in if case == 4 {
                vec![(1, false), (1, true), (2, false), (16, false)]
            } else {
                vec![(1, false)]
            } {
                let mut authored = project.clone();
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.loudness".into(),
                    params: BTreeMap::from([
                        ("target".into(), plan["target"].as_f64().unwrap() as f32),
                    ]),
                    ..Default::default()
                };
                clip(&mut authored)[0].dsp_effects = vec![spec; copies];
                if suffix {
                    clip(&mut authored)[0]
                        .dsp_effects
                        .push(effect("concat.pitch"));
                }
                let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
                let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
                let repeated = std::iter::repeat(plan["chain"].as_str().unwrap())
                    .take(copies)
                    .collect::<Vec<_>>()
                    .join(",");
                let mut graph = format!(
                    "atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{repeated}"
                );
                if suffix {
                    let ratio = 2f64.powf(7. / 12.);
                    graph.push_str(&format!(
                        ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                        48000. * ratio,
                        1. / ratio
                    ));
                }
                let original = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(path)
                    .args(["-af", &graph, "-ar", "48000", "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let reference = original
                    .stdout
                    .chunks_exact(channels as usize * 4)
                    .map(|bytes| {
                        let mut frame = [0.; 2];
                        for ch in 0..channels as usize {
                            frame[ch] =
                                f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                        }
                        frame
                    })
                    .collect::<Vec<_>>();
                let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
                let rejected = mix_project_audio_tracks(&runtime, Path::new("ffmpeg"), dir.path(), &authored, "root", 200000..800000, 33600, &sources, 256*1024*1024, Duration::from_secs(20), &CancellationToken::new(), &progress).await;
                match rejected { Err(error) => assert!(error.to_string().contains("aggregate staging budget exceeded"),"{error}"), Ok(_) => panic!("Loudness live DSP reservation was not enforced") }
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
                let bytes = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if (250000..650000).contains(&time) {
                        raw(&reference, clock, 150000, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"Loudness case{case} copies{copies} suffix{suffix} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(selected, bytes[4800 * 8..9600 * 8], "Loudness selected export");
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("Loudness authored export: {values} stereo output values");
}

#[tokio::test]
async fn authored_loudness_long_dynamic_and_selected_export_match_original() {
    use crate::domain::composition::{Composition,CanvasSpec,TransformSpec};
    let mut values=0;
    for (rate,channels,codec) in [(44100,2,"pcm_s16le"),(48000,1,"pcm_f32le"),(192000,2,"pcm_f64le")] {
        let (dir,_,mut sources,runtime)=fixture(rate,channels,codec).await;
        let path=sources.values().next().unwrap().path.clone();
        let signal=format!("aevalsrc=if(lt(t\\,2)\\,0.03\\,if(lt(t\\,4)\\,0.8\\,if(lt(t\\,4.5)\\,0\\,if(lt(t\\,6)\\,1.3\\,0.1))))*sin(2*PI*317*t){}:s={rate}:d=9",if channels==2 {"|0.7*cos(2*PI*997*t)"}else{""});
        let generated=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i",&signal,"-c:a",codec]).arg(&path).output().unwrap();
        assert!(generated.status.success(),"{}",String::from_utf8_lossy(&generated.stderr));
        let probe=super::super::pcm_conform::probe_pcm_metadata(&runtime,Path::new("ffprobe"),&path,Duration::from_secs(10),&CancellationToken::new()).await.unwrap();
        sources.insert(SourceId::parse("tone").unwrap(),PcmSource::from_probe(path.clone(),probe).unwrap());
        let mut child=Composition::new(CanvasSpec::default());
        child.sources=serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"audio","durationTicks":9000000,"width":0,"height":0,"hasAudio":true,"audioChannels":channels}})).unwrap();
        child.tracks=serde_json::from_value(serde_json::json!([{"kind":"audio","id":"audio","name":"Audio","clips":[{"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":100000,"sourceOutTick":8100000,"speed":1},"gain":{"mode":"constant","value":0.5},"roleGain":2.0,"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0}]}])).unwrap();
        let project:NestedCompositionProject=serde_json::from_value(serde_json::json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":9000000,"content":Composition::new(CanvasSpec::default()),"instances":[{"id":"group","timing":{"composition":"child","timelineStartUs":250000,"sourceInUs":100000,"sourceOutUs":8100000},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0},"blendMode":"normal"}]},"child":{"durationUs":9000000,"content":child,"instances":[]}}})).unwrap();
        for target in [-23.,-12.] {
            let mut authored=project.clone();clip(&mut authored)[0].dsp_effects=vec![EffectSpec{ concat_source_keys: Default::default(),id:"concat.loudness".into(),params:BTreeMap::from([("target".into(),target)]),..Default::default()}];
            let first=(u128::from(rate)*100000).div_ceil(1000000);let last=(u128::from(rate)*8100000).div_ceil(1000000);
            let graph=format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,loudnorm=I={target:.1}:TP=-1.5:LRA=11");
            let original=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&path).args(["-af",&graph,"-ar","48000","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
            assert!(original.status.success(),"{}",String::from_utf8_lossy(&original.stderr));
            let reference=original.stdout.chunks_exact(channels as usize*4).map(|b|{let mut f=[0.;2];for ch in 0..channels as usize {f[ch]=f32::from_le_bytes(b[ch*4..ch*4+4].try_into().unwrap());}f}).collect::<Vec<_>>();
            let output=mix_loudness(&runtime,dir.path(),&authored,&sources,0..9000000,432000).await;
            for (i,frame) in output.chunks_exact(8).enumerate() {
                let time=i as i64*1000000/48000;
                let expected=if (250000..8250000).contains(&time) {raw(&reference,i as i64,150000,channels)} else {[0.;2]};
                for ch in 0..2 {let actual=f32::from_le_bytes(frame[ch*4..ch*4+4].try_into().unwrap());assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"long Loudness rate{rate} channels{channels} target{target} frame{i} ch{ch}: {actual} != {}",expected[ch]);values+=1;}
            }
            let selected=mix_loudness(&runtime,dir.path(),&authored,&sources,3500000..4500000,48000).await;
            assert_eq!(selected,output[168000*8..216000*8],"dynamic selected export must retain full source history");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
        }
    }
    println!("Long authored Loudness: {values} output values and selected exports match original");
}

#[tokio::test]
async fn authored_two_loudness_order_export_match_original() {
    let plans: serde_json::Value =
        serde_json::json!({"cases":[
            {"target":-30,"chain":"loudnorm=I=-30.0:TP=-1.5:LRA=11"},
            {"target":-23,"chain":"loudnorm=I=-23.0:TP=-1.5:LRA=11"},
            {"target":-12,"chain":"loudnorm=I=-12.0:TP=-1.5:LRA=11"},
            {"target":-23.14,"chain":"loudnorm=I=-23.1:TP=-1.5:LRA=11"}]});
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
            for (copies, suffix) in if case == 0 { vec![(2,false)] } else { continue } {
                let mut authored = project.clone();
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.loudness".into(),
                    params: BTreeMap::from([
                        ("target".into(), plan["target"].as_f64().unwrap() as f32),
                    ]),
                    ..Default::default()
                };
                clip(&mut authored)[0].dsp_effects = vec![spec; copies];
                clip(&mut authored)[0].dsp_effects[1].params.insert("target".into(),-12.);
                if suffix {
                    clip(&mut authored)[0]
                        .dsp_effects
                        .push(effect("concat.pitch"));
                }
                let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
                let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
                let repeated = format!("{},loudnorm=I=-12.0:TP=-1.5:LRA=11",plan["chain"].as_str().unwrap());
                let mut graph = format!(
                    "atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{repeated}"
                );
                if suffix {
                    let ratio = 2f64.powf(7. / 12.);
                    graph.push_str(&format!(
                        ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                        48000. * ratio,
                        1. / ratio
                    ));
                }
                let original = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(path)
                    .args(["-af", &graph, "-ar", "48000", "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let reference = original
                    .stdout
                    .chunks_exact(channels as usize * 4)
                    .map(|bytes| {
                        let mut frame = [0.; 2];
                        for ch in 0..channels as usize {
                            frame[ch] =
                                f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                        }
                        frame
                    })
                    .collect::<Vec<_>>();
                let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
                let rejected = mix_project_audio_tracks(&runtime, Path::new("ffmpeg"), dir.path(), &authored, "root", 200000..800000, 33600, &sources, 256*1024*1024, Duration::from_secs(20), &CancellationToken::new(), &progress).await;
                match rejected { Err(error) => assert!(error.to_string().contains("aggregate staging budget exceeded"),"{error}"), Ok(_) => panic!("Loudness live DSP reservation was not enforced") }
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
                let bytes = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if (250000..650000).contains(&time) {
                        raw(&reference, clock, 150000, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"Loudness case{case} copies{copies} suffix{suffix} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(selected, bytes[4800 * 8..9600 * 8], "Loudness selected export");
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("Loudness authored export: {values} stereo output values");
}


#[tokio::test]
async fn authored_two_loudness_long_dynamic_and_selected_export_match_original() {
    use crate::domain::composition::{Composition,CanvasSpec,TransformSpec};
    let mut values=0;
    for (rate,channels,codec) in [(44100,2,"pcm_s16le"),(48000,1,"pcm_f32le"),(192000,2,"pcm_f64le")] {
        let (dir,_,mut sources,runtime)=fixture(rate,channels,codec).await;
        let path=sources.values().next().unwrap().path.clone();
        let signal=format!("aevalsrc=if(lt(t\\,2)\\,0.03\\,if(lt(t\\,4)\\,0.8\\,if(lt(t\\,4.5)\\,0\\,if(lt(t\\,6)\\,1.3\\,0.1))))*sin(2*PI*317*t){}:s={rate}:d=9",if channels==2 {"|0.7*cos(2*PI*997*t)"}else{""});
        let generated=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i",&signal,"-c:a",codec]).arg(&path).output().unwrap();
        assert!(generated.status.success(),"{}",String::from_utf8_lossy(&generated.stderr));
        let probe=super::super::pcm_conform::probe_pcm_metadata(&runtime,Path::new("ffprobe"),&path,Duration::from_secs(10),&CancellationToken::new()).await.unwrap();
        sources.insert(SourceId::parse("tone").unwrap(),PcmSource::from_probe(path.clone(),probe).unwrap());
        let mut child=Composition::new(CanvasSpec::default());
        child.sources=serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"audio","durationTicks":9000000,"width":0,"height":0,"hasAudio":true,"audioChannels":channels}})).unwrap();
        child.tracks=serde_json::from_value(serde_json::json!([{"kind":"audio","id":"audio","name":"Audio","clips":[{"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":100000,"sourceOutTick":8100000,"speed":1},"gain":{"mode":"constant","value":0.5},"roleGain":2.0,"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0}]}])).unwrap();
        let project:NestedCompositionProject=serde_json::from_value(serde_json::json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":9000000,"content":Composition::new(CanvasSpec::default()),"instances":[{"id":"group","timing":{"composition":"child","timelineStartUs":250000,"sourceInUs":100000,"sourceOutUs":8100000},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0},"blendMode":"normal"}]},"child":{"durationUs":9000000,"content":child,"instances":[]}}})).unwrap();
        for target in [-30.,-12.] {
            let mut authored=project.clone();clip(&mut authored)[0].dsp_effects=vec![EffectSpec{ concat_source_keys: Default::default(),id:"concat.loudness".into(),params:BTreeMap::from([("target".into(),target)]),..Default::default()}];
            clip(&mut authored)[0].dsp_effects.push(EffectSpec{ concat_source_keys: Default::default(),id:"concat.loudness".into(),params:BTreeMap::from([("target".into(),-23.)]),..Default::default()});
            let first=(u128::from(rate)*100000).div_ceil(1000000);let last=(u128::from(rate)*8100000).div_ceil(1000000);
            let graph=format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,loudnorm=I={target:.1}:TP=-1.5:LRA=11,loudnorm=I=-23.0:TP=-1.5:LRA=11");
            let original=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&path).args(["-af",&graph,"-ar","48000","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
            assert!(original.status.success(),"{}",String::from_utf8_lossy(&original.stderr));
            let reference=original.stdout.chunks_exact(channels as usize*4).map(|b|{let mut f=[0.;2];for ch in 0..channels as usize {f[ch]=f32::from_le_bytes(b[ch*4..ch*4+4].try_into().unwrap());}f}).collect::<Vec<_>>();
            let output=mix_loudness(&runtime,dir.path(),&authored,&sources,0..9000000,432000).await;
            for (i,frame) in output.chunks_exact(8).enumerate() {
                let time=i as i64*1000000/48000;
                let expected=if (250000..8250000).contains(&time) {raw(&reference,i as i64,150000,channels)} else {[0.;2]};
                for ch in 0..2 {let actual=f32::from_le_bytes(frame[ch*4..ch*4+4].try_into().unwrap());assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"long Loudness rate{rate} channels{channels} target{target} frame{i} ch{ch}: {actual} != {}",expected[ch]);values+=1;}
            }
            let selected=mix_loudness(&runtime,dir.path(),&authored,&sources,3500000..4500000,48000).await;
            assert_eq!(selected,output[168000*8..216000*8],"dynamic selected export must retain full source history");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
        }
    }
    println!("Long ordered Loudness: {values} output values and selected exports match original");
}


#[tokio::test]
async fn authored_loudness_then_pitch_export_match_original() {
    let plans: serde_json::Value =
        serde_json::json!({"cases":[
            {"target":-30,"chain":"loudnorm=I=-30.0:TP=-1.5:LRA=11"},
            {"target":-23,"chain":"loudnorm=I=-23.0:TP=-1.5:LRA=11"},
            {"target":-12,"chain":"loudnorm=I=-12.0:TP=-1.5:LRA=11"},
            {"target":-23.14,"chain":"loudnorm=I=-23.1:TP=-1.5:LRA=11"}]});
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
            for (copies, suffix) in if case == 0 { vec![(2,false)] } else { continue } {
                let mut authored = project.clone();
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.loudness".into(),
                    params: BTreeMap::from([
                        ("target".into(), plan["target"].as_f64().unwrap() as f32),
                    ]),
                    ..Default::default()
                };
                clip(&mut authored)[0].dsp_effects = vec![spec; copies];
                clip(&mut authored)[0].dsp_effects[1]=effect("concat.pitch");
                if suffix {
                    clip(&mut authored)[0]
                        .dsp_effects
                        .push(effect("concat.pitch"));
                }
                let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
                let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
                let repeated = format!("{},aresample=48000,asetrate=71918.739690,aresample=48000,atempo=0.66741993",plan["chain"].as_str().unwrap());
                let mut graph = format!(
                    "atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{repeated}"
                );
                if suffix {
                    let ratio = 2f64.powf(7. / 12.);
                    graph.push_str(&format!(
                        ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                        48000. * ratio,
                        1. / ratio
                    ));
                }
                let original = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(path)
                    .args(["-af", &graph, "-ar", "48000", "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let reference = original
                    .stdout
                    .chunks_exact(channels as usize * 4)
                    .map(|bytes| {
                        let mut frame = [0.; 2];
                        for ch in 0..channels as usize {
                            frame[ch] =
                                f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                        }
                        frame
                    })
                    .collect::<Vec<_>>();
                let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
                let rejected = mix_project_audio_tracks(&runtime, Path::new("ffmpeg"), dir.path(), &authored, "root", 200000..800000, 33600, &sources, 256*1024*1024, Duration::from_secs(20), &CancellationToken::new(), &progress).await;
                match rejected { Err(error) => assert!(error.to_string().contains("aggregate staging budget exceeded"),"{error}"), Ok(_) => panic!("Loudness live DSP reservation was not enforced") }
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
                let bytes = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if (250000..650000).contains(&time) {
                        raw(&reference, clock, 150000, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"Loudness case{case} copies{copies} suffix{suffix} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(selected, bytes[4800 * 8..9600 * 8], "Loudness selected export");
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("Loudness authored export: {values} stereo output values");
}



#[tokio::test]
async fn authored_pitch_then_loudness_export_match_original() {
    let plans: serde_json::Value =
        serde_json::json!({"cases":[
            {"target":-30,"chain":"loudnorm=I=-30.0:TP=-1.5:LRA=11"},
            {"target":-23,"chain":"loudnorm=I=-23.0:TP=-1.5:LRA=11"},
            {"target":-12,"chain":"loudnorm=I=-12.0:TP=-1.5:LRA=11"},
            {"target":-23.14,"chain":"loudnorm=I=-23.1:TP=-1.5:LRA=11"}]});
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
            for (copies, suffix) in if case == 0 { vec![(2,false)] } else { continue } {
                let mut authored = project.clone();
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.loudness".into(),
                    params: BTreeMap::from([
                        ("target".into(), plan["target"].as_f64().unwrap() as f32),
                    ]),
                    ..Default::default()
                };
                clip(&mut authored)[0].dsp_effects = vec![spec; copies];
                clip(&mut authored)[0].dsp_effects[1]=effect("concat.pitch");
                clip(&mut authored)[0].dsp_effects.swap(0,1);
                if suffix {
                    clip(&mut authored)[0]
                        .dsp_effects
                        .push(effect("concat.pitch"));
                }
                let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
                let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
                let repeated = format!("aresample=48000,asetrate=71918.739690,aresample=48000,atempo=0.66741993,{}",plan["chain"].as_str().unwrap());
                let mut graph = format!(
                    "atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{repeated}"
                );
                if suffix {
                    let ratio = 2f64.powf(7. / 12.);
                    graph.push_str(&format!(
                        ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                        48000. * ratio,
                        1. / ratio
                    ));
                }
                let original = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(path)
                    .args(["-af", &graph, "-ar", "48000", "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let reference = original
                    .stdout
                    .chunks_exact(channels as usize * 4)
                    .map(|bytes| {
                        let mut frame = [0.; 2];
                        for ch in 0..channels as usize {
                            frame[ch] =
                                f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                        }
                        frame
                    })
                    .collect::<Vec<_>>();
                let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
                let rejected = mix_project_audio_tracks(&runtime, Path::new("ffmpeg"), dir.path(), &authored, "root", 200000..800000, 33600, &sources, 256*1024*1024, Duration::from_secs(20), &CancellationToken::new(), &progress).await;
                match rejected { Err(error) => assert!(error.to_string().contains("aggregate staging budget exceeded"),"{error}"), Ok(_) => panic!("Loudness live DSP reservation was not enforced") }
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
                let bytes = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if (250000..650000).contains(&time) {
                        raw(&reference, clock, 150000, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"Loudness case{case} copies{copies} suffix{suffix} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(selected, bytes[4800 * 8..9600 * 8], "Loudness selected export");
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("Loudness authored export: {values} stereo output values");
}



#[tokio::test]
async fn authored_mixed_pitch_loudness_long_dynamic_export_match_original() {
    use crate::domain::composition::{Composition,CanvasSpec,TransformSpec};
    let mut values=0;
    for (rate,channels,codec) in [(44100,2,"pcm_s16le"),(48000,1,"pcm_f32le"),(192000,2,"pcm_f64le")] {
        let (dir,_,mut sources,runtime)=fixture(rate,channels,codec).await;
        let path=sources.values().next().unwrap().path.clone();
        let signal=format!("aevalsrc=if(lt(t\\,2)\\,0.03\\,if(lt(t\\,4)\\,0.8\\,if(lt(t\\,4.5)\\,0\\,if(lt(t\\,6)\\,1.3\\,0.1))))*sin(2*PI*317*t){}:s={rate}:d=9",if channels==2 {"|0.7*cos(2*PI*997*t)"}else{""});
        let generated=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i",&signal,"-c:a",codec]).arg(&path).output().unwrap();
        assert!(generated.status.success(),"{}",String::from_utf8_lossy(&generated.stderr));
        let probe=super::super::pcm_conform::probe_pcm_metadata(&runtime,Path::new("ffprobe"),&path,Duration::from_secs(10),&CancellationToken::new()).await.unwrap();
        sources.insert(SourceId::parse("tone").unwrap(),PcmSource::from_probe(path.clone(),probe).unwrap());
        let mut child=Composition::new(CanvasSpec::default());
        child.sources=serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"audio","durationTicks":9000000,"width":0,"height":0,"hasAudio":true,"audioChannels":channels}})).unwrap();
        child.tracks=serde_json::from_value(serde_json::json!([{"kind":"audio","id":"audio","name":"Audio","clips":[{"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":100000,"sourceOutTick":8100000,"speed":1},"gain":{"mode":"constant","value":0.5},"roleGain":2.0,"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0}]}])).unwrap();
        let project:NestedCompositionProject=serde_json::from_value(serde_json::json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":9000000,"content":Composition::new(CanvasSpec::default()),"instances":[{"id":"group","timing":{"composition":"child","timelineStartUs":250000,"sourceInUs":100000,"sourceOutUs":8100000},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0},"blendMode":"normal"}]},"child":{"durationUs":9000000,"content":child,"instances":[]}}})).unwrap();
        for reversed in [false,true] {
            let target=-12.;
            let mut authored=project.clone();clip(&mut authored)[0].dsp_effects=vec![EffectSpec{ concat_source_keys: Default::default(),id:"concat.loudness".into(),params:BTreeMap::from([("target".into(),target)]),..Default::default()}];
            clip(&mut authored)[0].dsp_effects.push(effect("concat.pitch"));
            if reversed {clip(&mut authored)[0].dsp_effects.swap(0,1);}
            let first=(u128::from(rate)*100000).div_ceil(1000000);let last=(u128::from(rate)*8100000).div_ceil(1000000);
            let loudness=format!("loudnorm=I={target:.1}:TP=-1.5:LRA=11");
            let pitch="aresample=48000,asetrate=71918.739690,aresample=48000,atempo=0.66741993";
            let chain=if reversed {format!("{pitch},{loudness}")}else{format!("{loudness},{pitch}")};
            let graph=format!("atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{chain}");
            let original=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&path).args(["-af",&graph,"-ar","48000","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
            assert!(original.status.success(),"{}",String::from_utf8_lossy(&original.stderr));
            let reference=original.stdout.chunks_exact(channels as usize*4).map(|b|{let mut f=[0.;2];for ch in 0..channels as usize {f[ch]=f32::from_le_bytes(b[ch*4..ch*4+4].try_into().unwrap());}f}).collect::<Vec<_>>();
            let output=mix_loudness(&runtime,dir.path(),&authored,&sources,0..9000000,432000).await;
            for (i,frame) in output.chunks_exact(8).enumerate() {
                let time=i as i64*1000000/48000;
                let expected=if (250000..8150000).contains(&time) {raw(&reference,i as i64,150000,channels)} else {[0.;2]};
                for ch in 0..2 {let actual=f32::from_le_bytes(frame[ch*4..ch*4+4].try_into().unwrap());assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"long Loudness rate{rate} channels{channels} target{target} reversed{reversed} frame{i} ch{ch}: {actual} != {}",expected[ch]);values+=1;}
            }
            let selected=mix_loudness(&runtime,dir.path(),&authored,&sources,3500000..4500000,48000).await;
            assert_eq!(selected,output[168000*8..216000*8],"dynamic selected export must retain full source history");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
        }
    }
    println!("Long mixed Pitch/Loudness: {values} output values and selected exports match original");
}


#[tokio::test]
async fn authored_three_stage_pitch_loudness_export_match_original() {
    let plans: serde_json::Value =
        serde_json::json!({"cases":[
            {"target":-30,"chain":"loudnorm=I=-30.0:TP=-1.5:LRA=11"},
            {"target":-23,"chain":"loudnorm=I=-23.0:TP=-1.5:LRA=11"},
            {"target":-12,"chain":"loudnorm=I=-12.0:TP=-1.5:LRA=11"},
            {"target":-23.14,"chain":"loudnorm=I=-23.1:TP=-1.5:LRA=11"}]});
    let mut values = 0;
    for (rate, channels, codec) in [
        (44100, 2, "pcm_s16le"),
        (48000, 1, "pcm_f32le"),
        (192000, 2, "pcm_f64le"),
    ] {
        let (dir, project, sources, runtime) = fixture(rate, channels, codec).await;
        let path = &sources.values().next().unwrap().path;
        for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
            for (copies, suffix) in if case < 4 { vec![(2,false)] } else { continue } {
                let mut authored = project.clone();
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: "concat.loudness".into(),
                    params: BTreeMap::from([
                        ("target".into(), plan["target"].as_f64().unwrap() as f32),
                    ]),
                    ..Default::default()
                };
                clip(&mut authored)[0].dsp_effects = vec![spec; copies];
                clip(&mut authored)[0].dsp_effects[1]=effect("concat.pitch");
                clip(&mut authored)[0].dsp_effects.push(EffectSpec{ concat_source_keys: Default::default(),id:"concat.loudness".into(),params:BTreeMap::from([("target".into(),-12.)]),..Default::default()});
                let loud=clip(&mut authored)[0].dsp_effects[0].clone();
                let plus=effect("concat.pitch");let mut minus=effect("concat.pitch");minus.params.insert("semitones".into(),-3.);
                if case==1 {clip(&mut authored)[0].dsp_effects=vec![plus,loud,minus];}
                else if case==2 {clip(&mut authored)[0].dsp_effects=vec![loud,plus,minus];}
                else if case==3 {clip(&mut authored)[0].dsp_effects=vec![plus,minus,loud];}

                if suffix {
                    clip(&mut authored)[0]
                        .dsp_effects
                        .push(effect("concat.pitch"));
                }
                let first = (u128::from(rate) * 100000).div_ceil(1_000_000);
                let last = (u128::from(rate) * 600000).div_ceil(1_000_000);
                let plus="aresample=48000,asetrate=71918.739690,aresample=48000,atempo=0.66741993";
                let minus="aresample=48000,asetrate=40363.027932,aresample=48000,atempo=1.18920712";
                let loud=plan["chain"].as_str().unwrap();
                let repeated=match case {0=>format!("{loud},{plus},loudnorm=I=-12.0:TP=-1.5:LRA=11"),1=>format!("{plus},{loud},{minus}"),2=>format!("{loud},{plus},{minus}"),_=>format!("{plus},{minus},{loud}")};
                let mut graph = format!(
                    "atrim=start_sample={first}:end_sample={last},asetpts=PTS-STARTPTS,{repeated}"
                );
                if suffix {
                    let ratio = 2f64.powf(7. / 12.);
                    graph.push_str(&format!(
                        ",aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}",
                        48000. * ratio,
                        1. / ratio
                    ));
                }
                let original = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(path)
                    .args(["-af", &graph, "-ar", "48000", "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let reference = original
                    .stdout
                    .chunks_exact(channels as usize * 4)
                    .map(|bytes| {
                        let mut frame = [0.; 2];
                        for ch in 0..channels as usize {
                            frame[ch] =
                                f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                        }
                        frame
                    })
                    .collect::<Vec<_>>();
                let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
                let rejected = mix_project_audio_tracks(&runtime, Path::new("ffmpeg"), dir.path(), &authored, "root", 200000..800000, 33600, &sources, 256*1024*1024, Duration::from_secs(20), &CancellationToken::new(), &progress).await;
                match rejected { Err(error) => assert!(error.to_string().contains("aggregate staging budget exceeded"),"{error}"), Ok(_) => panic!("Loudness live DSP reservation was not enforced") }
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
                let bytes = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    200000..800000,
                    33600,
                )
                .await;
                for (i, frame) in bytes.chunks_exact(8).enumerate() {
                    let clock = 9600 + i as i64;
                    let time = clock * 1_000_000 / 48000;
                    let expected = if (250000..650000).contains(&time) {
                        raw(&reference, clock, 150000, channels)
                    } else {
                        [0.; 2]
                    };
                    for ch in 0..2 {
                        let actual =
                            f32::from_le_bytes(frame[ch * 4..ch * 4 + 4].try_into().unwrap());
                        assert!((f64::from(actual)-expected[ch]).abs()<=2e-7,"Loudness case{case} copies{copies} suffix{suffix} rate{rate} channels{channels} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                        values += 1;
                    }
                }
                let selected = mix_loudness(
                    &runtime,
                    dir.path(),
                    &authored,
                    &sources,
                    300000..400000,
                    4800,
                )
                .await;
                assert_eq!(selected, bytes[4800 * 8..9600 * 8], "Loudness selected export");
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            }
        }
    }
    println!("Loudness authored export: {values} stereo output values");
}



