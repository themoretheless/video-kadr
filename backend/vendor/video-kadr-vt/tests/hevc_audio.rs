#![cfg(target_os = "macos")]
use filmcraft_isobmff::{PcmConfig, SampleEntry, TrackConfig};
use std::io::Cursor;
use video_kadr_vt::movie::HevcMovie;
fn movie() -> HevcMovie<Cursor<Vec<u8>>> {
    HevcMovie::new(Cursor::new(Vec::new()), true, 64, 48, 25, 1, 1_000_000, 25).unwrap()
}
#[test]
fn pcm24_six_channels_preserves_exact_samples() {
    let mut movie = movie();
    let config = TrackConfig::new(
        SampleEntry::pcm(PcmConfig {
            bits: 24,
            float: false,
            big_endian: false,
            signed: true,
            channels: 6,
            sample_rate: 48000.,
        }),
        48000,
    );
    movie.audio_track(config, 1920).unwrap();
    movie.frame(&[255; 64 * 48 * 4]).unwrap();
    let raw: Vec<u8> = (0..1920 * 18).map(|i| (i % 251) as u8).collect();
    movie.audio_packet(&raw, 1920).unwrap();
    let data = movie.finish().unwrap().into_inner();
    let parsed = filmcraft_isobmff::open(&data).unwrap();
    assert_eq!(parsed.tracks.len(), 2);
    assert_eq!(parsed.tracks[1].duration, 1920);
    let mut recovered = Vec::new();
    for index in 0..parsed.tracks[1].samples.len() {
        recovered.extend(parsed.read_sample(&data, 1, index).unwrap());
    }
    assert_eq!(recovered, raw);
}
#[test]
fn aac_source_packets_and_priming_are_retained() {
    let mut encoder =
        filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(44100, 2, 128000)).unwrap();
    let priming = encoder.priming_samples();
    let mut config = TrackConfig::new(
        SampleEntry::aac(encoder.audio_specific_config(), 2, 44100),
        44100,
    );
    config.media_start = Some(i64::from(priming));
    let total = 1764 + u64::from(priming);
    let mut movie = movie();
    movie.audio_track(config, total).unwrap();
    movie.frame(&[255; 64 * 48 * 4]).unwrap();
    let signal: Vec<f32> = (0..1764).map(|i| (i as f32 * 0.07).sin() * 0.2).collect();
    let mut packets = encoder.encode(&[&signal, &signal]);
    packets.extend(encoder.flush());
    let mut written = 0;
    for packet in &packets {
        let duration = (total - written).min(1024) as u32;
        movie.audio_packet(packet, duration).unwrap();
        written += u64::from(duration);
    }
    assert_eq!(written, total);
    let data = movie.finish().unwrap().into_inner();
    let parsed = filmcraft_isobmff::open(&data).unwrap();
    assert_eq!(parsed.tracks[1].duration, total);
    assert_eq!(parsed.tracks[1].tkhd_duration, 1764);
    assert_eq!(parsed.tracks[1].edits[0].media_time, i64::from(priming));
    for (index, packet) in packets.iter().enumerate() {
        assert_eq!(parsed.read_sample(&data, 1, index).unwrap(), *packet);
    }
}
#[test]
fn short_or_misaligned_audio_cannot_finish() {
    let config = TrackConfig::new(
        SampleEntry::pcm(PcmConfig {
            bits: 16,
            float: false,
            big_endian: false,
            signed: true,
            channels: 2,
            sample_rate: 48000.,
        }),
        48000,
    );
    let mut movie = movie();
    movie.audio_track(config, 1920).unwrap();
    movie.frame(&[255; 64 * 48 * 4]).unwrap();
    assert!(movie.audio_packet(&[0; 3], 1).is_err());
    assert!(movie.finish().is_err());
}
