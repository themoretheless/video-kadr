//! Real-file qualification of independent transition preparation, before delivery admission.
use crate::{domain::composition::{Composition, TransformSpec}, process_control::ProcessRuntime};
use std::{collections::BTreeMap, process::Command, time::Duration};

#[tokio::test]
async fn independent_transition_real_files_selected_range() {
    let dir = tempfile::tempdir().unwrap();
    let mut inputs = BTreeMap::new();
    for (id, color, frequency) in [("a", "red", "440"), ("b", "blue", "660")] {
        let path = dir.path().join(format!("{id}.mkv"));
        let result = Command::new("ffmpeg").args(["-nostdin", "-v", "error", "-f", "lavfi", "-i",
            &format!("color=c={color}:s=4x4:r=8:d=4"), "-f", "lavfi", "-i",
            &format!("sine=frequency={frequency}:sample_rate=48000:duration=4"),
            "-c:v", "ffv1", "-pix_fmt", "bgra", "-c:a", "pcm_f32le", "-ac", "2", "-shortest"])
            .arg(&path).output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        inputs.insert(crate::domain::composition::SourceId::parse(id).unwrap(), path);
    }
    let mut c = Composition::new(Default::default());
    c.canvas.width = 4; c.canvas.height = 4; c.canvas.fps_milli = 8000;
    c.sources = serde_json::from_value(serde_json::json!({
        "a":{"id":"a","kind":"video","durationTicks":4000000,"width":4,"height":4,"hasAudio":true,"audioChannels":2},
        "b":{"id":"b","kind":"video","durationTicks":4000000,"width":4,"height":4,"hasAudio":true,"audioChannels":2}
    })).unwrap();
    c.tracks = serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video",
        "transitions":[{"id":"fade","fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"},"kind":"dissolve"}],
        "clips":[
            {"id":"a","sourceId":"a","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1.0},"transform":TransformSpec::default(),"blendMode":"normal","opacity":{"mode":"constant","value":1.0}},
            {"id":"b","sourceId":"b","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1.0},"transform":TransformSpec::default(),"blendMode":"normal","opacity":{"mode":"constant","value":1.0}}
        ]}])).unwrap();
    c.validate().unwrap();
    crate::domain::transition_export::validate(&c).unwrap();
    assert!(super::composition_native_video::needs_native_video(&c));
    assert!(super::composition_native_audio::needs_native_audio(&c));
    assert!(crate::domain::transition_export::has_independent_windows(&c));
    let mut unsupported=c.clone();
    if let crate::domain::composition::CompositionTrack::Video{clips,..}=&mut unsupported.tracks[0]{
        clips[0].effects=serde_json::from_value(serde_json::json!([{"kind":"color_invert","parameters":{}}])).unwrap();
    }
    assert!(crate::domain::transition_export::validate(&unsupported).unwrap_err().to_string().contains("signed effect clocks"));
    let runtime = ProcessRuntime::local_default();
    let cancel = tokio_util::sync::CancellationToken::new();
    let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
    let movie = super::composition_native_video::prepare_native_video(&runtime, dir.path(), &c, &inputs,
        250000..1750000, Duration::from_secs(60), &cancel, &progress).await.unwrap();
    let pcm = super::composition_native_audio::prepare_native_audio(&runtime, dir.path(), &c, &inputs,
        250000..1750000, Duration::from_secs(60), &cancel, &progress).await.unwrap();
    let decoded = Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(movie.path())
        .args(["-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().unwrap();
    assert!(decoded.status.success(), "{}", String::from_utf8_lossy(&decoded.stderr));
    assert_eq!(decoded.stdout.len(), 12 * 4 * 4 * 4);
    // Check endpoints and monotone blend across a window extending beyond both clip spans.
    let pixels: Vec<_> = decoded.stdout.chunks_exact(64).map(|frame| [frame[0],frame[1],frame[2],frame[3]]).collect();
    assert!(pixels[0][0] > 240 && pixels[0][2] < 10, "{pixels:?}");
    assert!(pixels[11][2] > 240 && pixels[11][0] < 10, "{pixels:?}");
    for pair in pixels.windows(2) { assert!(pair[0][0] >= pair[1][0] && pair[0][2] <= pair[1][2], "{pixels:?}"); }
    let destination = dir.path().join("delivery.mov");
    let text = BTreeMap::new();
    let colors = BTreeMap::new();
    let stabilization = BTreeMap::new();
    let output = crate::ports::CompositionExportSpec {
        range: Some(crate::ports::CompositionExportRange { start_ticks: 250000, end_ticks: 1750000 }),
        profile: crate::ports::CompositionExportProfile::Mov { profile: crate::ports::CompositionProResProfile::Hq },
        video_quality: 23, video_bitrate_kbps: None, av1_encoder: None,
    };
    let request = || crate::ports::CompositionExportCompileRequest {
        composition: &c, inputs: &inputs, text_resources: &text,
        color_commands: &colors, vidstab_transforms: &stabilization, destination: &destination,
        parallel_jobs: 1, output: output.clone(),
    };
    assert!(super::composition_native_video::compile_with_native_video(request(), movie.path(), None).is_err());
    let command = super::composition_native_video::compile_with_native_video(request(), movie.path(), Some(pcm.path())).unwrap();
    let result = Command::new("ffmpeg").args(&command.arguments).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let delivered_video = Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&destination)
        .args(["-map","0:v:0","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().unwrap();
    assert!(delivered_video.status.success());
    assert_eq!(delivered_video.stdout.len(), decoded.stdout.len());
    let delivered_audio = Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&destination)
        .args(["-map","0:a:0","-f","f32le","-ac","2","-ar","48000","pipe:1"]).output().unwrap();
    assert!(delivered_audio.status.success());
    assert_eq!(delivered_audio.stdout.len(), 72000 * 2 * 4);
    assert_eq!(pcm.frames, 72000);
    let bytes = std::fs::read(pcm.path()).unwrap();
    assert_eq!(bytes.len(), 72000 * 2 * 4);
    let actual: Vec<f32> = bytes.chunks_exact(4).map(|v| f32::from_le_bytes(v.try_into().unwrap())).collect();
    for (index, sample) in delivered_audio.stdout.chunks_exact(4).enumerate() {
        let delivered = f32::from_le_bytes(sample.try_into().unwrap());
        assert!((delivered-actual[index]).abs() <= 1. / 32768. + 1e-7, "delivery quantization sample {index}");
    }
    let mut source_pcm = BTreeMap::new();
    for (id,path) in &inputs {
        let decoded = Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(path)
            .args(["-vn","-f","f32le","-ac","2","-ar","48000","pipe:1"]).output().unwrap();
        assert!(decoded.status.success());
        source_pcm.insert(id.as_str(), decoded.stdout.chunks_exact(4).map(|v| f32::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>());
    }
    for i in 0..72000 {
        let absolute = 12000 + i;
        let (id, source_sample) = if absolute < 48000 { ("a",48000+absolute) } else { ("b",absolute) };
        let gain = if (24000..72000).contains(&absolute) {
            let (a,b) = crate::domain::audio_transition::AudioTransitionCurve::ConstantPower.gains((absolute-24000) as f32/48000.).unwrap();
            if id == "a" { a } else { b }
        } else { 1. };
        for channel in 0..2 {
            let expected = source_pcm[id][source_sample*2+channel]*gain;
            assert!((actual[i*2+channel]-expected).abs()<0.0001, "sample={absolute} channel={channel}: {} != {expected}",actual[i*2+channel]);
        }
    }
}
