//! Original graphic-item vector/source-raster paths.
use super::{
    filmcraft_source_frame::{FrameRequest, LayerPixels},
    filmcraft_video_jobs::SourceClipJob,
    transition_kernel_pack::{context::RenderContext, geom::Affine, image::Image},
};
use anyhow::{ensure, Context, Result};
use serde_json::json;
pub(super) fn render(
    job: &SourceClipJob<'_>,
    request: &FrameRequest<'_>,
    scale: f32,
    context: &RenderContext,
) -> Result<LayerPixels> {
    let canvas = job.source_size.context("graphic dimensions missing")?;
    let effects = job.raw_clip["effects"]
        .as_array()
        .context("graphic effects missing")?;
    let layers: Vec<_> = effects
        .iter()
        .filter(|e| matches!(e["effect"].as_str(), Some("graphic_shape" | "graphic_text")))
        .cloned()
        .collect();
    ensure!(layers.len() <= 128, "graphic layer count exceeded");
    let masks = effects
        .iter()
        .find(|e| e["effect"] == "opacity")
        .is_some_and(|e| {
            e["enabled"] != false
                && e["masks"]
                    .as_array()
                    .is_some_and(|m| m.iter().any(|m| m["mode"] != "None"))
        });
    let standard = request.options.effects
        && effects.iter().any(|e| {
            !matches!(
                e["effect"].as_str(),
                Some("motion" | "opacity" | "graphic_shape" | "graphic_text")
            )
        });
    let positioned = !masks && !standard && !request.temporal_prefix;
    let (output, matrix) = if positioned {
        let [a, b, c, d, e, f] = job
            .appearance
            .as_ref()
            .context("graphic Motion missing")?
            .matrix;
        (
            request.canvas,
            Affine::scale(request.options.scale as f64, request.options.scale as f64)
                .then_apply(&Affine { a, b, c, d, e, f }),
        )
    } else {
        (
            (
                ((canvas.0 as f32 * scale).ceil() as usize).max(1),
                ((canvas.1 as f32 * scale).ceil() as usize).max(1),
            ),
            Affine::scale(scale as f64, scale as f64),
        )
    };
    ensure!(
        context.canvas_allowed(output.0, output.1),
        "graphic canvas budget exceeded"
    );
    let image=context.image_work(&[],||{
  let mut checkpoint=||{ensure!(!context.is_cancelled(),"graphic source cancelled");Ok(())};
  let _scratch=context.reserve_scratch(output.0.checked_mul(output.1).and_then(|n|n.checked_mul(128)).and_then(|n|n.checked_add(4*1024*1024)).context("graphic scratch overflow").map_err(|e|e.to_string())?);
  let run=||->Result<Image>{
   if layers.is_empty(){return Ok(Image::new(output.0,output.1))}
   let mut layers=layers;
   let mut uid=layers.iter().filter_map(|e|e["layer"]["uid"].as_u64()).max().unwrap_or(0);
   for layer in &mut layers{if layer["layer"]["uid"].as_u64().unwrap_or(0)==0{uid=uid.checked_add(1).context("graphic uid overflow")?;if !layer["layer"].is_object(){layer["layer"]=json!({});}layer["layer"]["uid"]=json!(uid);}}
   let template=super::graphic_templates::from_original(&json!({"format":"filmcraft.graphicsTemplate","version":1,"name":"Source graphic","canvas":[canvas.0,canvas.1],"layers":layers,"controls":[]}))?;
   let graphic=job.raw_clip.get("graphic").filter(|v|v.is_object());
   let time=|key:&str|->Result<i64>{graphic.and_then(|g|g.get(key)).map_or(Ok(0),|v|v.as_i64().context("invalid graphic time"))};
   let responsive=super::graphic_responsive_time::ResponsiveTime{intro:time("intro")?,outro:time("outro")?,design_in:time("designIn")?,design_duration:time("designDuration")?};
   let source_in=job.raw_clip["source_in"].as_i64().context("graphic source in missing")?;
   let u=job.parameter_ticks.checked_sub(source_in).context("graphic relative clock overflow")?;
   let duration=job.raw_clip["duration"].as_i64().context("graphic duration missing")?;
   let speed=job.raw_clip["speed"].as_f64().context("graphic speed missing")?.abs().max(1e-9);
   let d=(duration as f64*speed).round();ensure!(d.is_finite()&&d<i64::MAX as f64,"graphic duration overflow");
   let at=if responsive.active(){responsive.remap(u,d as i64)?}else{job.parameter_ticks};
   let snapshot=template.graphic.snapshot_original_ticks(canvas,at as i128)?;
   let mut margin=2f64;
   for layer in snapshot.evaluated_layers_signed(canvas,0,&mut checkpoint)?{
    let appearance=match &layer.content{super::graphic_pin_layers::EvaluatedContent::Shape(s)=>&s.appearance,super::graphic_pin_layers::EvaluatedContent::Text(s)=>&s.appearance};
    if let Some(shadow)=&appearance.shadow{
     let prefix_norm=(matrix.a.abs()+matrix.b.abs()+matrix.c.abs()+matrix.d.abs()).max(1.);
     let layer_norm=layer.matrix[..4].iter().map(|v|v.abs()).sum::<f64>().max(1.);
     margin=margin.max((shadow.blur.abs()as f64+shadow.size.abs()as f64+shadow.offset.0.abs()as f64+shadow.offset.1.abs()as f64)*prefix_norm*layer_norm+4.);
    }
   }
   let raster_bytes=(output.0 as f64+2.*margin)*(output.1 as f64+2.*margin)*128.;
   ensure!(raster_bytes.is_finite()&&raster_bytes<usize::MAX as f64,"graphic raster scratch overflow");
   let _raster=context.reserve_scratch(raster_bytes.ceil()as usize);
   let roll:super::graphic_roll::Roll=graphic.and_then(|g|g.get("roll")).cloned().map(serde_json::from_value).transpose()?.unwrap_or_default();
   let frame=snapshot.render_with_roll(canvas,(output.0 as u32,output.1 as u32),0,[matrix.a,matrix.b,matrix.c,matrix.d,matrix.e,matrix.f],Some((&roll,u,d as i64)),&mut checkpoint)?;
   let mut image=Image::new(output.0,output.1);for(dst,src)in image.px.chunks_exact_mut(4).zip(frame.pixels()){dst.copy_from_slice(src);}Ok(image)
  };run().map_err(|e|e.to_string())
 }).map_err(anyhow::Error::msg)?;
    Ok(if positioned {
        LayerPixels::Positioned(image)
    } else {
        LayerPixels::Source(image)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc};
    #[test]
    fn original_graphic_source_vectors_effects_masks_motion_pins_and_roll() {
        let fixture: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/filmcraft-source-graphic.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 648);
        let context = RenderContext::new_with_budget(
            Arc::new(AtomicBool::new(false)),
            2,
            8192,
            64 * 1024 * 1024,
        )
        .unwrap();
        for (i, c) in cases.iter().enumerate() {
            let doc = super::super::filmcraft_project_file::SourceProjectDocument::read(
                &serde_json::to_vec(&c["document"]).unwrap(),
            )
            .unwrap();
            let mut provider =
                super::super::filmcraft_generator_source_provider::GeneratorSourceProvider;
            let image = super::super::filmcraft_source_frame::render(
                &context,
                &doc,
                6,
                c["at"].as_i64().unwrap(),
                super::super::filmcraft_source_frame::FrameOptions {
                    scale: c["scale"].as_f64().unwrap() as f32,
                    working_output: true,
                    ..Default::default()
                },
                &mut provider,
            )
            .unwrap_or_else(|e| panic!("graphic source {i}: {e}"));
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(image.px.len(), expected.len());
            if let Some((k, (a, b))) = image
                .px
                .iter()
                .map(|v| v.to_bits())
                .zip(expected)
                .enumerate()
                .find(|(_, (a, b))| a != b)
            {
                panic!("graphic source {i} channel{k}: {a} != {b}; profile{} path{} motion{} meta{} scale{}",c["profile"],c["path"],c["motion"],c["meta"],c["scale"]);
            }
            let output =
                super::super::filmcraft_source_pixels::encode_srgb(&context, &image).unwrap();
            let expected: Vec<u8> = serde_json::from_value(c["rgba"].clone()).unwrap();
            assert_eq!(output.surface().rgba, expected, "graphic display {i}");
            drop(output);
            drop(image);
            assert_eq!(context.pixel_bytes_in_use(), 0);
        }
    }
}
