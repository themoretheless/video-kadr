#![cfg(target_os = "macos")]
use video_kadr_hevc_frame::Nv12;
use video_kadr_vt::{Encoder, EncoderCodec, Session, Surface};

#[test]
fn uploaded_cpu_frame_encodes_and_decodes_with_rational_timing() {
    let frame = Nv12::from_rgba(64, 48, &[255, 0, 0, 255].repeat(64 * 48)).unwrap();
    let surface = Surface::from_nv12(&frame).unwrap();
    assert_eq!(
        (
            surface.width(),
            surface.height(),
            surface.depth(),
            surface.full_range()
        ),
        (64, 48, 8, false)
    );
    let planes = surface.download().unwrap();
    assert_eq!(planes.y, frame.y());
    assert!(planes.cb.iter().all(|v| *v == 102));
    assert!(planes.cr.iter().all(|v| *v == 240));
    let mut encoder = Encoder::new_with_bitrate(EncoderCodec::Hevc, 64, 48, 8, 1_000_000).unwrap();
    encoder.set_colour(1, 1, 1).unwrap();
    let packet = encoder
        .encode_with_keyframe(&surface, 0, 1001, 30000, true)
        .unwrap();
    assert!(packet.key_frame);
    assert!(!packet.data.is_empty());
    assert!(!packet.decoder_configuration.is_empty());
    assert_eq!(packet.pts.value, 0);
    assert_eq!(
        i128::from(packet.duration.value) * 30000,
        i128::from(packet.duration.timescale) * 1001
    );
    assert_eq!(packet.parameter_sets.len(), 3);
    let sets: Vec<&[u8]> = packet.parameter_sets.iter().map(Vec::as_slice).collect();
    let mut decoder =
        Session::new_hevc(&sets[..1], &sets[1..2], &sets[2..], packet.nal_length_size).unwrap();
    let decoded = decoder.decode(&packet.data).unwrap().unwrap();
    assert_eq!((decoded.width, decoded.height), (64, 48));
    assert!(decoded.y.iter().all(|v| (i16::from(*v) - 63).abs() <= 4));
    for index in 1..3 {
        let packet = encoder.encode(&surface, index * 1001, 1001, 30000).unwrap();
        assert_eq!(
            i128::from(packet.pts.value) * 30000,
            i128::from(packet.pts.timescale) * i128::from(index) * 1001
        );
    }
}
