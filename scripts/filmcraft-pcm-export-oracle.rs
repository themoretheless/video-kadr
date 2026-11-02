#[path="__PCM_PATH__"] mod pcm;
use serde_json::json;
fn main(){
 let input=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
 let fixture:serde_json::Value=serde_json::from_slice(&input).unwrap();let mut cases=vec![];
 for case in fixture["cases"].as_array().unwrap(){let rate=case["document"]["project"]["items"][case["root"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["settings"]["sample_rate"].as_u64().unwrap() as u32;let mut requests=vec![];
 for request in case["requests"].as_array().unwrap(){let bus:Vec<Vec<f32>>=request["bits"].as_array().unwrap().iter().map(|ch|ch.as_array().unwrap().iter().map(|v|f32::from_bits(v.as_u64().unwrap() as u32)).collect()).collect();let mut outputs=vec![];
 for channels in [1u16,2]{for bits in [16u16,24]{let mut inter=vec![];for i in 0..bus[0].len(){if channels==1 {inter.push(0.5*(bus[0][i]+bus[1][i]));}else{inter.push(bus[0][i]);inter.push(bus[1][i]);}} outputs.push(json!({"channels":channels,"bits":bits,"wav":pcm::write_wav(&inter,channels,rate,bits),"aiff":pcm::write_aiff(&inter,channels,rate,bits)}));}}
 requests.push(json!(outputs));}cases.push(json!(requests));}
 println!("{}",json!({"commit":fixture["commit"],"cases":cases}));}
