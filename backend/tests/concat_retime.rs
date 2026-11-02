use video_kadr_backend::domain::concat_retime::TimelineCurve;
fn cases()->Vec<Vec<(f64,f64)>>{vec![
    vec![(0.,1.),(1.,3.)],vec![(0.25,1.),(0.75,3.)],
    vec![(0.5,4.),(0.,0.5),(1.,0.5)],vec![(2.,0.),(-1.,100.)],
    vec![(0.4,2.)],vec![(0.5,1.),(0.5,4.),(1.,2.)],
    vec![(f64::NAN,1.),(0.3,1.),(1.,f64::NAN)],
]}
#[test]
fn timeline_ramp_integrates_speed_and_refuses_invalid_sample_windows(){
    let curve=TimelineCurve::new(&[(0.,1.),(1.,3.)]).unwrap();
    assert_eq!(curve.mean(),2.);
    assert_eq!(curve.source_offset_ticks(254_016_000_000,508_032_000_000).unwrap(),381_024_000_000);
    assert_eq!(curve.source_offset_ticks(0,508_032_000_000).unwrap(),0);
    for(local,duration)in [(-1,1),(0,0),(1,1),(2,1)]{assert!(curve.source_offset_ticks(local,duration).is_err());}
    assert!(TimelineCurve::new(&[]).is_none());
    assert!(TimelineCurve::new(&[(f64::NAN,1.),(0.,f64::INFINITY)]).is_none());
    assert!(curve.audio_pieces(0.,0.,0.,0.,0.).is_err());
    assert!(curve.audio_pieces(0.,1.,-1.,0.,0.).is_err());
    assert!(curve.audio_pieces(f64::MAX,f64::MAX,0.,0.,0.).is_err());
    assert_eq!(curve.audio_pieces(0.,0.025,0.,0.,0.).unwrap().len(),8);
    assert_eq!(curve.audio_pieces(0.,100.,0.,0.,0.).unwrap().len(),400);
}
#[test]
#[ignore="requires unchanged original Concat speed curve and audio_pieces oracle"]
fn original_curve_clock_and_audio_piece_records_match_exactly(){
    let path=std::env::var("CONCAT_RETIME_ORACLE").expect("run scripts/qualify-concat-retime.py first");
    let records=std::fs::read_to_string(path).unwrap();let curves=cases().iter().map(|points|TimelineCurve::new(points).unwrap()).collect::<Vec<_>>();
    let mut counts=[0usize;5];
    for line in records.lines(){
        let fields=line.split_whitespace().collect::<Vec<_>>();let tag=fields[0].parse::<usize>().unwrap();let index=fields[1].parse::<usize>().unwrap();if tag>=5{continue;}let curve=&curves[index];counts[tag]+=1;
        let f=|i:usize|fields[i].parse::<f64>().unwrap();
        let integer=|i:usize|fields[i].parse::<i128>().unwrap();
        let equal=|actual:f64,expected:f64|assert_eq!(actual.to_bits(),expected.to_bits(),"{line}: {actual}/{expected}");
        match tag {
            0=>{let point=curve.points()[fields[2].parse::<usize>().unwrap()];equal(point.0,f(3));equal(point.1,f(4));},
            1=>assert_eq!(curve.source_offset_ticks(integer(5),integer(6)).unwrap(),integer(7),"{line}"),
            2=>{let duration=f(2);let pieces=curve.audio_pieces(0.021,duration,1.019,0.35,0.27).unwrap();let piece=&pieces[fields[3].parse::<usize>().unwrap()];equal(piece.start_seconds,f(4));equal(piece.duration_seconds,f(5));equal(piece.source_start_seconds,f(6));equal(piece.speed,f(7));equal(piece.fade_in_seconds,f(8));equal(piece.fade_out_seconds,f(9));},
            3|4=>{
                let placement=retained_placement(index,fields[2].parse().unwrap(),fields[3].parse().unwrap());
                assert_eq!(placement.timeline_duration_ticks().unwrap(),2_019_000);
                if tag==3 {
                    assert_eq!(video_kadr_backend::domain::video_clock::transition_source_sample_tick(&placement,video_kadr_backend::domain::composition::PlaybackMode::Forward,integer(5)).unwrap(),Some(integer(6)as i64),"{line}");
                }else{
                    assert_eq!(video_kadr_backend::domain::video_clock::source_tick(&placement,video_kadr_backend::domain::composition::PlaybackMode::Forward,integer(5)as u64).unwrap(),Some(integer(6)as u64),"{line}");
                }
            },
            _=>panic!("unknown record {line}"),
        }
    }
    assert_eq!(counts,[21,1113,3059,1113,1099],"all original records must be consumed");
}

