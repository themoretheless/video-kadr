//! Native raster bridge for the ordinary timeline text clip.
use super::{composition::{TextClip,Rgba},graphic_text::GraphicTextParameters,graphic_clip::{GraphicClip,GraphicLayer,GraphicContent},graphic_timeline::GraphicTimelineClip,linear_color_frame::LinearColorFrame};
use anyhow::Result;
fn rgba(color:Rgba)->[f32;4]{[color.red as f32,color.green as f32,color.blue as f32,color.alpha as f32]}

pub fn render(clip:&TextClip,canvas:(u32,u32),time:u64,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<LinearColorFrame>> {
    if !clip.enabled || time<clip.timeline_start_tick || time>=clip.timeline_end_tick {return Ok(None);}
    if let Some(style)=&clip.concat_title {return super::concat_title::render(clip,style,canvas,time,checkpoint);}
    checkpoint()?;
    let local=time-clip.timeline_start_tick;
    let color=if let Some(track)=&clip.color_animation {
        let s=clip.style.color;
        let default=super::animated_color::Color{r:(s.red*255.).round()as u8,g:(s.green*255.).round()as u8,b:(s.blue*255.).round()as u8,a:(s.alpha*255.).round()as u8};
        let sampled=super::animated_color::PreparedColorTrack::new(super::animated_color::ColorSampleRequest{track:track.clone(),default,times_us:vec![]}).map_err(anyhow::Error::msg)?.sample(local as f64).map_err(anyhow::Error::msg)?;
        [sampled.r as f32/255.,sampled.g as f32/255.,sampled.b as f32/255.,sampled.a as f32/255.]
    }else{rgba(clip.style.color)};
    let dx=clip.style.shadow_x.round()as f32;let dy=clip.style.shadow_y.round()as f32;
    let mut parameters=GraphicTextParameters{
        text:clip.text.clone(),font:clip.style.font_family.clone(),size:clip.style.font_size as f32,
        fill:true,fill_color:color,align:match clip.style.align {None|Some(super::composition::TextAlignment::Left)=>0,Some(super::composition::TextAlignment::Center)=>1,Some(super::composition::TextAlignment::Right)=>2},
        stroke:clip.style.stroke_width.round()>0.,stroke_width:clip.style.stroke_width.round()as f32,stroke_color:rgba(clip.style.stroke),stroke_type:0,
        background:clip.style.background.alpha>0.,background_color:rgba(clip.style.background),background_opacity:100.,background_size:0.,background_radius:0.,
        shadow:clip.style.shadow.alpha>0.,shadow_color:rgba(clip.style.shadow),shadow_opacity:100.,shadow_distance:dx.hypot(dy),shadow_angle:dy.atan2(dx).to_degrees()+90.,shadow_size:0.,shadow_blur:0.,
        position:Some([canvas.0 as f64/2.,canvas.1 as f64/2.]),..Default::default()
    };
    // Anchor against the complete multiline layout before the full-canvas clip transform.
    let (layer,_)=parameters.evaluate(true,canvas)?;
    let bounds=super::graphic_text_layout::text_layout(&layer.text).bounds;
    parameters.anchor=[bounds[0]as f64+clip.transform.anchor_x*(bounds[2]-bounds[0])as f64,bounds[1]as f64+clip.transform.anchor_y*(bounds[3]-bounds[1])as f64];
    let graphic=GraphicTimelineClip{concat_shape:None,concat_treatment:None,concat_shape_blend:None,id:clip.id.clone(),timeline_start_tick:clip.timeline_start_tick,timeline_end_tick:clip.timeline_end_tick,animation_offset_us:0,template:None,roll:None,responsive_time:None,width:canvas.0,height:canvas.1,
        graphic:GraphicClip{layers:vec![GraphicLayer{original_parameters:None,uid:1,enabled:true,pin:None,content:GraphicContent::Text{parameters,animation:None,source_text_animation:None}}]},effects:vec![],transform:clip.transform.clone(),opacity:clip.opacity.clone(),enabled:clip.enabled};
    graphic.render_timeline(canvas,time,&mut|_,_,_,_|Ok(()),checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::composition::TransformSpec;
    fn clip()->TextClip {
        serde_json::from_value(serde_json::json!({"id":"text","timelineStartTick":1000000,"timelineEndTick":3000000,"text":"Text\nДва","style":{"fontFamily":"Noto Sans","fontSize":12,"color":{"red":1,"green":1,"blue":1,"alpha":1},"background":{"red":0,"green":0,"blue":0,"alpha":0},"stroke":{"red":1,"green":0,"blue":0,"alpha":0},"strokeWidth":0,"shadow":{"red":0,"green":0,"blue":1,"alpha":0},"shadowX":2,"shadowY":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1}})).unwrap()
    }
    #[test]
    fn ordinary_text_has_visible_style_motion_opacity_and_half_open_admission() {
        let base=clip();
        let pixels=|clip:&TextClip|render(clip,(64,48),1500000,&mut||Ok(())).unwrap().unwrap().to_surface(&mut||Ok(())).unwrap();
        let original=pixels(&base);
        assert!(original.rgba.chunks_exact(4).any(|p|p[3]>0));
        for variant in 0..5 {
            let mut changed=base.clone();
            match variant {
                0=>changed.style.background=Rgba{red:0.,green:0.5,blue:0.,alpha:0.8},
                1=>{changed.style.stroke_width=2.;changed.style.stroke.alpha=1.;},
                2=>changed.style.shadow.alpha=1.,
                3=>changed.opacity=super::super::composition::AnimatableValue::Constant{value:0.25},
                _=>changed.transform.x=super::super::composition::AnimatableValue::Constant{value:10.},
            }
            assert_ne!(pixels(&changed),original,"style variant {variant}");
        }
        assert!(render(&base,(64,48),999999,&mut||Ok(())).unwrap().is_none());
        assert!(render(&base,(64,48),3000000,&mut||Ok(())).unwrap().is_none());
        let mut disabled=base;disabled.enabled=false;
        assert!(render(&disabled,(64,48),1500000,&mut||Ok(())).unwrap().is_none());
        assert!(render(&clip(),(64,48),1500000,&mut||anyhow::bail!("cancel")).is_err());
    }
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn text_foreground_animation_uses_microsecond_local_clock_without_recoloring_decorations() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/concat-merge-preview.json")).unwrap();
        let mut clip:TextClip=serde_json::from_value(serde_json::json!({"id":"text","timelineStartTick":1000000,"timelineEndTick":4000000,"text":"T","style":{"fontFamily":"Noto Sans","fontSize":16,"color":{"red":1,"green":1,"blue":1,"alpha":1},"background":{"red":0,"green":0.5,"blue":0,"alpha":0.8},"stroke":{"red":0,"green":0,"blue":0,"alpha":0},"strokeWidth":0,"shadow":{"red":0,"green":0,"blue":0,"alpha":0},"shadowX":0,"shadowY":0},"transform":fixture["createdTimeline"]["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["transform"],"opacity":{"mode":"constant","value":1},"colorAnimation":{"keys":[{"t_us":0,"value":{"r":255,"g":0,"b":0,"a":255},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}},{"t_us":2000000,"value":{"r":0,"g":0,"b":255,"a":255},"in_":[0,0],"out":[0,0],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}}})).unwrap();
        for local in [0u64,250000,1000000,2000000] {
            let animated=render(&clip,(64,48),1000000+local,&mut||Ok(())).unwrap().unwrap().to_surface(&mut||Ok(())).unwrap();
            let track=clip.color_animation.take().unwrap();
            let color=super::super::animated_color::PreparedColorTrack::new(super::super::animated_color::ColorSampleRequest{track:track.clone(),default:super::super::animated_color::Color{r:255,g:255,b:255,a:255},times_us:vec![]}).unwrap().sample(local as f64).unwrap();
            clip.style.color=Rgba{red:color.r as f64/255.,green:color.g as f64/255.,blue:color.b as f64/255.,alpha:color.a as f64/255.};
            let reference=render(&clip,(64,48),1000000+local,&mut||Ok(())).unwrap().unwrap().to_surface(&mut||Ok(())).unwrap();
            assert_eq!(animated,reference,"local microseconds {local}");
            assert!(animated.rgba.chunks_exact(4).any(|p|p[1]>p[0]&&p[1]>p[2]),"background stays green");
            clip.color_animation=Some(track);
        }
    }
}
