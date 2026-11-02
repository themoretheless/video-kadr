use filmcraft_project::{Caption,CaptionStyle,CaptionAlign,CaptionAnchor,ClipId};
use filmcraft_time::Tick;
use serde_json::json;
fn main(){
 let texts=["Plain caption","Hello <i>there</i>","<b>Bold <i>both</i></b>","<u>Underlined</u>","&amp; &lt; &gt; &quot; &nbsp; &apos;","Привет 🙂 мир","Line one\nLine two","<unknown>visible</unknown>","literal < without close","", "\n  Second line  ","word word word word word word word word word word word word word word word word word word word word", "{\\an8}Override", "\t  trimmed  \r\n line ","aSuperLongUnbrokenWordThatDoesNotFit", "&lrm;RTL &rlm; سلام"];
 let mut cases=vec![];
 for (index,text) in texts.iter().enumerate(){for (w,h) in [(80usize,64usize),(320,180)] {
  let mut style=CaptionStyle::default();style.size=42.+index as f32;style.align=[CaptionAlign::Left,CaptionAlign::Center,CaptionAlign::Right][index%3];style.anchor=[CaptionAnchor::Top,CaptionAnchor::Middle,CaptionAnchor::Bottom][index%3];style.background=index%2==0;style.color=[171,205,239,128];style.background_color=[18,52,86,128];style.outline=if index%4==0{2.}else{0.};style.outline_color=[35,69,103,255];style.font="Fallback-name".into();
  let settings=["","line:90% align:start","line:50%,start align:end","line:0% align:center"][index%4];
  let cue=Caption{id:ClipId(1),start:Tick(0),duration:Tick(254016000000),text:text.to_string(),speaker:Some("Narrator".into()),cue_id:None,settings:settings.into()};
  let overlay=filmcraft_captions::burn::render_caption(&cue,&style,w,h);
  let result=overlay.map(|overlay|json!({"x":overlay.x,"y":overlay.y,"w":overlay.w,"h":overlay.h,"bits":overlay.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  cases.push(json!({"text":text,"settings":settings,"width":w,"height":h,"style":{"font":style.font,"size":style.size,"align":format!("{:?}",style.align).to_lowercase(),"anchor":format!("{:?}",style.anchor).to_lowercase(),"background":style.background,"color":style.color,"backgroundColor":style.background_color,"outline":style.outline,"outlineColor":style.outline_color,"margin":style.margin,"lineSpacing":style.line_spacing},"overlay":result}));
 }}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
