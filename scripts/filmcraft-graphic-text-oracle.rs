//! Independent original rich graphic text raster fixtures (Filmcraft MIT).
use filmcraft_project::graphic::{LayerContent,eval_layer};
use filmcraft_project::graphic_design::CharStyle;
use filmcraft_render::graphic_clip::raster_layer;
use serde_json::json;
fn main() {
 let mut cases=vec![];
 for text in ["Office AV","Привет мир","مرحبا بالعالم","A\nB"] { for profile in 0..3 {for appearance in 0..3 {for matrix in [[1.,0.,0.,1.,16.,30.],[0.8,0.2,-0.2,0.8,50.25,24.75]] {
 let effect=filmcraft_project::effect_defs().iter().find(|d| d.id=="graphic_text").unwrap().instance();
 let mut spec=eval_layer(&effect,filmcraft_time::Tick::ZERO,(128,96)).unwrap();
 let LayerContent::Text(t)=&mut spec.content else {panic!("text")};
 t.text=text.into();t.font="Inter".into();t.style="Regular".into();t.size=18.;t.tracking=15.;t.leading=22.;t.baseline_shift=1.25;t.align=profile;t.box_width=if profile==1{70.}else{0.};t.box_height=48.;t.vertical=profile==2;t.underline=profile==1;t.caps=profile;t.faux_italic=profile==2;
 let end=text.char_indices().nth(2).map_or(text.len(),|(i,_)|i);
 t.runs=vec![(0..end,CharStyle{font:Some("Noto Serif".into()),font_style:Some("Bold".into()),size:Some(23.),fill:Some([0.9,0.15,0.3,0.7]),tracking:Some(45.),baseline_shift:Some(2.5),faux_bold:Some(true),underline:Some(true),..Default::default()})];
 let input=json!({"text":t.text,"font":t.font,"style":t.style,"size":t.size,"align":t.align,"tracking":t.tracking,"kerning":t.kerning,"ligatures":t.ligatures,"leading":t.leading,"baselineShift":t.baseline_shift,"fauxBold":t.faux_bold,"fauxItalic":t.faux_italic,"caps":t.caps,"underline":t.underline,"boxWidth":t.box_width,"boxHeight":t.box_height,"vertical":t.vertical,"runs":t.runs});
 spec.transform.opacity=0.65;
 spec.appearance.fill=Some([0.2,0.7,0.8,0.8]);
 if appearance>0 {spec.appearance.strokes=vec![([0.7,0.1,0.2,0.9],1.5,appearance),([0.1,0.9,0.2,0.6],2.5,0)];}
 if appearance==2 {spec.appearance.background=Some(([0.1,0.2,0.6,0.4],3.,2.));spec.appearance.shadow=Some(filmcraft_project::graphic::Shadow{color:[0.1,0.1,0.2,0.6],offset:(3.,4.),size:1.,blur:4.});}
 let a=&spec.appearance;
 let ap=json!({"fill":a.fill,"strokes":a.strokes,"background":a.background,"shadow":a.shadow.as_ref().map(|s|json!({"color":s.color,"offset":s.offset,"size":s.size,"blur":s.blur}))});
 let raster=raster_layer(&spec,&matrix,128,96).map(|r|json!({"x":r.x,"y":r.y,"width":r.img.w,"height":r.img.h,"bits":r.img.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 cases.push(json!({"text":input,"appearance":ap,"opacity":spec.transform.opacity,"matrix":matrix,"raster":raster}));
 }}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
