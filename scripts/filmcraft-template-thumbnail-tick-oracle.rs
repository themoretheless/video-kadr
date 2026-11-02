use filmcraft_project::{Project,Label,ItemKind,TrackKind,gtemplate::GraphicsTemplate,graphic_design::RollMode};
use filmcraft_time::{FrameRate,Tick,TimeRange};
use filmcraft_geom::Affine;
use serde_json::{json,Value};
fn main(){
 let input:Value=serde_json::from_slice(&std::fs::read(std::env::var("VIDEOKADR_TEMPLATE_INPUT").unwrap()).unwrap()).unwrap();
 let mut cases=Vec::new();
 for original in input["cases"].as_array().unwrap(){for duration_us in [1_i64,3,33333,999999]{ for responsive in [false,true]{
  let animated=true;
  let mut document=original.clone();
  if animated {document["layers"][0]["params"]["opacity"]["keyframes"]=json!([{"time":0,"value":{"Float":0},"interp":"Linear"},{"time":508032000000_i64,"value":{"Float":100},"interp":"Linear"}]);}
  document["duration"]=json!(duration_us*254016+1);
  if responsive {document["graphic"]=json!({"intro":254017,"outro":508033,"designIn":-762047,"designDuration":22861694017_i64});}
  document["layers"][0]["params"]["opacity"]["keyframes"]=json!([{"time":-254016,"value":{"Float":0},"interp":"Linear"},{"time":duration_us.max(7)*254016,"value":{"Float":100},"interp":"Linear"}]);
  let template=GraphicsTemplate::from_bytes(&serde_json::to_vec(&document).unwrap()).unwrap();
  for width in [16_u32,80]{
   let height=((width as f64*template.canvas[1] as f64/template.canvas[0].max(1) as f64).round() as u32).max(1);
   let mut meta=template.graphic.clone();meta.roll.mode=RollMode::Off;
   let mut project=Project::new("thumbnail oracle");let rate=FrameRate::FPS_30;
   let graphic=project.add_item("G",Label::Rose,ItemKind::Graphic{width:template.canvas[0],height:template.canvas[1],rate},None);
   let mut item=project.make_track_item(graphic,TrackKind::Video,Tick::ZERO,TimeRange::new(Tick::ZERO,template.duration.max(rate.frame_duration())),rate).unwrap();
   item.effects.extend(template.layers.iter().cloned());item.graphic=Some(Box::new(meta));
   let at=Tick((template.duration.0/2).min(filmcraft_time::TICKS_PER_SECOND));
   let mut image=filmcraft_render::image::Image::new(width as usize,height as usize);
   let scale=width as f64/template.canvas[0].max(1) as f64;
   filmcraft_render::graphic_clip::render_graphic(&item,at,(template.canvas[0],template.canvas[1]),&Affine::scale(scale,scale),&mut image);
   cases.push(json!({"document":document,"width":width,"height":height,"animated":animated,"sampleTick":filmcraft_render::graphic_clip::graphic_time(&item,at).0.0,"opacityBits":filmcraft_project::graphic::eval_layer(&template.layers[0],filmcraft_render::graphic_clip::graphic_time(&item,at).0,(template.canvas[0],template.canvas[1])).unwrap().transform.opacity.to_bits(),"bits":image.px.iter().map(|value|value.to_bits()).collect::<Vec<_>>(),"rgba":image.to_rgba8()}));
  }
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
