//! Original Filmcraft character formatting command fixtures (MIT).
use filmcraft_project::graphic_design::{CharStyle,StyleRun,normalize_runs,apply_char_style,clear_char_style,adjust_runs};
use serde_json::json;
fn encoded(runs:&[StyleRun])->serde_json::Value {json!(runs.iter().map(|r|json!({"start":r.start,"end":r.end,"style":r.style})).collect::<Vec<_>>())}
fn main(){let mut cases=vec![];
for text in ["","AБ🙂Z","Office AV","مرحبا"] {let n=text.chars().count();
for profile in 0..3 {let red=CharStyle{fill:Some([0.9,0.2,0.1,0.7]),font:Some("Noto Serif".into()),..Default::default()};let underline=CharStyle{underline:Some(true),tracking:Some(25.),..Default::default()};
let runs=match profile{0=>vec![],1=>vec![StyleRun{start:0,end:n,style:red}],_=>vec![StyleRun{start:1,end:n+2,style:underline.clone()},StyleRun{start:0,end:2,style:red}]};
let style=CharStyle{faux_bold:Some(true),underline:Some(false),size:Some(28.),..Default::default()};
for (start,end) in [(0,n),(1,3),(3,1),(n+2,n+4)] {for operation in ["normalize","apply","clear","adjust"] {
let new_text=format!("🙂{text}!");
let expected=match operation{"normalize"=>normalize_runs(&runs,n),"apply"=>apply_char_style(&runs,n,start,end,&style),"clear"=>clear_char_style(&runs,n,start,end),_=>adjust_runs(text,&new_text,&runs)};
cases.push(json!({"text":text,"newText":new_text,"runs":encoded(&runs),"start":start,"end":end,"operation":operation,"style":style,"expected":encoded(&expected)}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
