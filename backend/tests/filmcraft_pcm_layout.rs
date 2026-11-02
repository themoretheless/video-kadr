mod support;
use serde_json::Value;
use video_kadr_backend::{
    domain::{
        filmcraft_pcm_export::{self, PcmFormat},
        filmcraft_project_audio::SourceProjectAudioRuntime,
        filmcraft_project_file::SourceProjectDocument,
        filmcraft_wav_audio::SourceWavPool,
    },
    tools::native_pcm_export::{encode_layout, PcmLayout, PcmPreset},
};
fn reference(
    samples: &[f32],
    frames: u32,
    rate: u32,
    channels: u16,
    bits: u16,
    aiff: bool,
) -> Vec<u8> {
    let mut payload = Vec::new();
    for sample in samples {
        let max = if bits == 24 { 8388607. } else { 32767. };
        let value = (sample.clamp(-1., 1.) * max).round() as i32;
        if aiff {
            payload.extend_from_slice(&value.to_be_bytes()[4 - bits as usize / 8..]);
        } else {
            payload.extend_from_slice(&value.to_le_bytes()[..bits as usize / 8]);
        }
    }
    let mut out = Vec::new();
    let size = payload.len() as u32;
    if aiff {
        out.extend_from_slice(b"FORM");
        out.extend_from_slice(&(46 + size + (size & 1)).to_be_bytes());
        out.extend_from_slice(b"AIFFCOMM");
        out.extend_from_slice(&18u32.to_be_bytes());
        out.extend_from_slice(&channels.to_be_bytes());
        out.extend_from_slice(&frames.to_be_bytes());
        out.extend_from_slice(&bits.to_be_bytes());
        let exp = (f64::from(rate).log2().floor()) as i32;
        out.extend_from_slice(&(16383 + exp as u16).to_be_bytes());
        let mant = (f64::from(rate) * 2f64.powi(63 - exp)) as u64;
        out.extend_from_slice(&mant.to_be_bytes());
        out.extend_from_slice(b"SSND");
        out.extend_from_slice(&(size + 8).to_be_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend(payload);
        if size & 1 == 1 {
            out.push(0);
        }
    } else {
        let ext = channels == 6;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(size + if ext { 60 } else { 36 }).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&(if ext { 40u32 } else { 16 }).to_le_bytes());
        out.extend_from_slice(&(if ext { 0xfffeu16 } else { 1 }).to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * u32::from(bits / 8)).to_le_bytes());
        out.extend_from_slice(&(channels * (bits / 8)).to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        if ext {
            out.extend_from_slice(&22u16.to_le_bytes());
            out.extend_from_slice(&bits.to_le_bytes());
            out.extend_from_slice(&0x3fu32.to_le_bytes());
            out.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
        }
        out.extend_from_slice(b"data");
        out.extend_from_slice(&size.to_le_bytes());
        out.extend(payload);
    }
    out
}

