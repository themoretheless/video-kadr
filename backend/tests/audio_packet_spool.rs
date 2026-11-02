use video_kadr_backend::tools::audio_packet_spool::AudioPacketSpool;
#[test]
fn native_packet_transport_preserves_frames_and_float_bits(){
    let dir=tempfile::tempdir().unwrap();
    for rate in [8000,44100,48000,192000]{for channels in [1,2]{
        let mut spool=AudioPacketSpool::create(dir.path(),rate,channels,4*1024*1024).unwrap();
        let path=spool.path().to_owned();let sizes=[1,17,1967,4097,8191,2048,3];let mut expected=Vec::new();
        for count in sizes{
            let packet=(0..channels).map(|ch|(0..count).map(|i|if i==0{-0.0}else{((i+ch*17)%101)as f32/128.-0.5}).collect::<Vec<_>>()).collect::<Vec<_>>();
            for i in 0..count{for channel in &packet{expected.extend(channel[i].to_le_bytes());}}
            spool.push_f32(&packet).unwrap();
        }
        spool.seal().unwrap();assert_eq!(spool.bytes(),std::fs::metadata(&path).unwrap().len());
        let probe=std::process::Command::new("ffprobe").args(["-v","error","-show_frames","-show_entries","frame=nb_samples","-of","json"]).arg(&path).output().unwrap();
        assert!(probe.status.success(),"{}",String::from_utf8_lossy(&probe.stderr));
        let frames:serde_json::Value=serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(frames["frames"].as_array().unwrap().iter().map(|f|f["nb_samples"].as_u64().unwrap()as usize).collect::<Vec<_>>(),sizes);
        let decoded=std::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","pcm_f32le","-f","f32le","-"]).output().unwrap();
        assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));assert_eq!(decoded.stdout,expected);
        assert!(spool.push_f32(&[vec![0.]]).is_err());drop(spool);assert!(!path.exists());
    }}
}
#[test]
fn invalid_packets_and_budget_refuse_and_cleanup(){
    let dir=tempfile::tempdir().unwrap();assert!(AudioPacketSpool::create(dir.path(),48000,0,1024).is_err());assert!(AudioPacketSpool::create(dir.path(),48000,2,1).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
    let mut spool=AudioPacketSpool::create(dir.path(),48000,2,1024).unwrap();
    assert!(spool.seal().is_err());
    for packet in [vec![vec![0.]],vec![vec![],vec![]],vec![vec![f32::NAN],vec![0.]],vec![vec![0.;1000],vec![0.;1000]]]{assert!(spool.push_f32(&packet).is_err());}
    drop(spool);assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
}

// Exercise the production Rust kernel directly, with actual downstream FFmpeg
// negotiation and frame-evaluated gain. No recorded media fixture is stored.
#[path="../src/domain/concat_echo_kernel.rs"]
mod native_echo;
#[test]
fn rust_echo_tail_and_downstream_frame_gain_match_original_filter(){
    let dir=tempfile::tempdir().unwrap();
    for rate in [32000,48000]{for channels in [1,2]{
        let mut source=AudioPacketSpool::create(dir.path(),rate,channels,4*1024*1024).unwrap();
        let mut processed=AudioPacketSpool::create(dir.path(),rate,channels,4*1024*1024).unwrap();
        let mut echo=native_echo::Echo::<f32>::new(rate,channels,0.8,0.85,&[250.],&[0.4]).unwrap();
        for count in [17,1967,4097,8191,2048,3]{
            let packet=(0..channels).map(|ch|(0..count).map(|i|((i*17+ch*31)%199)as f32/256.-0.4).collect::<Vec<_>>()).collect::<Vec<_>>();
            source.push_f32(&packet).unwrap();
            let refs=packet.iter().map(Vec::as_slice).collect::<Vec<_>>();
            processed.push_f32(&echo.push_packet(&refs).unwrap()).unwrap();
        }
        while let Some(tail)=echo.finish_packet(){processed.push_f32(&tail).unwrap();}
        source.seal().unwrap();processed.seal().unwrap();
        let envelope="volume=volume='if(lt(t,0.1),0.2,0.8)':eval=frame,afade=t=in:st=0:d=0.0350,afade=t=out:st=0.4200:d=0.1700";
        let render=|path:&std::path::Path,filter:&str|{
            let output=std::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(path).args(["-af",filter,"-c:a","pcm_f32le","-f","f32le","-"]).output().unwrap();
            assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));output.stdout
        };
        let expected=render(source.path(),&format!("aecho=0.8:0.85:250:0.40,{envelope}"));
        let actual=render(processed.path(),envelope);
        assert_eq!(actual.len(),expected.len());
        let differing=actual.chunks_exact(4).zip(expected.chunks_exact(4)).filter(|(a,b)|a!=b).count();
        assert_eq!(differing,0,"Rust echo downstream gain mismatch: rate={rate}, channels={channels}");
    }}
}

