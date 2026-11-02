use serde_json::json;
use video_kadr_backend::domain::{
    composition::{Composition, CompositionSource, TransformSpec},
    concat_freeze_native, flat_surface,
    group_surface::GroupSurface,
    nested_audio,
    nested_composition::NestedCompositionProject,
};
fn setup() -> (Composition, CompositionSource) {
    let mut value = serde_json::to_value(Composition::new(Default::default())).unwrap();
    value["canvas"]["width"] = json!(64);
    value["canvas"]["height"] = json!(48);
    value["sources"] = json!({"source":{"id":"source","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":true}});
    value["tracks"] = json!([{"id":"v","name":"Video","kind":"video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":2000000,"sourceInTick":1000000,"sourceOutTick":7000000,"speed":1.5},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","sourceAudioEnabled":true,"audioFadeInTicks":300000,"audioFadeOutTicks":400000},{"id":"later","sourceId":"source","placement":{"timelineStartTick":8000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","sourceAudioEnabled":true}]}]);
    (serde_json::from_value(value).unwrap(),serde_json::from_value(json!({"id":"still","kind":"image","durationTicks":0,"width":1280,"height":1280,"hasAudio":false})).unwrap())
}
fn surface(size: usize) -> GroupSurface {
    GroupSurface {
        width: size,
        height: size,
        rgba: [255, 0, 0, 255].repeat(size * size),
    }
}
#[test]
fn ordinary_freeze_preserves_raster_footprint_across_the_jpeg_resolution_change() {
    let (c, still) = setup();
    let changed = concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
        .unwrap()
        .unwrap();
    let mut image = |_: &_| Ok(surface(1280));
    let mut video = |_: &_, _: u64, _: bool| Ok(surface(16));
    let before =
        flat_surface::render_media_layers(&c, 3125000, &mut || Ok(()), None, Some(&mut video))
            .unwrap()
            .unwrap();
    let frozen = flat_surface::render_media_layers(
        &changed,
        3500000,
        &mut || Ok(()),
        Some(&mut image),
        Some(&mut video),
    )
    .unwrap()
    .unwrap();
    assert_eq!(before, frozen);
    let rendered_tail = flat_surface::render_media_layers(
        &changed,
        4500000,
        &mut || Ok(()),
        Some(&mut image),
        Some(&mut video),
    )
    .unwrap()
    .unwrap();
    assert_eq!(before, rendered_tail);
    let pixels = before.rgba.chunks_exact(4).filter(|p| p[0] > 0).count();
    assert_eq!(pixels, 16 * 16);
    let value = serde_json::to_value(changed).unwrap();
    let head = &value["tracks"][1]["clips"][0];
    let tail = &value["tracks"][1]["clips"][1];
    assert_eq!(head["audioFadeInTicks"], 300000);
    assert!(head["audioFadeOutTicks"].is_null());
    assert!(tail["audioFadeInTicks"].is_null());
    assert_eq!(tail["audioFadeOutTicks"], 400000);
    assert_eq!(head["placement"]["sourceOutTick"], 2687500);
    assert_eq!(tail["placement"]["sourceInTick"], 2687500);
    assert_eq!(tail["placement"]["timelineStartTick"], 4125000);
    assert_eq!(
        value["tracks"][1]["clips"][2]["placement"]["timelineStartTick"],
        9000000
    );
}
#[test]
fn freeze_keeps_nonprimary_embedded_audio_inaudible_and_ripples_existing_stills() {
    let (c, still) = setup();
    let mut value = serde_json::to_value(c).unwrap();
    value["tracks"][0]["clips"][0]["audioFadeInTicks"] = json!(0);
    value["tracks"][0]["clips"][0]["audioFadeOutTicks"] = json!(0);
    let mut primary = value["tracks"][0]["clips"][1].clone();
    primary["id"] = json!("primary-clip");
    primary["sourceAudioEnabled"] = json!(false);
    value["tracks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"primary","name":"Primary","kind":"video","clips":[primary]}));
    let c: Composition = serde_json::from_value(value).unwrap();
    let original:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":c,"instances":[]}}})).unwrap();
    assert!(
        nested_audio::plan_audio_tracks(&original, "root", 0..9000000)
            .unwrap()
            .is_empty()
    );
    let first = concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
        .unwrap()
        .unwrap();
    let mut second = still;
    second.id = video_kadr_backend::domain::composition::SourceId::parse("still2").unwrap();
    let changed = concat_freeze_native::splice(
        &first,
        "clip",
        2.5,
        &second,
        "tail2",
        "hold2",
        "unused-images",
    )
    .unwrap()
    .unwrap();
    let project:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":changed,"instances":[]}}})).unwrap();
    assert!(
        nested_audio::plan_audio_tracks(&project, "root", 0..11000000)
            .unwrap()
            .is_empty()
    );
    let value = serde_json::to_value(&project.compositions["root"].content).unwrap();
    let hold = value["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|t| t["clips"].as_array().unwrap())
        .find(|clip| clip["id"] == "hold")
        .unwrap();
    assert_eq!(hold["timelineStartTick"], 4125000);
}

#[test]
fn native_tail_retains_the_original_media_version_binding() {
    let (c, still) = setup();
    let mut value = serde_json::to_value(c).unwrap();
    let mut original = value["sources"]["source"].clone();
    original["id"] = json!("original");
    value["sources"]["original"] = original;
    value["metadata"] = json!({"mediaVersions":{"clip":{"originalSourceId":"original","copySourceId":"source"}},"clipNames":{"clip":"Movie"}});
    let c: Composition = serde_json::from_value(value).unwrap();
    let before = serde_json::to_value(&c).unwrap();
    let changed = concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
        .unwrap()
        .unwrap();
    let changed = serde_json::to_value(changed).unwrap();
    assert_eq!(
        changed["metadata"]["mediaVersions"]["tail"],
        changed["metadata"]["mediaVersions"]["clip"]
    );
    assert!(changed["metadata"]["mediaVersions"].get("hold").is_none());
    assert_eq!(changed["metadata"]["clipNames"]["tail"], "Movie");
    assert_eq!(serde_json::to_value(c).unwrap(), before);
}

#[test]
fn native_primary_audio_keeps_its_head_tail_and_later_placements() {
    let (c, still) = setup();
    let mut value = serde_json::to_value(c).unwrap();
    value["tracks"][0]["clips"][0]["audioFadeInTicks"] = json!(0);
    value["tracks"][0]["clips"][0]["audioFadeOutTicks"] = json!(0);
    let c: Composition = serde_json::from_value(value).unwrap();
    let changed = concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
        .unwrap()
        .unwrap();
    assert!(flat_surface::requires_native_video(&changed));
    let project:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":changed,"instances":[]}}})).unwrap();
    let layers = nested_audio::plan_audio_tracks(&project, "root", 0..10000000).unwrap();
    assert_eq!(
        layers
            .iter()
            .map(|layer| layer.root_range_us.clone())
            .collect::<Vec<_>>(),
        vec![2000000..3125000, 4125000..7000000, 9000000..10000000]
    );
}

#[test]
fn still_preserves_projected_corners_for_rotation_noncentral_anchor_and_flip() {
    use video_kadr_backend::domain::{composition::AnimatableValue, group_transform::GroupAffine};
    for (scale, rotation) in [([1.5, 0.75], 30.), ([-1., 1.1], -60.)] {
        let (c, still) = setup();
        let mut value = serde_json::to_value(c).unwrap();
        let transform = &mut value["tracks"][0]["clips"][0]["transform"];
        for (name, v) in [
            ("scaleX", scale[0]),
            ("scaleY", scale[1]),
            ("rotationDegrees", rotation),
            ("x", 5.25),
            ("y", -3.75),
        ] {
            transform[name] = json!({"mode":"constant","value":v});
        }
        transform["anchorX"] = json!(0.2);
        transform["anchorY"] = json!(0.8);
        let c: Composition = serde_json::from_value(value).unwrap();
        let changed =
            concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
                .unwrap()
                .unwrap();
        let image = changed
            .tracks
            .iter()
            .find_map(|track| match track {
                video_kadr_backend::domain::composition::CompositionTrack::Image {
                    clips, ..
                } => clips.first(),
                _ => None,
            })
            .unwrap();
        let scalar = |value: &AnimatableValue| match value {
            AnimatableValue::Constant { value } => *value,
            _ => panic!("constant fixture"),
        };
        let old = GroupAffine::new(
            [64., 48.],
            [16., 16.],
            [0.2, 0.8],
            [5.25, -3.75],
            scale,
            rotation,
        )
        .unwrap();
        let new = GroupAffine::new(
            [64., 48.],
            [1280., 1280.],
            [image.transform.anchor_x, image.transform.anchor_y],
            [scalar(&image.transform.x), scalar(&image.transform.y)],
            [
                scalar(&image.transform.scale_x),
                scalar(&image.transform.scale_y),
            ],
            scalar(&image.transform.rotation_degrees),
        )
        .unwrap();
        for p in [[0., 0.], [0., 1.], [1., 0.], [1., 1.], [0.5, 0.5]] {
            let a = old.point([p[0] * 16., p[1] * 16.]);
            let b = new.point([p[0] * 1280., p[1] * 1280.]);
            assert!(
                (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
                "{a:?} != {b:?}"
            );
        }
    }
}

#[test]
fn unsupported_native_decoder_dimensions_do_not_produce_a_candidate() {
    let (c, still) = setup();
    let before = serde_json::to_value(&c).unwrap();
    let mut value = before.clone();
    value["sources"]["source"]["width"] = json!(8192);
    let wide: Composition = serde_json::from_value(value.clone()).unwrap();
    let error =
        concat_freeze_native::splice(&wide, "clip", 3.125, &still, "tail", "hold", "images")
            .unwrap_err();
    assert!(error.to_string().contains("source raster exceeds"));
    assert_eq!(serde_json::to_value(wide).unwrap(), value);
    let mut tall = still;
    tall.height = 3000;
    let error = concat_freeze_native::splice(&c, "clip", 3.125, &tall, "tail", "hold", "images")
        .unwrap_err();
    assert!(error.to_string().contains("JPEG exceeds"));
    assert_eq!(serde_json::to_value(c).unwrap(), before);
}

fn keyed(base: u64, a: f64, b: f64, c: f64) -> serde_json::Value {
    json!({"mode":"keyframes","track":{"timeBase":base,"interpolation":{"kind":"bezier","p1":[0.2,0.1],"p2":[0.8,1.2]},"extrapolation":{"before":"loop","after":"ping_pong"},"keyframes":[{"tick":base,"value":a,"interpolation":"ease_in"},{"tick":2*base,"value":b},{"tick":3*base,"value":c}]}})
}
fn sample(value: &video_kadr_backend::domain::composition::AnimatableValue, us: u64) -> f64 {
    use video_kadr_backend::domain::composition::AnimatableValue;
    match value {
        AnimatableValue::Constant { value } => *value,
        AnimatableValue::Keyframes { track } => track.sample_seconds(us as f64 / 1e6).unwrap(),
    }
}
#[test]
fn keyed_tail_preserves_motion_opacity_gain_pan_and_extrapolation_on_exact_grids() {
    use video_kadr_backend::domain::composition::{AnimatableValue, CompositionTrack};
    for base in [1_000_000, 1000, 24] {
        let (c, still) = setup();
        let mut value = serde_json::to_value(c).unwrap();
        let clip = &mut value["tracks"][0]["clips"][0];
        for (channel, a, b, c) in [
            ("x", -8., 8., 0.),
            ("y", -4., 4., 0.),
            ("scaleX", 1., 1.5, 0.75),
            ("scaleY", 1., 0.75, 1.25),
            ("rotationDegrees", 0., 45., -90.),
        ] {
            clip["transform"][channel] = keyed(base, a, b, c);
        }
        clip["opacity"] = keyed(base, 0.2, 0.8, 0.5);
        clip["audioGain"] = keyed(base, 0.5, 1., 0.75);
        clip["audioPan"] = keyed(base, -0.5, 0.5, 0.);
        let c: Composition = serde_json::from_value(value).unwrap();
        let before = serde_json::to_value(&c).unwrap();
        let changed =
            concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
                .unwrap()
                .unwrap();
        let CompositionTrack::Video {
            clips: original, ..
        } = &c.tracks[0]
        else {
            unreachable!()
        };
        let original = &original[0];
        let CompositionTrack::Video { clips, .. } = &changed.tracks[1] else {
            unreachable!()
        };
        let head = &clips[0];
        let tail = &clips[1];
        for (old, new) in [
            (&original.transform.x, &tail.transform.x),
            (&original.transform.y, &tail.transform.y),
            (&original.transform.scale_x, &tail.transform.scale_x),
            (&original.transform.scale_y, &tail.transform.scale_y),
            (
                &original.transform.rotation_degrees,
                &tail.transform.rotation_degrees,
            ),
            (&original.opacity, &tail.opacity),
            (&original.audio_gain, &tail.audio_gain),
            (&original.audio_pan, &tail.audio_pan),
        ] {
            for time in [0, 1, 62500, 500001, 1000000, 2000000, 2874999] {
                assert_eq!(
                    sample(old, time + 1125000).to_bits(),
                    sample(new, time).to_bits(),
                    "base {base} time {time}"
                );
            }
            let (AnimatableValue::Keyframes { track: a }, AnimatableValue::Keyframes { track: b }) =
                (old, new)
            else {
                unreachable!()
            };
            assert_eq!(a.extrapolation, b.extrapolation);
            assert_eq!(a.interpolation, b.interpolation);
            assert!(b.sample_clock.is_some() || b.keyframes[0].tick < 0);
        }
        assert_eq!(head.transform, original.transform);
        assert_eq!(head.opacity, original.opacity);
        let CompositionTrack::Image { clips, .. } = &changed.tracks[0] else {
            unreachable!()
        };
        let image = &clips[0];
        for (old, new, factor) in [
            (&original.transform.x, &image.transform.x, 1.),
            (
                &original.transform.scale_x,
                &image.transform.scale_x,
                16. / 1280.,
            ),
            (&original.opacity, &image.opacity, 1.),
        ] {
            for time in [0, 1, 62500, 250000, 500001, 875000, 999999] {
                let expected = sample(old, time * 4) * factor;
                assert!(
                    (sample(new, time) - expected).abs() < 1e-12,
                    "base {base} hold {time}"
                );
            }
            let AnimatableValue::Keyframes { track } = new else {
                unreachable!()
            };
            assert_eq!(track.time_base, base * 4);
        }
        assert_eq!(serde_json::to_value(c).unwrap(), before);
    }
}
#[test]
fn fractional_freeze_preserves_tail_hold_and_repeated_freeze_clocks_without_rounding() {
    use video_kadr_backend::domain::{composition::{CompositionTrack,AnimatableValue}, animation_sample_time::AnimationSampleTime};
    let (c, still) = setup();
    let mut value = serde_json::to_value(c).unwrap();
    let clip=&mut value["tracks"][0]["clips"][0];
    clip["placement"]["speed"]=json!(1.);
    clip["placement"]["sourceOutTick"]=json!(5123456);
    clip["transform"]["x"] = keyed(24, -8., 8., 0.);
    clip["opacity"] = keyed(24, 0.2, 0.8, 0.5);
    clip["audioGain"] = keyed(24, 0.5, 1., 0.75);
    clip["audioPan"] = keyed(24, -0.5, 0.5, 0.);
    let c: Composition = serde_json::from_value(value).unwrap();
    let before = serde_json::to_value(&c).unwrap();
    let changed = concat_freeze_native::splice(&c, "clip", 3.1, &still, "tail", "hold", "images").unwrap().unwrap();
    let changed:Composition=serde_json::from_slice(&serde_json::to_vec(&changed).unwrap()).unwrap();
    let CompositionTrack::Video{clips:original,..}=&c.tracks[0] else{unreachable!()};
    let CompositionTrack::Video{clips,..}=&changed.tracks[1] else{unreachable!()};
    let CompositionTrack::Image{clips:images,..}=&changed.tracks[0] else{unreachable!()};
    let exact=|value:&AnimatableValue,time:i64| match value{AnimatableValue::Constant{value}=>*value,AnimatableValue::Keyframes{track}=>AnimationSampleTime::SignedMicroseconds(time).sample_track(track).unwrap()};
    for(old,tail)in [(&original[0].transform.x,&clips[1].transform.x),(&original[0].opacity,&clips[1].opacity),(&original[0].audio_gain,&clips[1].audio_gain),(&original[0].audio_pan,&clips[1].audio_pan)]{
        let AnimatableValue::Keyframes{track}=tail else{unreachable!()};
        assert_eq!(track.sample_clock.unwrap().parts(),(1,1,1100000));
        for us in [-100000,0,1,10000,500001,2000000] {assert_eq!(exact(tail,us).to_bits(),exact(old,us+1100000).to_bits());}
    }
    for us in [0,1,10000,300000,500001,999999] {
        let AnimatableValue::Keyframes{track}=&images[0].transform.x else{unreachable!()};
        let expected=sample_replayed_curve(&original[0].transform.x,us,4123456);
        assert_eq!(exact(&images[0].transform.x,us).to_bits(),expected.to_bits());
        assert_eq!(track.time_base,24);
        assert_eq!(track.sample_clock.unwrap().parts(),(64429,15625,0));
    }
    let render=|composition:&Composition,time|{
        let mut image=|_:&_|Ok(surface(1280));
        let mut video=|_:&_,_:u64,_:bool|Ok(surface(16));
        flat_surface::render_media_layers(composition,time,&mut||Ok(()),Some(&mut image),Some(&mut video)).unwrap().unwrap()
    };
    assert_eq!(render(&c,2500000),render(&changed,2500000));
    assert_eq!(render(&c,3500001),render(&changed,4500001));
    // Native renderer samples the whole original curve across the inserted one-second still.
    assert_eq!(render(&c,3237037),render(&changed,3400000));
    let mut second_still=still.clone();
    second_still.id=video_kadr_backend::domain::composition::SourceId::parse("still2".to_owned()).unwrap();
    let twice=concat_freeze_native::splice(&changed,"tail",4.2,&second_still,"tail2","hold2","unused-image-lane").unwrap().unwrap();
    let CompositionTrack::Video{clips,..}=&twice.tracks[1] else{unreachable!()};
    let second=clips.iter().find(|clip|clip.id.as_str()=="tail2").unwrap();
    for us in [0,1,10000,500001] {assert_eq!(exact(&second.transform.x,us).to_bits(),exact(&original[0].transform.x,us+1200000).to_bits());}
    assert_eq!(serde_json::to_value(c).unwrap(),before);
}
fn sample_replayed_curve(value:&video_kadr_backend::domain::composition::AnimatableValue,us:i64,duration:u64)->f64 {
    use video_kadr_backend::domain::composition::AnimatableValue;
    let AnimatableValue::Keyframes{track}=value else{unreachable!()};
    let n=i128::from(us)*i128::from(duration)*i128::from(track.time_base);
    let denominator=1_000_000_000_000i128;
    let tick=n/denominator + i128::from((n%denominator)*2>=denominator);
    track.sample_tick(tick as i64)
}

#[test]
fn native_renderer_keeps_head_tail_motion_and_replays_the_whole_curve_during_hold() {
    let (c, still) = setup();
    let mut value = serde_json::to_value(c).unwrap();
    let animation = |a: f64, b: f64| json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":4000000,"value":b}]}});
    value["tracks"][0]["clips"][0]["transform"]["x"] = animation(0., 8.);
    value["tracks"][0]["clips"][0]["opacity"] = animation(0.2, 1.);
    let c: Composition = serde_json::from_value(value).unwrap();
    let changed = concat_freeze_native::splice(&c, "clip", 3.125, &still, "tail", "hold", "images")
        .unwrap()
        .unwrap();
    let render = |composition: &Composition, time| {
        let mut image = |_: &_| Ok(surface(1280));
        let mut video = |_: &_, _: u64, _: bool| Ok(surface(16));
        flat_surface::render_media_layers(
            composition,
            time,
            &mut || Ok(()),
            Some(&mut image),
            Some(&mut video),
        )
        .unwrap()
        .unwrap()
    };
    assert_eq!(render(&c, 2500000), render(&changed, 2500000));
    assert_eq!(render(&c, 3625000), render(&changed, 4625000));
    assert_eq!(render(&c, 4000000), render(&changed, 3625000));
}

#[test]
fn freeze_keeps_the_authored_pan_coefficient_grid_phase_in_actual_pcm() {
    use video_kadr_backend::domain::{composition::{CompositionTrack,AnimatableValue},nested_pcm};
    use std::sync::atomic::AtomicBool;
    let (c,still)=setup();
    let mut value=serde_json::to_value(c).unwrap();
    value["tracks"][0]["clips"][0]["audioPan"]=keyed(24,-0.5,0.5,0.);
    value["tracks"][0]["clips"][0]["placement"]["speed"]=json!(1.);
    value["tracks"][0]["clips"][0]["placement"]["sourceOutTick"]=json!(5000000);
    let original:Composition=serde_json::from_value(value).unwrap();
    let changed=concat_freeze_native::splice(&original,"clip",3.125,&still,"tail","hold","images").unwrap().unwrap();
    let mut legacy=changed.clone();
    let CompositionTrack::Video{clips,..}=&mut legacy.tracks[1] else{unreachable!()};
    let AnimatableValue::Keyframes{track}=&mut clips[1].audio_pan else{unreachable!()};
    track.sample_clock=None;
    for key in &mut track.keyframes{key.tick-=27;}
    let mix=|c:Composition,start:u64|{
        let project:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":c,"instances":[]}}})).unwrap();
        let duration=project.durations_us().unwrap()["root"];
        let layers=nested_audio::plan_audio_tracks(&project,"root",0..duration).unwrap();
        nested_pcm::mix_block(&layers,start,960,48000,&AtomicBool::new(false),|_,_|Ok((1,[1.,0.]))).unwrap()
    };
    let before=mix(original,150000); // 3.125 s, 1.125 s into the original clip
    let after=mix(changed,198000); // + one second of inserted silent still
    assert_eq!(before,after);
    assert_ne!(before,mix(legacy,198000),"resetting the 10 ms coefficient lattice must be detected");
}