#[tokio::test]
async fn pcm_layout_files_match_original_wav_extensible_aiff_headers_payload_and_padding() {
    let (state, dir) = support::make_state(false, false).await;
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let token = tokio_util::sync::CancellationToken::new();
    for channels in [1u16, 2, 6] {
        for rate in [8000, 44117, 96000, 192000] {
            for bits in [16u16, 24] {
                for frames in [1u32, 17, 4097] {
                    for aiff in [false, true] {
                        let samples = (0..frames as usize * channels as usize)
                            .map(|i| [-1.2, -0.5, 0., 0.25, 1.2, 0.03125, -0.0001][i % 7])
                            .collect::<Vec<f32>>();
                        let bytes = samples
                            .iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect::<Vec<_>>();
                        let raw = dir.path().join("layout.f32le");
                        std::fs::write(&raw, &bytes).unwrap();
                        let output = dir.path().join(format!(
                            "outputs/layout-{channels}-{rate}-{bits}-{frames}-{aiff}.{}",
                            if aiff { "aiff" } else { "wav" }
                        ));
                        encode_layout(
                            &raw,
                            u64::from(frames),
                            if aiff {
                                PcmPreset::Aiff16
                            } else {
                                PcmPreset::Wav16
                            },
                            PcmLayout {
                                sample_rate: rate,
                                channels,
                                bits,
                            },
                            &output,
                            std::time::Duration::from_secs(30),
                            &token,
                            &tx,
                            state.cpu_pool.clone(),
                        )
                        .await
                        .unwrap();
                        let actual = std::fs::read(&output).unwrap();
                        assert!(
                            actual == reference(&samples, frames, rate, channels, bits, aiff),
                            "PCM bytes {channels}/{rate}/{bits}/{frames}/{aiff}"
                        );
                        assert_eq!(std::fs::read(&raw).unwrap(), bytes);
                        if frames == 17 {
                            let probe = tokio::process::Command::new("ffprobe")
                                .args(["-v", "error", "-show_streams", "-of", "json"])
                                .arg(&output)
                                .output()
                                .await
                                .unwrap();
                            assert!(probe.status.success());
                            let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
                            let stream = &probe["streams"][0];
                            assert_eq!(stream["channels"], channels);
                            assert_eq!(stream["sample_rate"], rate.to_string());
                            assert_eq!(stream["bits_per_sample"], bits);
                            assert_eq!(stream["duration_ts"], frames);
                            if channels == 6 && !aiff {
                                assert_eq!(stream["channel_layout"], "5.1");
                            }
                        }
                    }
                }
            }
        }
    }
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[test]
fn pcm_headers_check_layout_rate_and_riff_form_overflows_before_allocating_payload() {
    for format in [PcmFormat::Wav, PcmFormat::Aiff] {
        for (channels, rate, bits) in [
            (0, 48000, 16),
            (3, 48000, 16),
            (2, 0, 16),
            (2, 192001, 16),
            (2, 48000, 32),
        ] {
            assert!(filmcraft_pcm_export::header(1, rate, channels, bits, format).is_err());
        }
        assert!(filmcraft_pcm_export::header(u64::MAX, 48000, 6, 24, format).is_err());
        assert!(filmcraft_pcm_export::header(0, 48000, 2, 16, format).is_err());
    }
    let maximum = (u64::from(u32::MAX) - 60) / 18;
    assert!(filmcraft_pcm_export::header(maximum, 48000, 6, 24, PcmFormat::Wav).is_ok());
    assert!(filmcraft_pcm_export::header(maximum + 1, 48000, 6, 24, PcmFormat::Wav).is_err());
}

#[tokio::test]
async fn multichannel_pcm_cancel_and_invalid_byte_count_preserve_source_peer_and_existing_output() {
    let (state, dir) = support::make_state(false, false).await;
    let raw = dir.path().join("source.f32le");
    let file = std::fs::File::create(&raw).unwrap();
    file.set_len(1_000_000 * 24).unwrap();
    drop(file);
    let peer = dir.path().join("outputs/.native-pcm-peer");
    std::fs::write(&peer, b"peer").unwrap();
    let output = dir.path().join("outputs/cancel.wav");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let token = tokio_util::sync::CancellationToken::new();
    let stop = token.clone();
    let observer = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            if progress > 0. {
                stop.cancel();
                break;
            }
        }
    });
    let result = encode_layout(
        &raw,
        1_000_000,
        PcmPreset::Wav16,
        PcmLayout {
            sample_rate: 96000,
            channels: 6,
            bits: 24,
        },
        &output,
        std::time::Duration::from_secs(30),
        &token,
        &tx,
        state.cpu_pool.clone(),
    )
    .await;
    observer.await.unwrap();
    assert!(result.is_err());
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while state.cpu_pool.snapshot().running > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(!output.exists());
    assert_eq!(std::fs::metadata(&raw).unwrap().len(), 24_000_000);
    assert_eq!(std::fs::read(&peer).unwrap(), b"peer");
    let bytes = vec![0u8; 20];
    std::fs::write(&raw, &bytes).unwrap();
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let token = tokio_util::sync::CancellationToken::new();
    let result = encode_layout(
        &raw,
        1,
        PcmPreset::Aiff16,
        PcmLayout {
            sample_rate: 44100,
            channels: 6,
            bits: 24,
        },
        &output,
        std::time::Duration::from_secs(30),
        &token,
        &tx,
        state.cpu_pool.clone(),
    )
    .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("byte count mismatch"));
    assert!(!output.exists());
    assert_eq!(std::fs::read(&raw).unwrap(), bytes);
    std::fs::write(&raw, vec![0u8; 24]).unwrap();
    std::fs::write(&output, b"existing").unwrap();
    assert!(encode_layout(
        &raw,
        1,
        PcmPreset::Wav24,
        PcmLayout {
            sample_rate: 48000,
            channels: 6,
            bits: 24
        },
        &output,
        std::time::Duration::from_secs(30),
        &token,
        &tx,
        state.cpu_pool.clone()
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    assert!(std::fs::read_dir(dir.path().join("outputs"))
        .unwrap()
        .all(|e| {
            let name = e.unwrap().file_name();
            let name = name.to_string_lossy();
            !name.starts_with(".native-pcm-") || name == ".native-pcm-peer"
        }));
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[test]
fn original_768_project_pcm_byte_oracles_still_match() {
    let projects: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-wav-project.json")).unwrap();
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-pcm-export.json")).unwrap();
    assert_eq!(projects["commit"], oracle["commit"]);
    let wav = projects["wav"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    let mut compared = 0;
    for (ci, case) in projects["cases"].as_array().unwrap().iter().enumerate() {
        let mut sources = SourceWavPool::default();
        sources
            .bind(case["source"].as_u64().unwrap(), wav.clone().into())
            .unwrap();
        for channels in [1u16, 2] {
            for bits in [16u16, 24] {
                for (format, key) in [(PcmFormat::Wav, "wav"), (PcmFormat::Aiff, "aiff")] {
                    let document = SourceProjectDocument::read(
                        &serde_json::to_vec(&case["document"]).unwrap(),
                    )
                    .unwrap();
                    let mut runtime = SourceProjectAudioRuntime::new(document).unwrap();
                    for (ri, r) in case["requests"].as_array().unwrap().iter().enumerate() {
                        let actual = filmcraft_pcm_export::export_pcm(
                            &mut runtime,
                            &sources,
                            case["root"].as_u64().unwrap(),
                            r["start"].as_i64().unwrap(),
                            r["frames"].as_u64().unwrap() as usize,
                            channels,
                            bits,
                            format,
                        )
                        .unwrap();
                        let index = ((channels - 1) * 2 + u16::from(bits == 24)) as usize;
                        let expected = oracle["cases"][ci][ri][index][key]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_u64().unwrap() as u8)
                            .collect::<Vec<_>>();
                        assert!(
                            actual == expected,
                            "original PCM {ci}/{ri}/{channels}/{bits}/{key}"
                        );
                        compared += 1;
                    }
                }
            }
        }
    }
    assert_eq!(compared, 768);
}
