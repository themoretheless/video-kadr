#[path = "../src/domain/concat_audio_filter.rs"]
mod concat_audio_filter;
#[path = "../src/domain/concat_compressor.rs"]
mod concat_compressor;
#[path = "../src/domain/concat_deesser.rs"]
mod concat_deesser;
#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_limiter.rs"]
mod concat_limiter;
#[path = "../src/domain/concat_tone_filter.rs"]
mod concat_tone_filter;
#[path = "../src/domain/concat_enhance_voice.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg spectral denoiser"]
fn actual_enhance_voice_graph_matches_owned_hops() {
    let dir = tempfile::tempdir().unwrap();
    let mut compared = 0;
    let mut failures = 0usize;
    for rate in [8000, 44100, 48000, 192000] {
        for channels in [1, 2] {
            let frames = rate + 317;
            let source: Vec<f64> = (0..frames)
                .flat_map(|i| {
                    (0..channels).map(move |ch| {
                        if (i * 8 / rate) % 8 >= 4 {
                            return 0.;
                        }
                        let h = ((i + ch * 1061) as u32)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        let amplitude = [1e-5, 0.003, 1., 2.][(i * 8 / rate) % 4];
                        ((h >> 12) as i32 % 2049 - 1024) as f64 / 1024. * amplitude
                    })
                })
                .collect();
            let path = dir.path().join("input.f32le");
            let mut file = std::fs::File::create(&path).unwrap();
            for v in &source {
                file.write_all(&(*v as f32).to_le_bytes()).unwrap();
            }
            drop(file);
            for amount in [0., 40., 60., 100.] {
                let noise_floor = -25. - amount / 100. * 20.;
                let reduction = 6. + amount / 100. * 18.;
                let out = std::process::Command::new("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-f",
                        "f32le",
                        "-ar",
                        &rate.to_string(),
                        "-ac",
                        &channels.to_string(),
                        "-i",
                        path.to_str().unwrap(),
                        "-af",
                        &format!("highpass=f=80,afftdn=nf={noise_floor:.1}:nr={reduction:.1},deesser=i={:.2},acompressor=threshold={:.2}:ratio={:.1}:attack=15:release=120,alimiter=limit=0.95",amount/100.*0.5,0.5-amount/100.*0.3,2.+amount/100.*2.),
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
                assert_eq!(out.stdout.len(), source.len() * 4);
                let mut effect = native::EnhanceVoice::new(rate as u32, channels, amount as f32);
                for pass in 0..2 {
                    if pass == 1 {
                        effect.reset();
                    }
                    let mut actual = Vec::new();
                    for block in source.chunks(effect.hop() * channels) {
                        let planar: Vec<Vec<f64>> = (0..channels)
                            .map(|ch| {
                                block
                                    .chunks_exact(channels)
                                    .map(|f| f64::from(f[ch] as f32))
                                    .collect()
                            })
                            .collect();
                        let final_hop = actual.len() + block.len() == source.len();
                        let output = effect.process_hop(&planar, final_hop);
                        for i in 0..output[0].len() {
                            for ch in 0..channels {
                                actual.push(output[ch][i]);
                            }
                        }
                    }
                    let mut max = 0f64;
                    for (i, (actual, raw)) in
                        actual.iter().zip(out.stdout.chunks_exact(4)).enumerate()
                    {
                        let expected = f64::from(f32::from_le_bytes(raw.try_into().unwrap()));
                        max = max.max((actual - expected).abs());
                        if !actual.is_finite() || (actual - expected).abs() > 2e-7 {
                            failures += 1;
                            if failures < 4 {
                                println!("mismatch rate{rate} ch{channels} floor{noise_floor} reduction{reduction} pass{pass} value{i}: {actual} vs {expected}");
                            }
                        }
                    }
                    if pass == 0 {
                        compared += source.len();
                        println!(
                            "rate{rate} ch{channels} floor{noise_floor} reduction{reduction}: max{max}"
                        );
                    }
                }
            }
        }
    }
    println!("compared {compared} float PCM values; {failures} violations including reset replay");
    assert_eq!(
        failures, 0,
        "float spectral parity exceeds unchanged 2e-7 tolerance"
    );
}

#[test]
#[ignore = "requires actual FFmpeg dynamics graph"]
fn isolate_mono_limiter_after_reference_dynamics() {
    let dir = tempfile::tempdir().unwrap();
    let rate = 44100usize;
    let frames = rate + 317;
    let source: Vec<f32> = (0..frames)
        .map(|i| {
            if (i * 8 / rate) % 8 >= 4 {
                return 0.;
            }
            let h = (i as u32).wrapping_mul(1664525).wrapping_add(1013904223);
            ((h >> 12) as i32 % 2049 - 1024) as f32 / 1024.
                * [1e-5, 0.003, 1., 2.][(i * 8 / rate) % 4]
        })
        .collect();
    let path = dir.path().join("input.f32le");
    let mut f = std::fs::File::create(&path).unwrap();
    for v in &source {
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    drop(f);
    let prefix="highpass=f=80,afftdn=nf=-37:nr=16.8,deesser=i=0.30,acompressor=threshold=0.32:ratio=3.2:attack=15:release=120";
    let run = |chain: &str| {
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f32le",
                "-ar",
                "44100",
                "-ac",
                "1",
                "-i",
                path.to_str().unwrap(),
                "-af",
                chain,
                "-c:a",
                "pcm_f64le",
                "-f",
                "f64le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        output
            .stdout
            .chunks_exact(8)
            .map(|raw| f64::from_le_bytes(raw.try_into().unwrap()))
            .collect::<Vec<_>>()
    };
    let upstream = run(prefix);
    let reference = run(&format!("{prefix},alimiter=limit=0.95"));
    let mut hp = concat_tone_filter::Shelf::highpass(44100, 1, 80.).unwrap();
    let mut denoise = concat_denoise::Denoiser::new(44100, 1, -37., 16.8);
    let mut deesser = concat_deesser::Deesser::new(44100, 1, 0.30, 0.5, 0.5);
    let mut compressor = concat_compressor::Compressor::new(44100, 0.32, 3.2);
    let filtered: Vec<f64> = source
        .iter()
        .enumerate()
        .map(|(i, &v)| hp.process(0, f64::from(v), true, i + 1 == frames && frames % 2 != 0))
        .collect();
    let mut reconstructed = Vec::new();
    for block in filtered.chunks(denoise.hop()) {
        let output = denoise.process_hop(&[block.to_vec()]);
        for &v in &output[0] {
            let mut frame = [v];
            deesser.process_frame(&mut frame);
            compressor.process_frame(&mut frame);
            reconstructed.push(frame[0]);
        }
    }
    let mut upstream_max = 0f64;
    let mut upstream_bad = 0;
    for (i, (&actual, &expected)) in reconstructed.iter().zip(&upstream).enumerate() {
        let error = (actual - expected).abs();
        upstream_max = upstream_max.max(error);
        if error > 2e-12 {
            if upstream_bad < 3 {
                println!("upstream value{i}: {actual} vs {expected}, error{error}");
            }
            upstream_bad += 1;
        }
    }
    println!("upstream max{upstream_max}, errors above 2e-12: {upstream_bad}");
    let reconstructed_path = dir.path().join("reconstructed.f64le");
    let mut f = std::fs::File::create(&reconstructed_path).unwrap();
    for value in &reconstructed {
        f.write_all(&value.to_le_bytes()).unwrap();
    }
    drop(f);
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "f64le",
            "-ar",
            "44100",
            "-ac",
            "1",
            "-i",
            reconstructed_path.to_str().unwrap(),
            "-af",
            "alimiter=limit=0.95",
            "-c:a",
            "pcm_f64le",
            "-f",
            "f64le",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut own_limiter = concat_limiter::LookaheadLimiter::new(44100, 1, 0.95, true);
    let mut reconstructed_max = 0f64;
    let mut reconstructed_bad = 0;
    for (i, (&input, raw)) in reconstructed
        .iter()
        .zip(output.stdout.chunks_exact(8))
        .enumerate()
    {
        let expected = f64::from_le_bytes(raw.try_into().unwrap());
        let mut frame = [input];
        own_limiter.process_frame(&mut frame);
        let error = (frame[0] - expected).abs();
        reconstructed_max = reconstructed_max.max(error);
        if error > 2e-12 {
            if reconstructed_bad < 3 {
                println!(
                    "native-upstream limiter value{i}: {} vs {expected}, error{error}",
                    frame[0]
                );
            }
            reconstructed_bad += 1;
        }
    }
    println!("native-upstream limiter max{reconstructed_max}, failures{reconstructed_bad}");
    assert_eq!(
        reconstructed_bad, 0,
        "native limiter on the same reconstructed input must match original"
    );
    let mut limiter = concat_limiter::LookaheadLimiter::new(44100, 1, 0.95, true);
    let mut max = 0f64;
    let mut failures = 0;
    for (i, (&input, &expected)) in upstream.iter().zip(&reference).enumerate() {
        let mut frame = [input];
        limiter.process_frame(&mut frame);
        let error = (frame[0] - expected).abs();
        max = max.max(error);
        if error > 2e-12 {
            if failures < 3 {
                println!("limiter value{i}: {} vs {expected}, error{error}", frame[0]);
            }
            failures += 1;
        }
    }
    println!("reference-fed limiter max{max}, failures{failures}");
    assert_eq!(failures, 0);
}
