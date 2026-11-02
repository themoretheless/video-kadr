use super::*;
fn picture(side: u64, time: u64) -> GroupSurface {
    let q = time / 100000;
    let rgba = (0..16u64)
        .flat_map(|i| {
            let (x, y) = (i % 4, i / 4);
            [
                ((11 + x * 37 + y * 19 + q * 13 + side * 71) % 256) as u8,
                ((29 + x * 17 + y * 43 + q * 23 + side * 31) % 256) as u8,
                ((47 + x * 53 + y * 7 + q * 41 + side * 89) % 256) as u8,
                [64, 128, 192, 255][((x + y + q + side) % 4) as usize],
            ]
        })
        .collect();
    GroupSurface {
        width: 4,
        height: 4,
        rgba,
    }
}

#[test]
fn all_840_ordinary_placement_frames_match_original() {
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-clip-placement-scenes.json")).unwrap();
 assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
 let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),840);
 let mut failures=Vec::new();
 for (index,case) in cases.iter().enumerate(){
 let source=super::super::filmcraft_sequence_source::SourceTrackItem::read(&case["item"]).unwrap();
 let timing=super::super::filmcraft_clip_placement::ImportedVideoPlacement::from_source(&source).unwrap();
 let mut clip:super::super::composition::VideoClip=serde_json::from_value(serde_json::json!({"id":"clip","sourceId":"media","placement":timing.placement,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[],"sourceAudioEnabled":false})).unwrap();
 timing.apply_to(&mut clip);
 let mut composition=Composition::new(Default::default());
 composition.canvas.width=4;composition.canvas.height=4;composition.canvas.background.alpha=0.;
 composition.canvas.fps_milli=23976;composition.canvas.fps_rate=Some(super::super::composition::FrameRateRatio{numerator:24000,denominator:1001});
 composition.sources=serde_json::from_value(serde_json::json!({"media":{"id":"media","kind":"video","durationTicks":1000000000,"width":4,"height":4,"hasAudio":false}})).unwrap();
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","clips":[clip],"transitions":[]}])).unwrap();
 composition=serde_json::from_value(serde_json::to_value(composition).unwrap()).unwrap();composition.validate().unwrap();
 let exact=case["at"].as_i64().unwrap() as i128;
 let mut decode=|_:&super::super::composition::SourceId,time:u64,_:bool|Ok(picture(0,time));
 let mut working=None;
 assert!(render_media_layers_context(&composition,(exact/254016)as u64,&mut||Ok(()),None,Some(&mut decode),&[],0,None,Some(&mut working),Some(exact)).unwrap().is_some());
 let bits:Vec<u32>=working.unwrap().pixels().iter().flatten().map(|v|v.to_bits()).collect();
 let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
 if bits!=expected{failures.push(index);}
 }
 assert!(failures.is_empty(),"placement frame mismatches: {failures:?}");
}
