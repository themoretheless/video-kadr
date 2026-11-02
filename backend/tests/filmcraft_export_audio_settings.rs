//! Settings boundaries from pinned Filmcraft export/settings.rs::resolve.
#[path = "../src/domain/filmcraft_export_movie.rs"]
mod settings;

#[test]
fn source_channel_resolution_and_aac_ceiling_include_out_of_range_inputs() {
    for (requested, expected) in [(0,1),(1,1),(2,2),(3,2),(4,2),(5,2),(6,6),(7,6),(u32::MAX,6)] {
        let audio = settings::AudioSettings {
            channels: requested,
            sample_rate: Some(8000),
            bitrate_kbps: 512,
            ..Default::default()
        };
        assert_eq!(audio.channels(), expected, "requested {requested}");
        assert_eq!(audio.aac_bps(), (512_000u32).min(6 * 8000 * expected));
        let inherited = settings::AudioOverrides { channels: Some(requested), ..Default::default() }
            .apply(settings::AudioSettings::default());
        assert_eq!(inherited.channels(), expected);
    }
}


#[test]
fn atom_staging_counts_one_copied_channel_and_normalization_peak() {
    for (channels,bits,plain,normalized) in [(1,16,8,8),(2,16,14,16),(6,16,38,48),(1,24,10,10),(2,24,17,17),(6,24,45,48)] {
        assert_eq!(settings::audio_staging_frame_bytes(channels,bits/8,true,false),plain);
        assert_eq!(settings::audio_staging_frame_bytes(channels,bits/8,true,true),normalized);
    }
    assert_eq!(settings::audio_staging_frame_bytes(6,3,false,false),42);
    assert_eq!(settings::audio_staging_frame_bytes(6,3,false,true),48);
    assert_eq!(settings::audio_staging_frame_bytes(6,1,false,false),30);
    let limit=8u64*1024*1024*1024;
    let per_second=192000*settings::audio_staging_frame_bytes(6,3,true,false)+64*48*4;
    // A blanket duplicate of all six encoded channels would wrongly reject 800s.
    assert!(per_second*800<limit);
    assert!(per_second*1000>limit);
}
