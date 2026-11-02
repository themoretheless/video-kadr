use filmcraft_media::{wav::WavSource, MediaSource};
use serde_json::json;
fn chunk(out:&mut Vec<u8>,id:&[u8;4],bytes:&[u8]) {out.extend(id);out.extend((bytes.len() as u32).to_le_bytes());out.extend(bytes);if bytes.len()%2!=0 {out.push(0);}}
fn main(){
 let mut cases=vec![];let mut samples=0;
 for (tag,bits) in [(1u16,8u16),(1,16),(1,24),(1,32),(3,32),(3,64)] {for channels in [1u16,2,4] {for rate in [44100u32,48000] {for mode in 0..3 {
 let mut bytes=if mode==1 {b"RF64\0\0\0\0WAVE".to_vec()} else {b"RIFF\0\0\0\0WAVE".to_vec()};
 let mut fmt=vec![];fmt.extend((if mode==2 {0xFFFEu16}else{tag}).to_le_bytes());fmt.extend(channels.to_le_bytes());fmt.extend(rate.to_le_bytes());fmt.extend((rate*channels as u32*bits as u32/8).to_le_bytes());fmt.extend((channels*bits/8).to_le_bytes());fmt.extend(bits.to_le_bytes());
 if mode==2 {fmt.extend([0u8;24]);fmt[24..26].copy_from_slice(&tag.to_le_bytes());}
 chunk(&mut bytes,b"fmt ",&fmt);chunk(&mut bytes,b"JUNK",&[1,2,3]);
 let mut bext=vec![0;346];bext[338..342].copy_from_slice(&12345u32.to_le_bytes());
 let xml=b"<BWF_TIME_REFERENCE_LOW>6789</BWF_TIME_REFERENCE_LOW><BWF_TIME_REFERENCE_HIGH>1</BWF_TIME_REFERENCE_HIGH>";
 if mode==1 {chunk(&mut bytes,b"bext",&bext);chunk(&mut bytes,b"iXML",xml);} else if mode==2 {chunk(&mut bytes,b"iXML",xml);}
 let mut pcm=vec![];
 for frame in 0..53 {for channel in 0..channels {let n=(frame*13+channel as i32*7)%31-15;match(tag,bits){
 (1,8)=>pcm.push((128+n*7) as u8),
 (1,16)=>pcm.extend((n as i16*1973).to_le_bytes()),
 (1,24)=>pcm.extend(&(n*500123i32).to_le_bytes()[..3]),
 (1,32)=>pcm.extend((n*133123457i32).to_le_bytes()),
 (3,32)=>pcm.extend((n as f32/17.).to_le_bytes()),
 (3,64)=>pcm.extend((n as f64/19.).to_le_bytes()),_=>unreachable!()}}}
 chunk(&mut bytes,b"data",&pcm);let len=bytes.len() as u32-8;bytes[4..8].copy_from_slice(&len.to_le_bytes());
 let source=WavSource::parse("oracle.wav",bytes.clone().into()).unwrap();let mut requests=vec![];
 for out_rate in [32000,44100,96000] {for start in [-7,0,37,105] {let audio=source.audio(start,31,out_rate).unwrap();let bits:Vec<Vec<u32>>=audio.channels.iter().map(|ch|ch.iter().map(|v|v.to_bits()).collect()).collect();samples+=bits.iter().map(Vec::len).sum::<usize>();requests.push(json!({"start":start,"rate":out_rate,"bits":bits}));}}
 cases.push(json!({"bytes":bytes,"time_reference":source.time_reference(),"requests":requests}));
 }}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","samples":samples,"cases":cases}));
}
