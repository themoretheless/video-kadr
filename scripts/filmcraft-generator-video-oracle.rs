//! Original GeneratorSource::video_frame, including its scale/cache/clock path.
use filmcraft_media::{generators::GeneratorSource,Generator,DemoScene,MediaSource,FrameRequest};
use filmcraft_time::{FrameRate,Tick,TICKS_PER_SECOND as T};
use serde_json::json;
fn main(){
 let mut generators=vec![Generator::BarsAndTone,Generator::ColorMatte{color:[0.125,0.5,0.875,0.25]},Generator::ColorMatte{color:[-0.1,1.5,0.0,1.0]},Generator::BlackVideo,Generator::TransparentVideo,Generator::CountingLeader,Generator::Tone{hz:440.,db:-6.}];
 generators.extend(DemoScene::ALL.into_iter().map(Generator::Demo));let mut cases=vec![];
 for generator in generators{for (width,height) in [(2,2),(17,11),(64,36)]{for rate in [FrameRate::FPS_24,FrameRate::FPS_23_976,FrameRate::FPS_25]{
 let source=GeneratorSource::new(generator.clone(),width,height,rate,Tick(T));
 for time in [-T,0,T/3,2*T,8*T+T/7]{for scale in [-1.0f32,0.03,0.333,1.0,2.0]{
 let mut case=json!({"generator":generator,"width":width,"height":height,"rate":rate,"time":time,"scale":scale});
 match source.video_frame(FrameRequest{time:Tick(time),scale}){Ok(frame)=>{case["outputWidth"]=json!(frame.width);case["outputHeight"]=json!(frame.height);case["pts"]=json!(frame.pts.0);case["rgbaHex"]=json!(frame.to_rgba8().iter().map(|v|format!("{v:02x}")).collect::<String>());},Err(_)=>case["noVideo"]=json!(true)}
 cases.push(case);
 }}}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