fn retained_placement(index:usize,num:u32,den:u32)->video_kadr_backend::domain::composition::ClipPlacement{
    let points=cases()[index].iter().copied().filter(|(x,v)|x.is_finite()&&v.is_finite()).collect::<Vec<_>>();
    let mean=TimelineCurve::new(&points).unwrap().mean();
    serde_json::from_value(serde_json::json!({"timelineStartTick":650650,"sourceInTick":650650,"sourceOutTick":650650+(2.019*mean*1e6).round()as u64,"speed":1,"concatRetime":{"points":points,"durationTicks":2019000,"rateNum":num,"rateDen":den}})).unwrap()
}
#[test]
fn retained_source_clock_roundtrips_and_rejects_mixed_or_modified_clocks(){
    use video_kadr_backend::domain::{composition::{ClipPlacement,PlaybackMode},video_clock};
    for index in 0..7{
        let placement=retained_placement(index,30000,1001);
        let wire=serde_json::to_value(&placement).unwrap();
        let restored:ClipPlacement=serde_json::from_value(wire.clone()).unwrap();assert_eq!(restored,placement);
        assert_eq!(wire["concatRetime"]["points"],serde_json::to_value(&placement.concat_retime.as_ref().unwrap().points).unwrap());
        assert!(video_clock::transition_source_sample_tick(&placement,PlaybackMode::Reverse,0).is_err());
        for kind in ["duration","range","phase","head","ramp","empty","rate"]{
            let mut wrong=wire.clone();match kind{
                "duration"=>wrong["timelineDurationTick"]=serde_json::json!(2019001),
                "range"=>wrong["sourceOutTick"]=serde_json::json!(placement.source_out_tick+1),
                "phase"=>wrong["sourceTimelineOffsetTick"]=serde_json::json!(1),
                "head"=>wrong["sourceHeadTimelineTick"]=serde_json::json!(1),
                "ramp"=>wrong["speedRamp"]=serde_json::json!({"interpolation":"linear","points":[{"sourceProgressTick":0,"speed":1},{"sourceProgressTick":1000000,"speed":2}]}),
                "empty"=>wrong["concatRetime"]["points"]=serde_json::json!([]),
                "rate"=>wrong["concatRetime"]["rateNum"]=serde_json::json!(0),
                _=>unreachable!(),
            }
            let wrong:ClipPlacement=serde_json::from_value(wrong).unwrap();assert!(wrong.timeline_duration_ticks().is_err(),"{kind}");
        }
    }
    let plain:ClipPlacement=serde_json::from_value(serde_json::json!({"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1})).unwrap();
    assert!(!serde_json::to_value(plain).unwrap().as_object().unwrap().contains_key("concatRetime"));
}

fn gain_cases()->Vec<Vec<video_kadr_backend::domain::concat_effect_upgrade::ParamKey>>{
    use video_kadr_backend::domain::concat_effect_upgrade::ParamKey;
    let key=|at,value,ease|ParamKey{derived:false,at,value,ease};let linear=[0.,0.,1.,1.];
    vec![vec![],vec![key(0.,0.2,linear),key(1.,0.8,linear)],vec![key(0.,0.2,linear),key(1.,0.8,[0.42,0.,0.58,1.])],vec![key(1.,0.9,linear),key(0.4,1.2,linear),key(0.4,0.3,[0.,0.,0.58,1.]),key(0.,0.1,linear)],vec![key(0.25,0.2,linear),key(0.75,1.8,[0.42,-0.2,0.58,1.2])]]
}
fn curve_audio(index:usize,gain:usize,duration:f64)->video_kadr_backend::domain::composition::AudioClip{
    let points=cases()[index].iter().copied().filter(|(x,v)|x.is_finite()&&v.is_finite()).collect::<Vec<_>>();let curve=TimelineCurve::new(&points).unwrap();
    serde_json::from_value(serde_json::json!({"id":"curve-audio","sourceId":"source","placement":{"timelineStartTick":21000,"sourceInTick":1019000,"sourceOutTick":1019000+(duration*curve.mean()*1e6).round()as u64,"speed":1,"concatRetime":{"points":points,"durationTicks":(duration*1e6).round()as u64,"rateNum":25,"rateDen":1}},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"concatGainKeys":gain_cases()[gain],"fadeInTicks":350000,"fadeOutTicks":270000})).unwrap()
}
#[test]
#[ignore="requires unchanged original gain track_slice/audio_pieces oracle"]
fn original_gain_slices_and_expanded_native_audio_pieces_match(){
    use video_kadr_backend::domain::{concat_animation::PreparedTrack,concat_retime::expand_audio};
    let path=std::env::var("CONCAT_RETIME_ORACLE").unwrap();let text=std::fs::read_to_string(path).unwrap();
    let ranges=[(0.,0.1),(0.2,0.3),(0.25,0.75),(0.,1.),(0.5,0.5),(0.9,1.)];
    let tracks=gain_cases().iter().map(|keys|PreparedTrack::new(keys).unwrap()).collect::<Vec<_>>();
    let plans=(0..7).map(|curve|(0..5).map(|gain|expand_audio(&curve_audio(curve,gain,2.019)).unwrap()).collect::<Vec<_>>()).collect::<Vec<_>>();
    for (gain,track) in tracks.iter().enumerate(){for (slice,(from,to)) in ranges.iter().copied().enumerate(){let expected=text.lines().filter(|line|{let fields=line.split_whitespace().collect::<Vec<_>>();fields[0]=="6"&&fields[1].parse::<usize>().unwrap()==gain&&fields[2].parse::<usize>().unwrap()==slice}).count();assert_eq!(track.slice(from,to).unwrap().len(),expected);}}
    let mut counts=[0usize;3];
    for line in text.lines(){let f=line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>();let tag=f[0]as usize;if !(5..=7).contains(&tag){continue;}counts[tag-5]+=1;
        let equal=|actual:f64,expected:f64|assert_eq!(actual.to_bits(),expected.to_bits(),"{line}: {actual}/{expected}");
        match tag{
            5|6=>{let index=f[1]as usize;let(from,to)=ranges[f[2]as usize];let keys=tracks[index].slice(from,to).unwrap();if tag==5{equal(PreparedTrack::new(&keys).unwrap().value_at(f[3],1.).unwrap(),f[4]);}else{let key=&keys[f[3]as usize];equal(key.at,f[4]);equal(key.value,f[5]);for i in 0..4{equal(key.ease[i],f[6+i]);}}},
            7=>{let part=&plans[f[1]as usize][f[2]as usize][f[3]as usize];equal(PreparedTrack::new(&part.clip.concat_gain_keys).unwrap().value_at(f[4],1.).unwrap(),f[5]);},
            _=>unreachable!(),
        }
    }
    assert_eq!(counts,[150,51,2205]);
    for curve in 0..7{for duration in [0.025,0.8,2.019,100.]{let clip=curve_audio(curve,2,duration);let before=serde_json::to_value(&clip).unwrap();let pieces=expand_audio(&clip).unwrap();let expected=((duration/0.1).ceil()as usize).clamp(8,400);assert_eq!(pieces.len(),expected);let mut ids=std::collections::BTreeSet::new();for part in pieces{assert!(ids.insert(part.clip.id.clone()));assert!(part.clip.placement.concat_retime.is_none());assert!(part.clip.placement.speed_ramp.is_none());assert_eq!(part.clip.placement.timeline_start_tick,(part.source.start_seconds*1000.).round()as u64*1000);assert_eq!(part.clip.placement.timeline_duration_ticks().unwrap(),(part.source.duration_seconds*1e6).round()as u64);assert_eq!(part.clip.source_id,clip.source_id);}assert_eq!(serde_json::to_value(clip).unwrap(),before);}}
}

#[test]
fn complete_frame_feed_retains_source_rounding_and_filter_order(){
    use video_kadr_backend::domain::concat_retime::AudioPiece;
    let p=AudioPiece{start_seconds:0.021,duration_seconds:0.1,source_start_seconds:1.0190008,speed:0.123456789,from:0.,to:1.,fade_in_seconds:0.03,fade_out_seconds:0.04};
    let feed=p.frame_feed(Some(1.),48000,&["volume=0.25".into()]).unwrap();
    assert_eq!(feed.source_seek_microseconds,1019000);
    assert_eq!(feed.minimum_input_samples,((0.1*0.123456789+0.0190008)*48000_f64).round()as i64);
    assert_eq!(feed.filters,vec![format!("aselect=lt(st(0\\,ld(0)+samples_n)-samples_n\\,{})",feed.minimum_input_samples),"asetpts=N/SR/TB".into(),"atrim=start=0.019001".into(),"asetpts=PTS-STARTPTS".into(),"atempo=0.500000".into(),"atempo=0.500000".into(),"atempo=0.500000".into(),"atempo=0.987654".into(),"volume=0.25".into(),"afade=t=in:st=0:d=0.0300".into(),"afade=t=out:st=0.0600:d=0.0400".into(),"adelay=21:all=1".into()]);
    assert!(p.frame_feed(Some(f64::NAN),48000,&[]).is_err());assert!(p.frame_feed(None,0,&[]).is_err());assert!(p.frame_feed(None,48000,&["anull;movie=unsafe".into()]).is_err());
    let mut unit=p;unit.speed=1.;assert!(!unit.frame_feed(None,48000,&[]).unwrap().filters.iter().any(|filter|filter.starts_with("atempo=")));
}

#[test]
fn unprepared_source_piece_never_falls_back_to_native_trimmed_tempo(){
    use video_kadr_backend::domain::{concat_retime::expand_audio,nested_audio::NestedAudioLayer};
    let source=curve_audio(0,2,2.019);let piece=expand_audio(&source).unwrap().remove(0);
    let layer=NestedAudioLayer{concat_retime_owner:None,concat_retime_piece:Some(piece.source),gain_track:None,export_fade_clock:false,video_transitions:None,composition:"root".into(),instance_path:vec![],root_range_us:0..100000,clip_local_start_us:0,clip:piece.clip,crossfade:Default::default()};
    assert_eq!(layer.concat_retime_piece.unwrap().source_start_seconds.to_bits(),piece.source.source_start_seconds.to_bits());
    assert!(layer.sample(50000).is_err());
    assert!(video_kadr_backend::domain::nested_dsp::validate_export_processing(&[layer.clone()]).is_err());
    assert!(video_kadr_backend::domain::nested_pcm::validate_processing(&[layer]).is_err());
}

#[test]
#[ignore="requires original volume_expr/Track::resample oracle"]
fn original_frame_volume_expressions_match_exact_strings(){
    use video_kadr_backend::domain::concat_animation::PreparedTrack;
    let path=std::path::PathBuf::from(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).with_file_name("gain-expressions.tsv");
    let text=std::fs::read_to_string(path).unwrap();let ranges=[(0.,0.1),(0.2,0.3),(0.25,0.75),(0.,1.),(0.5,0.5),(0.9,1.)];
    let tracks=gain_cases().iter().map(|keys|PreparedTrack::new(keys).unwrap()).collect::<Vec<_>>();let mut count=0;
    for line in text.lines(){let f=line.splitn(6,'\t').collect::<Vec<_>>();assert_eq!(f[0],"GAIN_EXPR");let track=f[1].parse::<usize>().unwrap();let range=ranges[f[2].parse::<usize>().unwrap()];let keys=tracks[track].slice(range.0,range.1).unwrap();let filter=PreparedTrack::new(&keys).unwrap().audio_volume_filter(f[3].parse().unwrap(),f[4].parse().unwrap()).unwrap().unwrap_or_else(||"-".into());assert_eq!(filter,f[5],"{line}");count+=1;}
    assert_eq!(count,180);
}

#[test]
fn whole_curve_owner_preserves_ancestor_window_and_authored_clip() {
    use video_kadr_backend::domain::nested_audio::source_curve_owner_layer;
    let clip=curve_audio(0,2,2.019);
    let before=serde_json::to_value(&clip).unwrap();
    let path=vec!["outer-instance".to_owned(),"inner-instance".to_owned()];
    for (offset,window) in [(0,0..5_000_000),(400_000,450_000..4_500_000),(-200_000,0..3_000_000)] {
        let layer=source_curve_owner_layer("inner",offset,window.clone(),&path,&clip).unwrap();
        assert_eq!(layer.root_range_us,window);
        assert_eq!(layer.concat_retime_owner.unwrap().root_offset_us,offset);
        assert_eq!(layer.instance_path,path);
        assert_eq!(layer.composition,"inner");
        assert!(layer.concat_retime_piece.is_none());
        assert_eq!(serde_json::to_value(&layer.clip).unwrap(),before);
        assert!(layer.sample(window.start).is_err());
        assert!(video_kadr_backend::domain::nested_dsp::validate_export_processing(&[layer.clone()]).is_err());
        assert!(video_kadr_backend::domain::nested_pcm::validate_processing(&[layer]).is_err());
    }
    for (offset,window) in [(0,1..1),(0,2..1),(500_000,499_999..600_000)] {
        assert!(source_curve_owner_layer("inner",offset,window,&path,&clip).is_err());
    }
    let mut ordinary=clip.clone();ordinary.placement.concat_retime=None;
    assert!(source_curve_owner_layer("inner",0,0..100,&path,&ordinary).is_err());
    let mut edited=clip;edited.placement.source_out_tick+=1;
    assert!(source_curve_owner_layer("inner",0,0..100,&path,&edited).is_err());
}
