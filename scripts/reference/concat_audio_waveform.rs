// Observe the original mix before AAC without changing its sample clock.
use std::io::Write;
static PCM_TAP:std::sync::OnceLock<std::sync::Mutex<std::fs::File>>=std::sync::OnceLock::new();
fn capture_pcm(frame:&ffmpeg_the_third::util::frame::audio::Audio){
    let Some(tap)=PCM_TAP.get()else{return;};
    let mut tap=tap.lock().unwrap();
    let left=frame.plane::<f32>(0);let right=frame.plane::<f32>(1);
    let mut bytes=Vec::with_capacity(frame.samples()*8);
    for i in 0..frame.samples(){bytes.extend_from_slice(&left[i].to_le_bytes());bytes.extend_from_slice(&right[i].to_le_bytes());}
    tap.write_all(&bytes).unwrap();
}
fn main(){
    use concat_core::animate::{Ease,Key,Track};
    let args=std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(),4,"plan.json out.m4a tap.f32-or-dash");
    if args[3]!="-"{PCM_TAP.set(std::sync::Mutex::new(std::fs::File::create(&args[3]).unwrap())).unwrap();}
    let plan:serde_json::Value=serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let number=|v:&serde_json::Value,key:&str,default:f64|v.get(key).and_then(|v|v.as_f64()).unwrap_or(default);
    let clips=plan["clips"].as_array().unwrap().iter().map(|clip|{
        let keys=clip["keys"].as_array().into_iter().flatten().map(|key|{
            let ease=key.get("ease").and_then(|v|v.as_array());
            let e=|i:usize,default:f64|ease.and_then(|v|v.get(i)).and_then(|v|v.as_f64()).unwrap_or(default);
            Key{at:number(key,"at",0.),value:number(key,"value",1.),ease:Ease{x1:e(0,0.),y1:e(1,0.),x2:e(2,1.),y2:e(3,1.)}}
        }).collect();
        audio::AudioClip{path:std::path::PathBuf::from(clip["path"].as_str().unwrap()),stream:None,start:number(clip,"start",0.),duration:number(clip,"duration",1.),source_start:number(clip,"sourceStart",0.),speed:number(clip,"speed",1.),preserve_pitch:true,volume:number(clip,"volume",1.),volume_curve:Track::new(keys),fade_in:number(clip,"fadeIn",0.),fade_out:number(clip,"fadeOut",0.),filter_chain:clip.get("filterChain").and_then(|v|v.as_str()).unwrap_or("").to_owned()}
    }).collect::<Vec<_>>();
    audio::mix_to_file(&clips,plan["duration"].as_f64().unwrap(),std::path::Path::new(&args[2])).unwrap();
}
