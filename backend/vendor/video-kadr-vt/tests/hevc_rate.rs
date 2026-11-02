#![cfg(target_os = "macos")]
use video_kadr_hevc_frame::Nv12;
use video_kadr_vt::{
    Encoder, EncoderCodec, Surface,
    rate::{Mode, RateControl},
};
#[test]
fn real_hardware_explicit_cbr_and_capped_vbr() {
    let frame = Nv12::from_rgba(128, 128, &[255; 128 * 128 * 4]).unwrap();
    let surface = Surface::from_nv12(&frame).unwrap();
    for mode in [Mode::Cbr, Mode::Vbr { max_bps: 2_000_000 }] {
        let control = RateControl {
            mode,
            target_bps: 1_000_000,
            rate_num: 30000,
            rate_den: 1001,
        };
        let mut encoder =
            Encoder::new_with_rate_control(EncoderCodec::Hevc, 128, 128, 8, control).unwrap();
        encoder.set_colour(1, 1, 1).unwrap();
        let mut bytes = 0;
        for frame in 0..30 {
            let packet = encoder
                .encode_with_keyframe(&surface, frame * 1001, 1001, 30000, frame == 0)
                .unwrap();
            assert!(!packet.data.is_empty());
            bytes += packet.data.len();
        }
        match mode {
            Mode::Cbr => assert!(bytes > 50_000, "CBR did not pad low-motion stream: {bytes}"),
            Mode::Vbr { .. } => assert!(bytes <= 250_000, "VBR exceeded one-second cap: {bytes}"),
        }
    }
}
#[test]
fn invalid_rate_control_is_rejected() {
    for control in [
        RateControl {
            mode: Mode::Cbr,
            target_bps: 0,
            rate_num: 30,
            rate_den: 1,
        },
        RateControl {
            mode: Mode::Cbr,
            target_bps: 1000,
            rate_num: 30,
            rate_den: 0,
        },
        RateControl {
            mode: Mode::Vbr { max_bps: 999 },
            target_bps: 1000,
            rate_num: 30,
            rate_den: 1,
        },
        RateControl {
            mode: Mode::Cbr,
            target_bps: 1000,
            rate_num: 1001,
            rate_den: 1,
        },
    ] {
        assert!(control.validate().is_err());
    }
}
