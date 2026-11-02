//! Actual FFmpeg graphs assembled from the pinned original compiler manifests.
use std::io::Write;
use video_kadr_backend::domain::filmcraft_dsp::{Chain, EffectSpec};
#[test]
#[ignore = "requires actual FFmpeg format negotiation"]
fn double_echo_and_stadium_lowpass_preserve_native_transport() {
    let tones: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-tones-pcm.json")).unwrap();
    let stadium: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-stadium-pcm.json")).unwrap();
    let rooms: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-reflections-pcm.json")).unwrap();
    let find = |manifest: &serde_json::Value, id: &str| -> serde_json::Value {
        manifest["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["rate"] == 48000
                    && c["channels"] == 2
                    && c.get("signal").and_then(|s| s.as_str()).unwrap_or("noise") == "noise"
                    && c["settings"].as_array().unwrap().len() == 1
                    && c["settings"][0]["id"] == id
            })
            .unwrap()
            .clone()
    };
    let bass = tones["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["rate"] == 48000
                && c["channels"] == 2
                && c["signal"] == "noise"
                && c["settings"].as_array().unwrap().len() == 1
                && c["settings"][0]["id"] == "concat.bass"
                && c["settings"][0]["params"]["gain"].as_f64() == Some(5.)
        })
        .unwrap()
        .clone();
    let bright = find(&tones, "concat.bright");
    let stadium = find(&stadium, "concat.stadium");
    let room = find(&rooms, "concat.room");
    let frames = 54017;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f32le");
    let mut file = std::fs::File::create(&path).unwrap();
    let source = |i: usize, ch: usize| -> f32 {
        if i >= frames {
            return 0.;
        }
        if [0, 48000 / 7, frames - 1].contains(&i) {
            return [1.25, -0.75][ch];
        }
        let h = (i % 257 + ch * 1061) as u32;
        let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
        (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
    };
    for i in 0..frames {
        for ch in 0..2 {
            file.write_all(&source(i, ch).to_le_bytes()).unwrap();
        }
    }
    drop(file);
    let mut values = 0;
    for (a, b) in [(&bass, &stadium), (&stadium, &bass), (&bright, &room)] {
        let graph = format!(
            "{},{}",
            a["chain"].as_str().unwrap(),
            b["chain"].as_str().unwrap()
        );
        let out = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f32le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-i",
                path.to_str().unwrap(),
                "-af",
                &graph,
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout.len() % 8, 0);
        let output_frames = out.stdout.len() / 8;
        let mut specs: Vec<EffectSpec> = serde_json::from_value(a["settings"].clone()).unwrap();
        specs.extend(serde_json::from_value::<Vec<EffectSpec>>(b["settings"].clone()).unwrap());
        let mut chain = Chain::new(48000, 2, &specs).unwrap();
        chain.set_input_frames(frames).unwrap();
        let mut data: Vec<Vec<f32>> = (0..2)
            .map(|ch| (0..output_frames).map(|i| source(i, ch)).collect())
            .collect();
        let mut at = 0;
        while at < output_frames {
            let count = 1024.min(output_frames - at);
            let mut block: Vec<&mut [f32]> =
                data.iter_mut().map(|v| &mut v[at..at + count]).collect();
            chain.process(&mut block, &mut || Ok(())).unwrap();
            at += count;
        }
        for (i, raw) in out.stdout.chunks_exact(4).enumerate() {
            let expected = f32::from_le_bytes(raw.try_into().unwrap());
            let actual = data[i % 2][i / 2];
            assert!(
                (actual - expected).abs() <= 2e-7,
                "{graph} value{i}: {actual} vs {expected}"
            );
            values += 1;
        }
    }
    println!("qualified {values} float PCM values across double reflections/lowpass precision transitions");
}
