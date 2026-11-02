use std::io::Cursor;
use video_kadr_backend::domain::insta360_video_clock::inspect;
#[path = "support/insta360_mp4.rs"]
mod mp4;
#[test]
fn exact_rational_cfr_clock_uses_stts_instead_of_nominal_rate() {
    let clock = inspect(&mut Cursor::new(mp4::movie(
        30000,
        &[(300, 1001), (600, 1001)],
        None,
        Some(0),
    )))
    .unwrap()
    .unwrap();
    assert_eq!(
        (clock.numerator, clock.denominator, clock.sample_count),
        (30000, 1001, 900)
    );
    let integer = inspect(&mut Cursor::new(mp4::movie(
        90000,
        &[(500, 1800)],
        Some(&[(500, 3600)]),
        None,
    )))
    .unwrap()
    .unwrap();
    assert_eq!((integer.numerator, integer.denominator), (50, 1));
}
#[test]
fn variable_reordered_or_edited_video_clock_requires_explicit_pts() {
    for file in [
        mp4::movie(30000, &[(300, 1001), (300, 1002)], None, None),
        mp4::movie(90000, &[(500, 1800)], Some(&[(1, 3600), (499, 1800)]), None),
        mp4::movie(90000, &[(500, 1800)], None, Some(1800)),
        mp4::movie(90000, &[(500, 1800)], None, Some(-1)),
    ] {
        assert!(inspect(&mut Cursor::new(file)).is_err());
    }
}
#[test]
fn malformed_clock_atoms_are_bounded_and_non_mp4_is_absent() {
    let file = mp4::movie(90000, &[(500, 1800)], None, None);
    let mut bad = file.clone();
    bad[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(inspect(&mut Cursor::new(bad)).is_err());
    assert!(inspect(&mut Cursor::new(file[..file.len() - 2].to_vec())).is_err());
    assert!(inspect(&mut Cursor::new(vec![0; 128])).unwrap().is_none());
    assert!(inspect(&mut Cursor::new(mp4::movie(0, &[(1, 1)], None, None))).is_err());
}

#[test]
fn audio_priming_edits_do_not_change_video_clock_but_video_counts_must_match() {
    let mut movie = mp4::movie(90000, &[(500, 1800)], None, None);
    mp4::add_audio_edit(&mut movie, 1024);
    assert_eq!(
        inspect(&mut Cursor::new(movie)).unwrap().unwrap().numerator,
        50
    );
    let mut mismatch = mp4::movie(90000, &[(500, 1800)], None, None);
    mp4::add_video_track(&mut mismatch, mp4::movie(90000, &[(499, 1800)], None, None));
    assert!(inspect(&mut Cursor::new(mismatch)).is_err());
}
