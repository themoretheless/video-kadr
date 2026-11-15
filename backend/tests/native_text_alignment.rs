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

#[test]
fn explicit_text_anchor_moves_the_glyphs_around_a_fixed_position() {
 let mut base = clip();
 base.text = "MMMM".into();
 let original = pixels(&base);
 base.text_anchor = Some([0.5, 0.5]);
 assert_eq!(pixels(&base), original);
 let bounds = |clip: &TextClip| {
  let frame = pixels(clip);
  let mut bounds = [320, 180, 0, 0];
  for (index, pixel) in frame.chunks_exact(4).enumerate() {
   if pixel[3] == 0 { continue; }
   let (x, y) = (index % 320, index / 320);
   bounds = [bounds[0].min(x), bounds[1].min(y), bounds[2].max(x), bounds[3].max(y)];
  }
  bounds
 };
 base.text_anchor = Some([0.0, 0.0]);
 let top_left = bounds(&base);
 base.text_anchor = Some([1.0, 1.0]);
 let bottom_right = bounds(&base);
 // The layout box includes font bearings and line metrics beyond the ink.
 assert!((160..=168).contains(&top_left[0]), "{top_left:?}");
 assert!((90..=100).contains(&top_left[1]), "{top_left:?}");
 assert!((152..=160).contains(&bottom_right[2]), "{bottom_right:?}");
 assert!((80..=90).contains(&bottom_right[3]), "{bottom_right:?}");
}

#[test]
fn native_text_raster_uses_bold_and_italic_from_the_document() {
 let base = clip();
 let regular = pixels(&base);
 let mut bold = base.clone();
 bold.style.bold = true;
 let mut italic = base.clone();
 italic.style.italic = true;
 let mut combined = bold.clone();
 combined.style.italic = true;
 assert_ne!(pixels(&bold), regular);
 assert_ne!(pixels(&italic), regular);
 assert_ne!(pixels(&combined), pixels(&bold));
 assert_ne!(pixels(&combined), pixels(&italic));
 let restored: TextClip = serde_json::from_value(serde_json::to_value(&combined).unwrap()).unwrap();
 assert_eq!(pixels(&restored), pixels(&combined));
}

#[test]
fn caption_box_wraps_without_reducing_the_font_size() {
 let mut base = clip();
 base.text = "MMMM MMMM MMMM".into();
 let unwrapped = pixels(&base);
 let single_line = lines(&unwrapped);
 assert_eq!(single_line.len(), 1);
 base.text_box_width = Some(140.);
 let wrapped = pixels(&base);
 let wrapped_lines = lines(&wrapped);
 assert_eq!(wrapped_lines.len(), 3, "{wrapped_lines:?}");
 for (left, right) in wrapped_lines {
  assert!(right - left < 140);
 }
 let widest_ink_column = |frame: &[u8]| {
  (0..320).map(|x| (0..180).filter(|&y| frame[(y * 320 + x) * 4 + 3] > 0).count()).max().unwrap()
 };
 assert_eq!(widest_ink_column(&wrapped), 3 * widest_ink_column(&unwrapped));
}
