use serde_json::json;
fn main(){
 let mut inputs=Vec::new();
 for channel in 0..4 {for byte in 0..=255 {let mut rgba=[0_u8,85,170,255];rgba[channel]=byte;inputs.push(format!("#{:02x}{:02x}{:02x}{:02x}",rgba[0],rgba[1],rgba[2],rgba[3]));}}
 for text in ["#112233","112233"," ##aAbBcC80 ","","#fff","#gg1122","#123456789","#é12345","#112233zz"] {inputs.push(text.into());}
 let cases:Vec<_>=inputs.into_iter().map(|input|{let bits=filmcraft_color::parse_hex(&input).map(|value|value.map(f32::to_bits));json!({"input":input,"bits":bits})}).collect();
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
