mod support;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,path::{Path,PathBuf},time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{domain::{composition::{AudioClip,CompositionTrack,SourceId},concat_project_import::{import,ImportRequest},concat_retime::{expand_audio,SourceClock,TimelineCurve}},tools::{native_audio_export::{mix_project_audio_tracks,PcmSource},pcm_conform::probe_pcm_metadata}};
fn extracted_aac(path:&Path)->Vec<u8>{
    let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(path).args(["-map","0:a:0","-c:a","copy","-f","adts","pipe:1"]).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));output.stdout
}
#[tokio::test]
#[ignore="requires original pre-AAC PCM observation adapter and real FFmpeg"]
async fn expanded_curve_waveform_matches_actual_original_mixer_before_aac(){
    compare_waveform(false,0).await;
}
#[tokio::test]
#[ignore="requires original PCM adapter and real FFmpeg/FFprobe"]
async fn rust_complete_frame_feed_matches_actual_original_mixer_before_aac(){
    compare_waveform(true,0).await;
}
#[tokio::test]
#[ignore="requires original PCM adapter and source-key oracle"]
async fn frame_evaluated_keyed_gain_matches_original_bounded_waveform(){
    for gain in [2,3]{compare_waveform(true,gain).await;}
}
async fn compare_waveform(whole_frames:bool,gain_mode:usize){compare_waveform_input(whole_frames,gain_mode,"wav","pcm_f32le",48000,false).await;}
#[tokio::test]
#[ignore="requires original PCM adapter and compressed decode"]
async fn compressed_and_non_mix_rate_curves_match_original(){
    compare_waveform_input(true,0,"m4a","aac",44100,true).await;
    compare_waveform_input(true,0,"wav","pcm_s16le",32000,true).await;
}
async fn compare_waveform_input(whole_frames:bool,gain_mode:usize,extension:&str,codec:&str,rate:u32,mono:bool){compare_waveform_effects(whole_frames,gain_mode,extension,codec,rate,mono,0).await;}
#[tokio::test]
#[ignore="requires original mixer with filterChain observation"]
async fn native_echo_curves_match_original_before_gain_and_fades(){
    for gain in [0,2]{compare_waveform_effects(true,gain,"wav","pcm_f32le",48000,false,1).await;}
}
#[tokio::test]
#[ignore="requires original mixer with ordered filterChain observation"]
async fn native_echo_chain_curves_preserve_tail_order_and_parameter_rounding(){
    for gain in [0,2]{compare_waveform_effects(true,gain,"wav","pcm_f32le",48000,false,2).await;}
}
#[tokio::test]
#[ignore="requires original mixer and native typed source curve execution"]
async fn native_echo_curves_retain_negotiated_integer_and_double_precision(){
    for (extension,codec,rate,mono,gain) in [
        ("wav","pcm_u8",32000,true,0),
        ("wav","pcm_s16le",32000,true,0),
        ("wav","pcm_s32le",48000,false,0),
        ("wav","pcm_f64le",44100,true,0),
        ("wav","pcm_f64le",44100,true,2),
        ("m4a","aac",44100,true,0),
    ]{compare_waveform_effects(true,gain,extension,codec,rate,mono,1).await;}
}
#[tokio::test]
#[ignore="requires original mixer and S64 decode/tempo precision negotiation"]
async fn native_echo_curves_from_s64_match_original_double_negotiation(){
    for gain in [0,2]{compare_waveform_effects(true,gain,"nut","pcm_s64le",32000,true,1).await;}
}
#[tokio::test]
#[ignore="requires original mixer and original reflection fixture chains"]
async fn native_reflection_curves_match_default_and_extreme_source_taps(){
    for mode in 10..14{compare_waveform_effects(true,2,"wav","pcm_f32le",48000,false,mode).await;}
    for mode in (20..24).chain(30..34){compare_waveform_effects(true,0,"wav","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer and ordered source reflection chain"]
async fn native_reflection_curves_preserve_ordered_room_hall_tails(){
    for mode in [40,41]{compare_waveform_effects(true,0,"wav","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with Stadium echo/lowpass source chain"]
async fn native_stadium_curves_apply_lowpass_after_reflections(){
    for (extension,mode,codec,rate,mono,gain) in [
        ("wav",14,"pcm_f32le",48000,false,2),("caf",14,"pcm_f64le",44100,true,0),
        ("flac",14,"flac",48000,false,2),
        ("caf",24,"pcm_f64le",44100,true,0),("caf",34,"pcm_f64le",44100,true,0),
        ("wav",14,"pcm_s16le",32000,true,0),("flac",14,"flac",8000,true,0),
    ]{compare_waveform_effects(true,gain,extension,codec,rate,mono,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with ordered Stadium/Hall source chains"]
async fn native_stadium_curves_preserve_lowpass_and_clipping_chain_order(){
    for mode in [42,43]{compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with typed tone and limiter source chains"]
async fn native_tone_curves_preserve_biquads_limiter_and_gain_order(){
    for mode in 50..54{
        compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;
        compare_waveform_effects(true,2,"flac","flac",48000,false,mode).await;
    }
}
#[tokio::test]
#[ignore="requires original mixer with source tone minimum/maximum settings"]
async fn native_tone_curves_match_extreme_settings(){
    for mode in (60..64).chain(70..74){compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer and active lookahead limiting before source gain"]
async fn native_tone_curves_limit_hot_input_before_animated_gain(){
    for mode in [90,91]{compare_waveform_effects(true,2,"caf","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with unknown channel layout and low-rate tone bypass"]
async fn native_tone_curves_keep_unknown_layout_and_nyquist_behavior(){
    for mode in [50,53]{compare_waveform_effects(true,2,"wav","pcm_f32le",48000,false,mode).await;}
    compare_waveform_effects(true,0,"flac","flac",8000,true,51).await;
}
#[tokio::test]
#[ignore="requires original mixer with ordered tone/limiter/Echo source chains"]
async fn native_tone_curves_preserve_mixed_echo_limiter_order(){
    for mode in [80,81]{for gain in [0,2]{compare_waveform_effects(true,gain,"caf","pcm_f64le",44100,true,mode).await;}}
}
#[tokio::test]
#[ignore="requires original mixer with voice biquads and limiter source chains"]
async fn native_voice_curves_match_defaults_and_gain(){
    for mode in 100..104{
        compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;
        compare_waveform_effects(true,2,"flac","flac",48000,false,mode).await;
    }
}
#[tokio::test]
#[ignore="requires original mixer with minimum/maximum voice settings"]
async fn native_voice_curves_match_extreme_settings(){
    for mode in (110..114).chain(120..124){compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with voice channel-selection and Nyquist behavior"]
async fn native_voice_curves_preserve_layout_and_nyquist(){
    for mode in 100..104{compare_waveform_effects(true,2,"wav","pcm_f32le",48000,false,mode).await;}
    for mode in [102,103]{compare_waveform_effects(true,0,"flac","flac",8000,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with hot voice limiting and ordered Echo chains"]
async fn native_voice_curves_preserve_hot_input_and_effect_order(){
    for mode in 130..134{compare_waveform_effects(true,2,"caf","pcm_f64le",44100,true,mode).await;}
    for mode in [140,141]{for gain in [0,2]{compare_waveform_effects(true,gain,"caf","pcm_f64le",44100,true,mode).await;}}
}
#[tokio::test]
#[ignore="requires original mixer with source logarithmic crusher chains"]
async fn native_crusher_curves_match_defaults_and_gain(){
    for mode in [150,151]{
        compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;
        compare_waveform_effects(true,2,"flac","flac",48000,false,mode).await;
    }
}
#[tokio::test]
#[ignore="requires original mixer with source crusher boundary settings"]
async fn native_crusher_curves_match_extreme_and_fractional_settings(){
    for mode in [160,161,170,171,152,153]{compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with crusher channel-selection and Nyquist behavior"]
async fn native_crusher_curves_preserve_layout_and_nyquist(){
    for mode in [150,151]{
        compare_waveform_effects(true,2,"wav","pcm_f32le",48000,false,mode).await;
        compare_waveform_effects(true,0,"flac","flac",8000,true,mode).await;
    }
}
#[tokio::test]
#[ignore="requires original mixer with hot crusher limiting and ordered Echo chains"]
async fn native_crusher_curves_preserve_hot_input_and_effect_order(){
    for mode in [180,181]{compare_waveform_effects(true,2,"caf","pcm_f64le",44100,true,mode).await;}
    for mode in [190,191]{for gain in [0,2]{compare_waveform_effects(true,gain,"caf","pcm_f64le",44100,true,mode).await;}}
}
#[tokio::test]
#[ignore="requires original mixer with independently typed Robot stages"]
async fn native_robot_curves_accept_source_precision(){
    for (extension,codec,rate,mono,gain) in [
        ("caf","pcm_f64le",44100,true,0),("flac","flac",48000,false,2),
        ("wav","pcm_f32le",48000,false,2),("wav","pcm_s16le",32000,true,0),
        ("wav","pcm_u8",8000,true,0),
    ]{compare_waveform_effects(true,gain,extension,codec,rate,mono,200).await;}
}
#[tokio::test]
#[ignore="requires original mixer with Robot boundary and rounded modulation speeds"]
async fn native_robot_curves_match_settings(){
    for mode in [210,220,230]{compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,mode).await;}
}
#[tokio::test]
#[ignore="requires original mixer with low-rate Robot stages"]
async fn native_robot_curves_preserve_low_rate_layouts(){
    for (extension,codec) in [("flac","flac"),("caf","pcm_f64le")]{compare_waveform_effects(true,0,extension,codec,8000,true,200).await;}
}
#[tokio::test]
#[ignore="requires original mixer with hot Robot limiting and mixed Echo orders"]
async fn native_robot_curves_preserve_hot_input_and_effect_order(){
    compare_waveform_effects(true,2,"caf","pcm_f64le",44100,true,250).await;
    for mode in [240,241]{for gain in [0,2]{compare_waveform_effects(true,gain,"caf","pcm_f64le",44100,true,mode).await;}}
}
#[tokio::test]
#[ignore="requires actual original mixer and original sub-microsecond audio piece records"]
async fn raw_document_seconds_reach_original_pcm_without_premature_rounding(){
    compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,300).await;
    compare_waveform_effects(true,2,"flac","flac",48000,false,300).await;
    compare_waveform_effects(true,2,"caf","pcm_f64le",44100,true,301).await;
}
#[tokio::test]
#[ignore="requires actual original constant document clock mixer"]
async fn constant_document_seconds_preserve_pcm_and_whole_gain_track(){
    compare_waveform_effects(true,0,"caf","pcm_f64le",44100,true,310).await;
    compare_waveform_effects(true,4,"flac","flac",48000,false,310).await;
    compare_waveform_effects(true,4,"caf","pcm_f64le",44100,true,311).await;
}
#[tokio::test]
#[ignore="requires original single-input Echo/Stadium integer effect graphs"]
async fn constant_document_reflections_preserve_integer_precision(){
    for (extension,codec,rate,mono,mode) in [
        ("wav","pcm_s16le",32000,true,312),("wav","pcm_u8",8000,true,312),
        ("flac","flac",48000,false,312),("caf","pcm_s16le",44100,true,313),
        ("flac","flac",48000,false,313),("flac","flac",8000,true,313),
    ]{compare_waveform_effects(true,0,extension,codec,rate,mono,mode).await;}
}
fn source_effect_case(mode:usize)->(Vec<video_kadr_backend::domain::filmcraft_dsp::EffectSpec>,String){
    let make=|id:&str,params:Value|serde_json::from_value(json!({"id":id,"params":params})).unwrap();
    if mode==0{return (vec![],String::new());}
    if mode==312{return source_effect_case(1);}
    if mode==313{return source_effect_case(14);}
    if mode==310{return(vec![],String::new());}
    if mode==311{return source_effect_case(50);}
    if mode==300{return(vec![],String::new());}
    if mode==301{return source_effect_case(50);}
    if mode==250{return source_effect_case(200);}
    if mode==240||mode==241{let(a,b)=if mode==240{(200,1)}else{(1,200)};let(mut specs,first)=source_effect_case(a);let(second,last)=source_effect_case(b);specs.extend(second);return(specs,format!("{first},{last}"));}
    if matches!(mode,200|210|220|230){
        let(depth,speed)=match mode{200=>(5.,"1.20"),210=>(1.,"0.56"),220=>(10.,"2.00"),_=>(6.555,"1.45")};
        return(vec![make("concat.robot",json!({"depth":depth}))],format!("aphaser=type=t:speed={speed}:decay=0.6,flanger=delay=2:depth=4:speed=0.8,equalizer=f=1800:t=q:w=1.2:g=3,alimiter=limit=0.94"));
    }
    if mode==180||mode==181{return source_effect_case(mode-30);}
    if mode==190||mode==191{let(a,b)=if mode==190{(150,1)}else{(1,150)};let(mut specs,first)=source_effect_case(a);let(second,last)=source_effect_case(b);specs.extend(second);return(specs,format!("{first},{last}"));}
    if mode==152{return(vec![make("concat.radio",json!({"noise":12.5}))],"highpass=f=300,lowpass=f=3400,acrusher=bits=15:mode=log:mix=0.6,alimiter=limit=0.9".into());}
    if mode==153{return(vec![make("concat.lofi",json!({"bits":6.5}))],"lowpass=f=3200,acrusher=bits=7:mode=log:mix=0.8,alimiter=limit=0.9".into());}
    if matches!(mode,150..=151|160..=161|170..=171){
        let index=mode%10;let setting=(mode-150)/10;
        // Independent unchanged original package fixture graphs.
        let chains=[
            ["highpass=f=300,lowpass=f=3400,acrusher=bits=12:mode=log:mix=0.6,alimiter=limit=0.9","highpass=f=300,lowpass=f=3400,acrusher=bits=16:mode=log:mix=0.6,alimiter=limit=0.9","highpass=f=300,lowpass=f=3400,acrusher=bits=4:mode=log:mix=0.6,alimiter=limit=0.9"],
            ["lowpass=f=3200,acrusher=bits=8:mode=log:mix=0.8,alimiter=limit=0.9","lowpass=f=3200,acrusher=bits=4:mode=log:mix=0.8,alimiter=limit=0.9","lowpass=f=3200,acrusher=bits=12:mode=log:mix=0.8,alimiter=limit=0.9"],
        ];
        let(id,param,values)=[("concat.radio","noise",[30.,0.,100.]),("concat.lofi","bits",[8.,4.,12.])][index];
        let mut params=serde_json::Map::new();params.insert(param.into(),json!(values[setting]));return(vec![make(id,Value::Object(params))],chains[index][setting].into());
    }
    if (130..134).contains(&mode){return source_effect_case(mode-30);}
    if mode==140||mode==141{let(a,b)=if mode==140{(100,1)}else{(1,100)};let(mut specs,first)=source_effect_case(a);let(second,last)=source_effect_case(b);specs.extend(second);return(specs,format!("{first},{last}"));}
    if mode==90||mode==91{return source_effect_case(if mode==90{50}else{53});}
    if mode==80||mode==81{let(a,b)=if mode==80{(50,1)}else{(1,50)};let(mut specs,first)=source_effect_case(a);let(second,last)=source_effect_case(b);specs.extend(second);return(specs,format!("{first},{last}"));}
    if matches!(mode,50..=53|60..=63|70..=73){
        let index=mode%10;let setting=mode/10-5;
        let chains=[
            ["bass=g=5.00:f=110:w=0.6,alimiter=limit=0.95","bass=g=0.00:f=110:w=0.6,alimiter=limit=0.95","bass=g=12.00:f=110:w=0.6,alimiter=limit=0.95"],
            ["treble=g=4.00:f=9000:w=0.7,alimiter=limit=0.95","treble=g=0.00:f=9000:w=0.7,alimiter=limit=0.95","treble=g=12.00:f=9000:w=0.7,alimiter=limit=0.95"],
            ["treble=g=5.00:f=6000:w=0.8,alimiter=limit=0.95","treble=g=0.00:f=6000:w=0.8,alimiter=limit=0.95","treble=g=12.00:f=6000:w=0.8,alimiter=limit=0.95"],
            ["lowpass=f=600.0,equalizer=f=200:t=q:w=2.0:g=3.50,alimiter=limit=0.92","lowpass=f=760.0,equalizer=f=200:t=q:w=2.0:g=2.30,alimiter=limit=0.92","lowpass=f=400.0,equalizer=f=200:t=q:w=2.0:g=5.00,alimiter=limit=0.92"],
        ];
        let (id,param,default)=[("concat.bass","gain",5.),("concat.treble","gain",4.),("concat.bright","gain",5.),("concat.underwater","depth",5.)][index];
        let value=if setting==0{default}else if setting==1{if index==3{1.}else{0.}}else if index==3{10.}else{12.};
        let mut params=serde_json::Map::new();params.insert(param.into(),json!(value));return(vec![make(id,Value::Object(params))],chains[index][setting].into());
    }
    if matches!(mode,100..=103|110..=113|120..=123){
        let index=mode%10;let setting=(mode-100)/10;
        // Literal package fixture graphs, independent of the native recipe.
        let chains=[
            ["highpass=f=400,lowpass=f=3400,equalizer=f=1600:t=q:w=1.4:g=4.50,alimiter=limit=0.92","highpass=f=400,lowpass=f=3400,equalizer=f=1600:t=q:w=1.4:g=3.00,alimiter=limit=0.92","highpass=f=400,lowpass=f=3400,equalizer=f=1600:t=q:w=1.4:g=8.00,alimiter=limit=0.92"],
            ["highpass=f=550,lowpass=f=2800,equalizer=f=1600:t=q:w=2.0:g=7.00,alimiter=limit=0.88","highpass=f=550,lowpass=f=2800,equalizer=f=1600:t=q:w=2.0:g=3.80,alimiter=limit=0.88","highpass=f=550,lowpass=f=2800,equalizer=f=1600:t=q:w=2.0:g=11.00,alimiter=limit=0.88"],
            ["highpass=f=250,lowpass=f=5000,equalizer=f=1200:t=q:w=1.5:g=4.00,alimiter=limit=0.95","highpass=f=250,lowpass=f=5000,equalizer=f=1200:t=q:w=1.5:g=2.00,alimiter=limit=0.95","highpass=f=250,lowpass=f=5000,equalizer=f=1200:t=q:w=1.5:g=6.00,alimiter=limit=0.95"],
            ["highpass=f=500,equalizer=f=3200:t=q:w=1.4:g=5.00,equalizer=f=7500:t=q:w=1.2:g=3.50,alimiter=limit=0.92","highpass=f=500,equalizer=f=3200:t=q:w=1.4:g=2.60,equalizer=f=7500:t=q:w=1.2:g=1.50,alimiter=limit=0.92","highpass=f=500,equalizer=f=3200:t=q:w=1.4:g=8.00,equalizer=f=7500:t=q:w=1.2:g=6.00,alimiter=limit=0.92"],
        ];
        let(id,param,default,min)=[("concat.telephone","drive",3.,0.),("concat.megaphone","drive",5.,1.),("concat.vinyl","warmth",5.,0.),("concat.whisper","air",5.,1.)][index];
        let value=match setting{0=>default,1=>min,_=>10.};let mut params=serde_json::Map::new();params.insert(param.into(),json!(value));return(vec![make(id,Value::Object(params))],chains[index][setting].into());
    }
    if mode==1{return (vec![make("concat.echo",json!({"delay":0.25,"decay":0.4}))],"aecho=0.8:0.85:250:0.40".into());}
    if mode==2{return (vec![make("concat.echo",json!({"delay":0.25,"decay":0.4})),make("concat.echo",json!({"delay":0.12345,"decay":0.6667}))],"aecho=0.8:0.85:250:0.40,aecho=0.8:0.85:123:0.67".into());}
    if mode==42||mode==43{let(a,b)=if mode==42{(14,11)}else{(11,14)};let(mut specs,first)=source_effect_case(a);let(second,last)=source_effect_case(b);specs.extend(second);return(specs,format!("{first},{last}"));}
    if mode==40||mode==41{let(a,b)=if mode==40{(10,11)}else{(11,10)};let(mut specs,first)=source_effect_case(a);let(second,last)=source_effect_case(b);specs.extend(second);return(specs,format!("{first},{last}"));}
    // Independent strings from unchanged original package fixtures.toml files.
    let chains=[
        ["aecho=0.8:0.88:24|43|70|101:0.320|0.240|0.170|0.110","aecho=0.8:0.88:9|16|27|39:0.320|0.240|0.170|0.110","aecho=0.8:0.88:46|82|134|194:0.320|0.240|0.170|0.110"],
        ["aecho=0.8:0.85:75|138|225|338:0.5|0.4|0.3|0.2","aecho=0.8:0.85:30|55|90|135:0.5|0.4|0.3|0.2","aecho=0.8:0.85:120|220|360|540:0.5|0.4|0.3|0.2"],
        ["aecho=0.8:0.9:9|19|32|47:0.45|0.35|0.25|0.15","aecho=0.8:0.9:4|9|15|22:0.45|0.35|0.25|0.15","aecho=0.8:0.9:16|34|58|86:0.45|0.35|0.25|0.15"],
        ["aecho=0.8:0.9:159|305|517|795:0.6|0.5|0.4|0.3","aecho=0.8:0.9:60|115|195|300:0.6|0.5|0.4|0.3","aecho=0.8:0.9:240|460|780|1200:0.6|0.5|0.4|0.3"],
        ["aecho=0.8:0.88:228|456:0.45|0.28,lowpass=f=6500","aecho=0.8:0.88:138|276:0.45|0.28,lowpass=f=6500","aecho=0.8:0.88:300|600:0.45|0.28,lowpass=f=6500"],
    ];
    let index=mode%10;let setting=mode/10-1;assert!(index<5&&setting<3,"unknown source effect case {mode}");
    let (id,default)=[("concat.room",40.),("concat.hall",50.),("concat.plate",40.),("concat.cathedral",55.),("concat.stadium",6.)][index];
    let size=match setting{0=>default,1=>if index==4{1.}else{0.},_=>if index==4{10.}else{100.}};(vec![make(id,json!({"size":size}))],chains[index][setting].into())
}
fn equal_pcm(actual:&[u8],expected:&[u8],label:&str){
    assert_eq!(actual.len(),expected.len(),"{label}: PCM length");
    let differences=actual.chunks_exact(4).zip(expected.chunks_exact(4)).enumerate().filter(|(_, (a,b))|a!=b).map(|(i,(a,b))|(i,f32::from_le_bytes(a.try_into().unwrap()),f32::from_le_bytes(b.try_into().unwrap()))).take(8).collect::<Vec<_>>();
    assert!(differences.is_empty(),"{label}: first differing scalars {differences:?}");
}
async fn compare_waveform_effects(whole_frames:bool,gain_mode:usize,extension:&str,codec:&str,rate:u32,mono:bool,echo:usize){
    let reference=PathBuf::from(std::env::var("CONCAT_AUDIO_WAVEFORM_ORACLE").expect("build scripts/qualify-concat-audio-waveform.py"));
    let scalar=std::fs::read_to_string(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).unwrap();
    let(state,_dir)=support::make_state(true,false).await;
    let input=state.sources_dir().join(format!("curve-source.{extension}"));
    let hot=matches!(echo,40|41|42|43|80|81|90|91|130..=133|140|141|180|181|190|191|240|241|250);let (left,right)=if hot{(2.5,1.75)}else{(0.1,0.2)};
    let signal=if mono{format!("aevalsrc={left}*sin(2*PI*440*t):s={rate}:d=8")}else{format!("aevalsrc={left}*sin(2*PI*440*t)|{right}*sin(2*PI*660*t):s={rate}:d=8")};
    let made=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i",&signal,"-c:a",codec]).arg(&input).output().unwrap();assert!(made.status.success(),"{}",String::from_utf8_lossy(&made.stderr));
    let cancel=CancellationToken::new();let probe=probe_pcm_metadata(&state.process_runtime,Path::new("ffprobe"),&input,Duration::from_secs(30),&cancel).await.unwrap();
    let metadata=probe.streams.iter().find(|s|s.sample_rate.is_some()).unwrap().clone();
    if matches!(echo,14|24|34|42|43){assert_eq!(metadata.channel_layout.is_some(),matches!(extension,"caf"|"flac"),"Stadium corpus must distinguish specified/unspecified channel layouts");}
    let sources=BTreeMap::from([(SourceId::parse("source").unwrap(),PcmSource::from_probe(input.clone(),probe).unwrap())]);
    for (case,points) in [(0,vec![(0.,1.),(1.,3.)]),(2,vec![(0.5,4.),(0.,0.5),(1.,0.5)]),(5,vec![(0.5,1.),(0.5,4.),(1.,2.)])] {
        let constant_document=matches!(echo,310..=313);
        if constant_document&&case!=0{continue;}
        let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"mediaBindings":{"a":{"id":"source","kind":"audio","durationTicks":8000000,"width":0,"height":0,"hasAudio":true,"audioChannels":if mono{1}else{2}}},"document":{"version":1,"media":[{"id":"a","kind":"audio","path":"/unopened/source.wav"}],"timelines":[{"id":"TL","video":{"width":16,"height":16,"rateNum":25,"rateDen":1},"tracks":[{"id":"a"}],"clips":[{"id":"clip","trackId":"a","kind":"audio","mediaId":"a","start":0.021,"duration":2.019,"sourceStart":1.019,"fadeIn":0.35,"fadeOut":0.27}]}]}})).unwrap();
        let report=import(request).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);let mut project=report.project.unwrap();
        let content=&mut project.compositions.get_mut(&project.root).unwrap().content;
        let CompositionTrack::Audio{clips,..}=&mut content.tracks[0]else{panic!()};let mut authored:AudioClip=clips[0].clone();
        authored.placement.source_out_tick=authored.placement.source_in_tick+(2.019*TimelineCurve::new(&points).unwrap().mean()*1e6).round()as u64;
        authored.placement.concat_retime=Some(SourceClock{points,duration_ticks:2019000,rate_num:25,rate_den:1,document_clock:None});
        let raw_seconds=matches!(echo,300|301|310..=313);
        if raw_seconds {
            let raw=video_kadr_backend::domain::concat_retime_edit::Clock{start:0.0199996,duration:2.01900049,source_start:0.0199996,speed:if constant_document{1.23456789}else{1.},speed_curve:if constant_document{None}else{Some(authored.placement.concat_retime.as_ref().unwrap().points.iter().map(|(at,speed)|video_kadr_backend::domain::concat_retime_edit::Point{at:*at,speed:*speed}).collect())}};
            authored.placement=raw.placement(25,1).unwrap();
        }
        authored.concat_gain_keys=wave_gain_keys(gain_mode);
        let (effects,reference_chain)=source_effect_case(echo);authored.dsp_effects=effects;
        let expanded=expand_audio(&authored).unwrap();
        *clips=expanded.into_iter().map(|piece|piece.clip).collect();
        let mut original=Vec::new();
        for line in scalar.lines(){let f=line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>();if f[0]!=(if constant_document{12.}else if raw_seconds{10.}else{2.})||f[1]!=(if constant_document{2.}else{case as f64})||f[2]!=(if raw_seconds{0.}else{2.019}){continue;}original.push(json!({"path":input,"start":f[4],"duration":f[5],"sourceStart":f[6],"speed":f[7],"fadeIn":f[8],"fadeOut":f[9]}));}
        assert_eq!(original.len(),if constant_document{1}else{21});
        if echo>0{for piece in &mut original{piece["filterChain"]=json!(reference_chain);}}
        if constant_document {
            original[0]["keys"]=json!(wave_gain_keys(gain_mode).iter().map(|key|json!({"at":key.at,"value":key.value,"ease":key.ease})).collect::<Vec<_>>());
        }else{
        let gain_file=std::path::PathBuf::from(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).with_file_name("gain-pieces.tsv");
        for line in std::fs::read_to_string(gain_file).unwrap().lines(){let f=line.split('\t').collect::<Vec<_>>();if f[1].parse::<usize>().unwrap()!=case||f[2].parse::<usize>().unwrap()!=gain_mode{continue;}let index=f[3].parse::<usize>().unwrap();if original[index].get("keys").is_none(){original[index]["keys"]=json!([]);}original[index]["keys"].as_array_mut().unwrap().push(json!({"at":f[4].parse::<f64>().unwrap(),"value":f[5].parse::<f64>().unwrap(),"ease":[f[6].parse::<f64>().unwrap(),f[7].parse::<f64>().unwrap(),f[8].parse::<f64>().unwrap(),f[9].parse::<f64>().unwrap()]}));}
        }

        let plan=state.outputs_dir().join(format!("reference-{case}.json"));std::fs::write(&plan,serde_json::to_vec(&json!({"duration":2.04,"clips":&original})).unwrap()).unwrap();
        let observed=state.outputs_dir().join(format!("observed-{case}.m4a"));let plain=state.outputs_dir().join(format!("plain-{case}.m4a"));let pcm=state.outputs_dir().join(format!("reference-{case}.f32"));
        for (output,tap) in [(&observed,pcm.as_os_str()),(&plain,std::ffi::OsStr::new("-"))]{let result=std::process::Command::new(&reference).arg(&plan).arg(output).arg(tap).output().unwrap();assert!(result.status.success(),"original mixer {case}: {}",String::from_utf8_lossy(&result.stderr));}
        let observed_aac=extracted_aac(&observed);assert!(!observed_aac.is_empty());assert_eq!(observed_aac,extracted_aac(&plain),"PCM observer changed original encoded packets");
        let expected=std::fs::read(&pcm).unwrap();assert_eq!(expected.len(),97920*8);
        let actual=if whole_frames{render_complete_frames(&state.process_runtime,&input,&metadata,&authored).await}else{
            let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
            let mixed=mix_project_audio_tracks(&state.process_runtime,Path::new("ffmpeg"),&state.outputs_dir(),&project,&project.root,0..2040000,97920,&sources,64*1024*1024,Duration::from_secs(60),&cancel,&tx).await.unwrap();std::fs::read(mixed.path()).unwrap()
        };assert_eq!(actual.len(),expected.len());
        if whole_frames{equal_pcm(&actual,&expected,&format!("source stem before host mixing {case}"));}
        if whole_frames {
            let layer=video_kadr_backend::domain::nested_audio::source_curve_owner_layer("root",0,0..2040000,&[],&authored).unwrap();
            let(tx,_)=tokio::sync::mpsc::unbounded_channel();
            let mixed=video_kadr_backend::tools::native_audio_export::mix_prepared_audio_layers(&state.process_runtime,Path::new("ffmpeg"),&state.outputs_dir(),vec![layer],0..2040000,97920,&sources,64*1024*1024,Duration::from_secs(60),&cancel,&tx,true).await.unwrap();
            equal_pcm(&std::fs::read(mixed.path()).unwrap(),&expected,&format!("common mixer source owner {case}"));
            if case==0 && gain_mode==0 {
                let mut half=authored.clone();half.role_gain=0.5;
                let layers=(0..2).map(|index|video_kadr_backend::domain::nested_audio::source_curve_owner_layer("nested",400000,900000..2440000,&[format!("instance-{index}")],&half).unwrap()).collect();
                let nested=video_kadr_backend::tools::native_audio_export::mix_prepared_audio_layers(&state.process_runtime,Path::new("ffmpeg"),&state.outputs_dir(),layers,900000..2440000,73920,&sources,64*1024*1024,Duration::from_secs(60),&cancel,&tx,true).await.unwrap();
                equal_pcm(&std::fs::read(nested.path()).unwrap(),&expected[24000*8..],"nested owner seek/multiple-instance gain");
            }

        }

        if whole_frames{equal_pcm(&actual,&expected,&format!("Rust frame-feed graph {case}"));}
        let mut max=0f64;let mut at=0;let mut bad=0;let mut square=0.;
        for(index,(a,b))in actual.chunks_exact(4).zip(expected.chunks_exact(4)).enumerate(){let a=f32::from_le_bytes(a.try_into().unwrap())as f64;let b=f32::from_le_bytes(b.try_into().unwrap())as f64;let diff=(a-b).abs();assert!(a.is_finite()&&b.is_finite());square+=diff*diff;if diff>max{max=diff;at=index;}if diff>2e-6{bad+=1;}}
        let result:Value=json!({"curve":case,"gainMode":gain_mode,"nativeEcho":echo,"sourceEffectCase":echo,"clippingSignal":hot,"referenceSha256":format!("{:x}",Sha256::digest(&expected)),"sourceCodec":codec,"sourceRate":rate,"sourceChannels":if mono{1}else{2},"sourceChannelLayout":metadata.channel_layout.as_deref(),"wholeFrameFeed":whole_frames,"maxAbsError":max,"maxErrorSample":at/2,"rmsError":(square/(actual.len()/4)as f64).sqrt(),"mismatchingScalars":bad,"totalScalars":actual.len()/4,"observerEncodedPacketsIdentical":true});println!("{result}");
        if let Ok(path)=std::env::var("CONCAT_RETIME_WAVEFORM_DIAGNOSTICS"){let mut file=std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();use std::io::Write;writeln!(file,"{result}").unwrap();}
        assert_eq!(bad,0,"curve audio differs from original mixer: {result}");
    }
}

async fn render_complete_frames(runtime:&video_kadr_backend::process_control::ProcessRuntime,input:&Path,metadata:&video_kadr_backend::domain::media_probe::StreamMetadata,clip:&AudioClip)->Vec<u8>{
    let directory=tempfile::tempdir().unwrap();
    let cancelled=CancellationToken::new();cancelled.cancel();
    let refused=video_kadr_backend::tools::concat_retime_audio::prepare_source_curve_stem(runtime,Path::new("/missing/ffmpeg"),Path::new("ffprobe"),input,metadata,clip,0..97920,directory.path(),64*1024*1024,Duration::from_secs(30),&cancelled).await.unwrap_err();assert!(refused.to_string().contains("cancelled"));
    let mut active=clip.clone();active.dsp_effects.push(serde_json::from_value(json!({"id":"amplify","params":{"gain":1}})).unwrap());
    let refused=video_kadr_backend::tools::concat_retime_audio::prepare_source_curve_stem(runtime,Path::new("/missing/ffmpeg"),Path::new("ffprobe"),input,metadata,&active,0..97920,directory.path(),64*1024*1024,Duration::from_secs(30),&CancellationToken::new()).await.unwrap_err();assert!(refused.to_string().contains("active source curve effects"));
    assert!(video_kadr_backend::tools::concat_retime_audio::prepare_source_curve_stem(runtime,Path::new("ffmpeg"),Path::new("ffprobe"),input,metadata,clip,0..97920,directory.path(),97920*8+100000,Duration::from_secs(30),&CancellationToken::new()).await.is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(),0,"failed stem preparation leaked source captures");
    let stem=video_kadr_backend::tools::concat_retime_audio::prepare_source_curve_stem(runtime,Path::new("ffmpeg"),Path::new("ffprobe"),input,metadata,clip,0..97920,directory.path(),64*1024*1024,Duration::from_secs(60),&CancellationToken::new()).await.unwrap();
    assert_eq!(stem.frames,97920);assert_eq!(stem.start_sample,0);
    let bytes=std::fs::read(stem.path()).unwrap();drop(stem);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(),0,"owned source stem data leaked");bytes
}

fn wave_gain_keys(mode:usize)->Vec<video_kadr_backend::domain::concat_effect_upgrade::ParamKey>{
    let keys=match mode{
        0=>json!([]),
        2=>json!([{"at":0,"value":0.2},{"at":1,"value":0.8,"ease":[0.42,0,0.58,1]}]),
        4=>json!([{"at":0.25,"value":0.2},{"at":0.75,"value":1.8,"ease":[0.42,-0.2,0.58,1.2]}]),
        3=>json!([{"at":1,"value":0.9},{"at":0.4,"value":1.2},{"at":0.4,"value":0.3,"ease":[0,0,0.58,1]},{"at":0,"value":0.1}]),
        _=>panic!("unknown gain mode"),
    };serde_json::from_value(keys).unwrap()
}
