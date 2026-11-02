use serde_json::json;
use video_kadr_backend::domain::{composition::{TextAlignment,TextClip,TransformSpec},legacy_text_surface};

fn clip()->TextClip{
 serde_json::from_value(json!({"id":"title","timelineStartTick":0,"timelineEndTick":1000000,"text":"MMMMMMMM\nM","style":{"fontFamily":"Noto Sans","fontSize":24,"color":{"red":1,"green":1,"blue":1,"alpha":1},"background":{"red":0,"green":0,"blue":0,"alpha":0},"stroke":{"red":0,"green":0,"blue":0,"alpha":0},"strokeWidth":0,"shadow":{"red":0,"green":0,"blue":0,"alpha":0},"shadowX":0,"shadowY":0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1}})).unwrap()
}
fn pixels(clip:&TextClip)->Vec<u8>{legacy_text_surface::render(clip,(320,180),500000,&mut||Ok(())).unwrap().unwrap().to_surface(&mut||Ok(())).unwrap().rgba}
fn lines(pixels:&[u8])->Vec<(usize,usize)>{
 let mut result=vec![];let mut current:Option<(usize,usize)>=None;
 for row in pixels.chunks_exact(320*4){
  let xs:Vec<_>=row.chunks_exact(4).enumerate().filter_map(|(x,p)|(p[3]>0).then_some(x)).collect();
  if let(Some(first),Some(last))=(xs.first(),xs.last()) {current=Some(current.map_or((*first,*last),|(min,max)|(min.min(*first),max.max(*last))));}
  else if let Some(bounds)=current.take(){result.push(bounds);}
 }
 if let Some(bounds)=current{result.push(bounds);}result
}
#[test]
fn multiline_alignment_moves_the_short_line_and_preserves_legacy_default(){
 let base=clip();let left=pixels(&base);let mut changed=base.clone();changed.style.align=Some(TextAlignment::Left);assert_eq!(pixels(&changed),left);
 changed.style.align=Some(TextAlignment::Center);let center=pixels(&changed);
 changed.style.align=Some(TextAlignment::Right);let right=pixels(&changed);
 let l=lines(&left);let c=lines(&center);let r=lines(&right);
 assert_eq!(l.len(),2);assert_eq!(c.len(),2);assert_eq!(r.len(),2);
 assert_eq!(l[0],c[0]);assert_eq!(l[0],r[0]);
 assert!(l[1].0<c[1].0&&c[1].0<r[1].0);
 assert_eq!(l[1].1-l[1].0,c[1].1-c[1].0);assert_eq!(l[1].1-l[1].0,r[1].1-r[1].0);
 assert!(serde_json::from_value::<TextClip>({let mut value=serde_json::to_value(base).unwrap();value["style"]["align"]=json!("justify");value}).is_err());
}
