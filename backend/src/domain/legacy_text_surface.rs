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
    let font_style = clip.style.face_style();
    let size = clip.style.font_size as f32;
    let (dx, dy, shadow_color, shadow_blur) = if clip.style.concat_shadow {
        // Match the source offset and opacity, using the native mask blur.
        let radius = (size * 0.04).round().max(1.);
        // Convert the variance of two source box passes to the native blur parameter (3σ).
        let blur = (6. * radius * (radius + 1.)).sqrt();
        (size * 0.05, size * 0.07, [0., 0., 0., 150. / 255.], blur)
    } else {
        (clip.style.shadow_x.round() as f32, clip.style.shadow_y.round() as f32, rgba(clip.style.shadow), 0.)
    };
    let leading = clip.style.line_height.map_or(0., |ratio| {
        use filmcraft_text::fonts::{face, resolve};
        let metrics = face(resolve(&clip.style.font_family, &font_style).face).metrics(size);
        ratio * size - (metrics.ascent + metrics.descent + metrics.line_gap).max(size * 0.5)
    });
    let mut parameters=GraphicTextParameters{
        text:clip.text.clone(),font:clip.style.font_family.clone(),size,
        font_style: font_style.into_owned(),
        leading,
        tracking: clip.style.tracking.unwrap_or_default() * 1000. / clip.style.font_size as f32,
        box_width: clip.text_box_width.unwrap_or_default(),
        min_box_height: clip.text_box_height,
        block_padding: clip.style.background_padding.filter(|_| clip.style.background.alpha > 0.).map(|[x, y]| [
            if clip.text_box_width.is_none() { x } else { 0. },
            if clip.text_box_height.is_none() { y } else { 0. },
        ]),
        fill:true,fill_color:color,align:match clip.style.align {None|Some(super::composition::TextAlignment::Left)=>0,Some(super::composition::TextAlignment::Center)=>1,Some(super::composition::TextAlignment::Right)=>2},
        stroke:clip.style.stroke_width.round()>0.,stroke_width:clip.style.stroke_width.round()as f32,stroke_color:rgba(clip.style.stroke),stroke_type:0,
        background:clip.style.background.alpha>0.,background_color:rgba(clip.style.background),background_opacity:100.,background_size:0.,background_radius:clip.style.background_radius.unwrap_or_default(),
        shadow:shadow_color[3]>0.,shadow_color,shadow_opacity:100.,shadow_distance:dx.hypot(dy),shadow_angle:dy.atan2(dx).to_degrees()+90.,shadow_size:0.,shadow_blur,
        position:Some([canvas.0 as f64/2.,canvas.1 as f64/2.]),..Default::default()
    };
    if clip.karaoke.iter().any(|span| span.mode == super::composition::SubtitleKaraokeMode::Step && local < span.start_ticks) {
        use super::{composition::SubtitleKaraokeMode, graphic_text_layout::GraphicCharStyle, graphic_text_runs::CharacterStyleRun};
        let mut offset = 0;
        let mut boundaries = vec![0];
        for character in clip.text.chars() {
            offset += character.len_utf16();
            boundaries.push(offset);
        }
        parameters.runs = clip.karaoke.iter()
            .filter(|span| span.mode == SubtitleKaraokeMode::Step && local < span.start_ticks)
            .filter_map(|span| Some(CharacterStyleRun {
                start: boundaries.binary_search(&span.start).ok()?,
                end: boundaries.binary_search(&span.end).ok()?,
                style: GraphicCharStyle {
                    fill: Some(span.secondary_color.map(rgba).unwrap_or([0., 1., 1., 1.])),
                    ..Default::default()
                },
            }))
            .collect();
    }
    // Anchor against the complete multiline layout before the full-canvas clip transform.
    let (layer,_)=parameters.evaluate(true,canvas)?;
    let bounds=layer.text.block_bounds(super::graphic_text_layout::text_layout(&layer.text).bounds);
    let [anchor_x, anchor_y] = clip.text_anchor.unwrap_or([clip.transform.anchor_x, clip.transform.anchor_y]);
    parameters.anchor=[bounds[0]as f64+anchor_x*(bounds[2]-bounds[0])as f64,bounds[1]as f64+anchor_y*(bounds[3]-bounds[1])as f64];
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
    fn karaoke_step_changes_only_at_the_syllable_start() {
        use super::super::composition::{SubtitleKaraokeMode, SubtitleKaraokeSpan};
        let mut caption = clip();
        caption.karaoke.push(SubtitleKaraokeSpan {
            start: 0, end: 4, start_ticks: 500_000, end_ticks: 750_000,
            mode: SubtitleKaraokeMode::Step, secondary_color: None,
        });
        let pixels = |clip: &TextClip, time| render(clip, (64, 48), time, &mut || Ok(()))
            .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap();
        let plain = pixels(&clip(), 1_500_000);
        assert_ne!(pixels(&caption, 1_499_999), plain);
        assert_eq!(pixels(&caption, 1_500_000), plain);
        assert_eq!(pixels(&caption, 1_750_000), plain);
    }
    #[test]
    fn ordinary_text_has_visible_style_motion_opacity_and_half_open_admission() {
        let base=clip();
        let pixels=|clip:&TextClip|render(clip,(64,48),1500000,&mut||Ok(())).unwrap().unwrap().to_surface(&mut||Ok(())).unwrap();
        let original=pixels(&base);
        assert!(original.rgba.chunks_exact(4).any(|p|p[3]>0));
        for variant in 0..6 {
            let mut changed=base.clone();
            match variant {
                0=>changed.style.background=Rgba{red:0.,green:0.5,blue:0.,alpha:0.8},
                1=>{changed.style.stroke_width=2.;changed.style.stroke.alpha=1.;},
                2=>changed.style.shadow.alpha=1.,
                3=>changed.opacity=super::super::composition::AnimatableValue::Constant{value:0.25},
                4=>changed.transform.x=super::super::composition::AnimatableValue::Constant{value:10.},
                _=>changed.style.tracking=Some(3.),
            }
            assert_ne!(pixels(&changed),original,"style variant {variant}");
        }
        assert!(render(&base,(64,48),999999,&mut||Ok(())).unwrap().is_none());
        assert!(render(&base,(64,48),3000000,&mut||Ok(())).unwrap().is_none());
        let mut disabled=base;disabled.enabled=false;
        assert!(render(&disabled,(64,48),1500000,&mut||Ok(())).unwrap().is_none());
        assert!(render(&clip(),(64,48),1500000,&mut||anyhow::bail!("cancel")).is_err());
    }

    #[test]
    fn caption_box_height_expands_the_background_without_clipping_lines() {
        let mut caption = clip();
        caption.text = "H\nH".into();
        caption.style.background = Rgba { red: 0., green: 1., blue: 0., alpha: 1. };
        let pixels = |clip: &TextClip| {
            render(clip, (64, 64), 1500000, &mut || Ok(()))
                .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap().rgba
        };
        let original = pixels(&caption);
        caption.text_box_height = Some(1.);
        assert_eq!(pixels(&caption), original);
        caption.text_box_height = Some(48.);
        let expanded = pixels(&caption);
        let rows = expanded.chunks_exact(64 * 4)
            .filter(|row| row.chunks_exact(4).any(|pixel| pixel[3] > 0 && pixel[1] > pixel[0]))
            .count();
        assert!((48..=49).contains(&rows), "visible background rows: {rows}");
        let glyphs = |frame: &[u8]| frame.chunks_exact(4).map(|pixel| u32::from(pixel[0])).sum::<u32>();
        assert!(glyphs(&original) > 0);
        assert_eq!(glyphs(&expanded), glyphs(&original));
        caption.style.background_radius = Some(4.);
        let rounded = pixels(&caption);
        assert!(expanded.chunks_exact(4).zip(rounded.chunks_exact(4))
            .any(|(square, rounded)| square[3] > 0 && rounded[3] == 0));
        assert_eq!(glyphs(&rounded), glyphs(&expanded));
    }

    #[test]
    fn caption_uses_the_current_projects_embedded_font_without_leaking() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use filmcraft_text::fonts::{INTER_BOLD, INTER_REGULAR};
        use super::super::concat_project_fonts::{FontResource, Scope};
        let mut caption = clip();
        caption.style.font_family = "Inter".into();
        let pixels = || render(&caption, (64, 64), 1500000, &mut || Ok(()))
            .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap().rgba;
        let regular = pixels();
        let resource = |data: &[u8]| FontResource { family: "Inter".into(), data: STANDARD.encode(data) };
        let bold_project = Scope::enter(&[resource(INTER_BOLD)]).unwrap();
        let bold = pixels();
        assert_ne!(bold, regular);
        {
            let _regular_project = Scope::enter(&[resource(INTER_REGULAR)]).unwrap();
            assert_eq!(pixels(), regular);
        }
        assert_eq!(pixels(), bold);
        drop(bold_project);
        assert_eq!(pixels(), regular);
    }

    #[test]
    fn caption_numeric_font_weight_selects_the_nearest_face() {
        let mut caption = clip();
        caption.style.font_family = "Inter".into();
        let pixels = |clip: &TextClip| {
            render(clip, (64, 64), 1500000, &mut || Ok(()))
                .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap().rgba
        };
        let regular = pixels(&caption);
        caption.style.font_weight = Some(500.);
        let medium = pixels(&caption);
        assert_ne!(medium, regular);
        caption.style.font_weight = Some(530.);
        assert_eq!(pixels(&caption), medium);
        caption.style.font_weight = Some(580.);
        assert_ne!(pixels(&caption), medium);
        caption.style.font_family = "JetBrains Mono".into();
        let mono = pixels(&caption);
        assert!(mono.chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert_ne!(mono, medium);
    }

    #[test]
    fn caption_can_enable_concat_shadow_without_an_authored_shadow() {
        let mut caption = clip();
        let pixels = |clip: &TextClip| {
            render(clip, (64, 64), 1500000, &mut || Ok(()))
                .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap().rgba
        };
        let original = pixels(&caption);
        caption.style.concat_shadow = true;
        let shaded = pixels(&caption);
        assert!(original.chunks_exact(4).zip(shaded.chunks_exact(4))
            .any(|(base, shadow)| base[3] == 0 && shadow[..3] == [0; 3] && shadow[3] > 0));
        caption.style.concat_shadow = false;
        assert_eq!(pixels(&caption), original);
    }

    #[test]
    fn caption_background_padding_applies_only_to_auto_sized_axes() {
        let mut caption = clip();
        caption.text = "H".into();
        caption.style.background = Rgba { red: 0., green: 1., blue: 0., alpha: 1. };
        let bounds = |clip: &TextClip| {
            let frame = render(clip, (64, 64), 1500000, &mut || Ok(()))
                .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap();
            let mut bounds = [64, 64, 0, 0];
            for (index, pixel) in frame.rgba.chunks_exact(4).enumerate() {
                if pixel[3] > 0 {
                    let (x, y) = (index % 64, index / 64);
                    bounds = [bounds[0].min(x), bounds[1].min(y), bounds[2].max(x), bounds[3].max(y)];
                }
            }
            [bounds[2] - bounds[0] + 1, bounds[3] - bounds[1] + 1]
        };
        let [width, height] = bounds(&caption);
        caption.style.background_padding = Some([4., 6.]);
        assert_eq!(bounds(&caption), [width + 8, height + 12]);
        caption.text_box_width = Some(32.);
        caption.text_box_height = Some(36.);
        let fixed = bounds(&caption);
        caption.style.background_padding = None;
        assert_eq!(bounds(&caption), fixed);
    }

    #[test]
    fn caption_line_height_sets_the_distance_between_glyph_rows() {
        let mut caption = clip();
        caption.text = "H\nH".into();
        for (ratio, pitch) in [(1.5, 18), (2.0, 24)] {
            caption.style.line_height = Some(ratio);
            let frame = render(&caption, (64, 64), 1500000, &mut || Ok(()))
                .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap();
            let mut starts = Vec::new();
            let mut previous = false;
            for (row, pixels) in frame.rgba.chunks_exact(64 * 4).enumerate() {
                let visible = pixels.chunks_exact(4).any(|pixel| pixel[3] > 0);
                if visible && !previous {
                    starts.push(row);
                }
                previous = visible;
            }
            assert_eq!(starts.len(), 2);
            assert_eq!(starts[1] - starts[0], pitch);
        }
    }

    #[test]
    fn ordinary_caption_renders_both_flip_axes() {
        let mut caption = clip();
        caption.style.shadow.alpha = 0.5;
        let pixels = |caption: &TextClip| {
            render(caption, (64, 48), 1500000, &mut || Ok(()))
                .unwrap().unwrap().to_surface(&mut || Ok(())).unwrap().rgba
        };
        let original = pixels(&caption);
        for (flip_h, flip_v) in [(true, false), (false, true), (true, true)] {
            caption.transform.concat_preparation = Some(super::super::concat_picture::Preparation {
                crop: [0.; 4],
                canvas: [64, 48],
                flip_h,
                flip_v,
            });
            let actual = pixels(&caption);
            assert_ne!(actual, original);
            for y in 0..48 {
                for x in 0..64 {
                    let source_x = if flip_h { 63 - x } else { x };
                    let source_y = if flip_v { 47 - y } else { y };
                    let source = (source_y * 64 + source_x) * 4;
                    let target = (y * 64 + x) * 4;
                    assert_eq!(actual[target..target + 4], original[source..source + 4]);
                }
            }
        }
        let source = super::super::group_surface::GroupSurface { width: 64, height: 48, rgba: original };
        for crop in [[0.125; 4], [0.2, 0., 0., 0.]] {
            let preparation = super::super::concat_picture::Preparation {
                crop,
                canvas: [64, 48],
                flip_h: true,
                flip_v: false,
            };
            let expected = preparation.prepare(&source, &mut || Ok(())).unwrap().surface;
            let mut centered = vec![0; 64 * 48 * 4];
            let left = (64 - expected.width) / 2;
            let top = (48 - expected.height) / 2;
            for (row, strip) in expected.rgba.chunks_exact(expected.width * 4).enumerate() {
                let start = ((top + row) * 64 + left) * 4;
                centered[start..start + strip.len()].copy_from_slice(strip);
            }
            for pixel in centered.chunks_exact_mut(4) {
                if pixel[3] == 0 {
                    pixel.fill(0);
                }
            }
            caption.transform.concat_preparation = Some(preparation);
            for (index, (actual, expected)) in pixels(&caption).chunks_exact(4)
                .zip(centered.chunks_exact(4)).enumerate()
            {
                assert_eq!(actual, expected, "crop {crop:?}, pixel {index}");
            }
        }
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