#[test]
fn typed_packet_transport_preserves_integer_extremes_and_double_precision(){
    use video_kadr_backend::tools::audio_packet_spool::PcmPacketFormat;
    let dir=tempfile::tempdir().unwrap();
    macro_rules! qualify {($format:ident,$scalar:ty,$push:ident,$codec:literal,$raw:literal,$values:expr)=>{{
        for rate in [44100,48000]{for channels in [1,2]{
            let mut spool=AudioPacketSpool::create_format(dir.path(),rate,channels,4*1024*1024,PcmPacketFormat::$format).unwrap();
            let values:Vec<$scalar>=$values;let sizes=[1,17,4097,8191];let mut expected=Vec::new();
            for count in sizes{
                let packet=(0..channels).map(|ch|(0..count).map(|i|values[(i+ch)%values.len()]).collect::<Vec<_>>()).collect::<Vec<_>>();
                for i in 0..count{for channel in &packet{expected.extend(channel[i].to_le_bytes());}}
                spool.$push(&packet).unwrap();
            }
            spool.seal().unwrap();
            let probe=std::process::Command::new("ffprobe").args(["-v","error","-show_frames","-show_entries","frame=nb_samples","-of","json"]).arg(spool.path()).output().unwrap();assert!(probe.status.success());
            let frames:serde_json::Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(frames["frames"].as_array().unwrap().iter().map(|f|f["nb_samples"].as_u64().unwrap()as usize).collect::<Vec<_>>(),sizes);
            let output=std::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(spool.path()).args(["-map","0:a:0","-c:a",$codec,"-f",$raw,"-"]).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));assert_eq!(output.stdout,expected,"typed packet roundtrip {:?}",PcmPacketFormat::$format);
        }}
    }}}
    qualify!(S16,i16,push_i16,"pcm_s16le","s16le",vec![i16::MIN,i16::MAX,-1,0,1]);
    qualify!(S32,i32,push_i32,"pcm_s32le","s32le",vec![i32::MIN,i32::MAX,-1,0,1,16777217]);
    qualify!(F32,f32,push_f32,"pcm_f32le","f32le",vec![-0.0,0.0,0.25,-0.75,1.0000001]);
    qualify!(F64,f64,push_f64,"pcm_f64le","f64le",vec![-0.0,0.0,0.25,-0.75,1.0000000000000002]);
    let mut spool=AudioPacketSpool::create_format(dir.path(),48000,1,4096,PcmPacketFormat::F64).unwrap();
    assert!(spool.push_f32(&[vec![0.]]).is_err());assert!(spool.push_f64(&[vec![f64::NAN]]).is_err());
}

#[test]
fn packet_transport_retains_tempo_pts_instead_of_rebuilding_sample_clock(){
    let dir=tempfile::tempdir().unwrap();let mut spool=AudioPacketSpool::create(dir.path(),32000,1,4096).unwrap();
    let stamps=[0u64,19,41,60];
    for pts in stamps{spool.push_f32_at(&[vec![0.;17]],pts).unwrap();}spool.seal().unwrap();
    let probe=std::process::Command::new("ffprobe").args(["-v","error","-show_frames","-show_entries","frame=best_effort_timestamp,nb_samples:stream=time_base","-of","json"]).arg(spool.path()).output().unwrap();assert!(probe.status.success());
    let data:serde_json::Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(data["streams"][0]["time_base"],"1/1000000000");
    let actual=data["frames"].as_array().unwrap().iter().map(|f|{assert_eq!(f["nb_samples"],17);f["best_effort_timestamp"].as_u64().unwrap()*32000/1_000_000_000}).collect::<Vec<_>>();assert_eq!(actual,stamps);
}
