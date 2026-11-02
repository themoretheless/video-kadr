use filmcraft_project::graphic_design::{Roll,RollMode,roll_progress,roll_offset};
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];
for mode in RollMode::ALL {for start in [false,true] {for end in [false,true] {for times in [[0,0,0,0],[3,4,5,2],[20,30,40,25],[-2,-3,-4,-5],[2,100,100,3]] {for d in [0,10,50] {for u in [-2,0,1,3,9,10,25,49,51] {for bounds in [[10.,12.,24.,28.],[-20.5,-30.25,100.75,80.125],[0.;4]] {
let r=Roll{mode,start_off_screen:start,end_off_screen:end,preroll:Tick(times[0]),ease_in:Tick(times[1]),ease_out:Tick(times[2]),postroll:Tick(times[3])};let offset=roll_offset(&r,Tick(u),Tick(d),(64,48),bounds);
cases.push(json!({"roll":r,"u":u,"d":d,"canvas":[64,48],"bounds":bounds,"progressBits":roll_progress(&r,Tick(u),Tick(d)).to_bits(),"offsetBits":[offset.0.to_bits(),offset.1.to_bits()]}));
}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
