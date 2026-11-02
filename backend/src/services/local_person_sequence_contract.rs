//! Pure bounded RVM sequence layout and independent RGBA/mask validation.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub mask_destination: Option<&'a Path>,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub mask_bytes: u64,
    pub blank_frames: u64,
    pub mask_hash: String,
    pub frames: u64,
    pub bytes: u64,
    pub source_hash: String,
    pub model_hash: String,
    pub runtime_hash: String,
    pub output_hash: String,
}
pub(crate) fn layout(w: u32, h: u32, frames: u64) -> Result<(u32, u32, u64, u64)> {
    ensure!(
        w > 0
            && h > 0
            && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024
            && frames > 0
            && frames <= 360000,
        "invalid person sequence layout"
    );
    let scale = 512. / f64::from(w.max(h));
    let round = |edge: u32| (((f64::from(edge) * scale / 16.).round() as u32) * 16).max(64);
    let (mw, mh) = (round(w), round(h));
    let bytes = (u64::from(w) * u64::from(h) * 4)
        .checked_mul(frames)
        .filter(|n| *n <= MAX_BYTES)
        .context("person raw staging budget")?;
    let masks = (u64::from(mw) * u64::from(mh))
        .checked_mul(frames)
        .filter(|n| *n <= MAX_BYTES)
        .context("person mask staging budget")?;
    Ok((mw, mh, bytes, masks))
}
pub(crate) fn layout_for(model:&str,w:u32,h:u32,frames:u64)->Result<(u32,u32,u64,u64)>{
 if model=="88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828"{return layout(w,h,frames)}
 ensure!(w>0&&h>0&&u64::from(w)*u64::from(h)<=64*1024*1024&&frames>0&&frames<=360000,"invalid mask sequence layout");
 let edge=match model{"3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad"=>256u32,"60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a"=>512u32,_=>anyhow::bail!("unsupported sequence mask family")};
 let bytes=(u64::from(w)*u64::from(h)*4).checked_mul(frames).filter(|n|*n<=MAX_BYTES).context("raw mask sequence staging budget")?;
 let masks=u64::from(edge).pow(2).checked_mul(frames).filter(|n|*n<=MAX_BYTES).context("mask sequence staging budget")?;Ok((edge,edge,bytes,masks))
}

fn sample(mask: &[u8], width: u32, height: u32, x: u32, y: u32, w: u32, h: u32) -> f32 {
    let u = (x as f32 + 0.5) / w as f32;
    let v = (y as f32 + 0.5) / h as f32;
    let x = u.clamp(0., 1.) * width as f32 - 0.5;
    let y = v.clamp(0., 1.) * height as f32 - 0.5;
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let at = |x: i64, y: i64| {
        f32::from(
            mask[y.clamp(0, i64::from(height) - 1) as usize * width as usize
                + x.clamp(0, i64::from(width) - 1) as usize],
        )
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let upper = at(x0, y0) * (1. - fx) + at(x0 + 1, y0) * fx;
    let lower = at(x0, y0 + 1) * (1. - fx) + at(x0 + 1, y0 + 1) * fx;
    (upper * (1. - fy) + lower * fy) / 255.
}
pub(crate) async fn verify_frames(
    input: &Input<'_>,
    result: &Path,
    masks: &Path,
    mw: u32,
    mh: u32,
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<(String, String, u64)> {
    verify_frames_with_fallbacks(input, result, masks, mw, mh, cancel, deadline, None).await
}
pub(crate) async fn verify_frames_with_fallbacks(
    input: &Input<'_>,
    result: &Path,
    masks: &Path,
    mw: u32,
    mh: u32,
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
    fallbacks: Option<&[bool]>,
) -> Result<(String, String, u64)> {
    ensure!(
        fallbacks.is_none_or(|v| v.len() == input.frames as usize),
        "invalid fallback frame count"
    );
    let mut source = tokio::fs::File::open(input.source).await?;
    let mut output = tokio::fs::File::open(result).await?;
    let mut mask_file = tokio::fs::File::open(masks).await?;
    let mut source_hash = Sha256::new();
    let mut output_hash = Sha256::new();
    let mut mask_hash = Sha256::new();
    let mut blank_frames = 0;
    let pixels = u64::from(input.width) * u64::from(input.height);
    let mut mask = vec![0u8; mw as usize * mh as usize];
    let mut before = vec![0u8; 65536];
    let mut after = vec![0u8; 65536];
    for ordinal in 0..input.frames {
        ensure!(
            !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
            "person validation cancelled or expired"
        );
        mask_file.read_exact(&mut mask).await?;
        mask_hash.update(&mask);
        let blank = fallbacks
            .map(|v| v[ordinal as usize])
            .unwrap_or_else(|| mask.iter().all(|v| *v < 10));
        if blank {
            blank_frames += 1;
        }
        let mut position = 0;
        while position < pixels {
            ensure!(
                !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
                "person validation cancelled or expired"
            );
            let count = (pixels - position).min(16384) as usize;
            let bytes = count * 4;
            source.read_exact(&mut before[..bytes]).await?;
            output.read_exact(&mut after[..bytes]).await?;
            source_hash.update(&before[..bytes]);
            output_hash.update(&after[..bytes]);
            for i in 0..count {
                let offset = i * 4;
                let pixel = position + i as u64;
                let x = (pixel % u64::from(input.width)) as u32;
                let y = (pixel / u64::from(input.width)) as u32;
                let alpha = if blank {
                    before[offset + 3]
                } else {
                    (f32::from(before[offset + 3])
                        * sample(&mask, mw, mh, x, y, input.width, input.height)
                        + 0.5) as u8
                };
                ensure!(
                    before[offset..offset + 3] == after[offset..offset + 3]
                        && after[offset + 3] == alpha,
                    "person RGBA disagrees with original and learned mask"
                );
            }
            position += count as u64;
        }
    }
    let mut byte = [0u8; 1];
    ensure!(
        source.read(&mut byte).await? == 0
            && output.read(&mut byte).await? == 0
            && mask_file.read(&mut byte).await? == 0,
        "person sequence size mismatch"
    );
    ensure!(
        format!("{:x}", source_hash.finalize()) == input.source_hash,
        "person source bytes changed"
    );
    Ok((
        format!("{:x}", output_hash.finalize()),
        format!("{:x}", mask_hash.finalize()),
        blank_frames,
    ))
}
#[cfg(test)]
mod tests {
    use super::super::local_person::Config;
    use super::*;
    use std::time::Duration;
    #[test]
    fn layout_and_config_contract() {
        assert_eq!(layout(37, 23, 4).unwrap(), (512, 320, 13616, 655360));
        assert_eq!(layout(1, 1, 1).unwrap(), (512, 512, 4, 262144));
        assert!(layout(0, 1, 1).is_err());
        assert!(layout(37, 23, 0).is_err());
        assert!(layout(37, 23, 360001).is_err());
        assert!(layout(4096, 4096, 360000).is_err());
        assert!(Config::from_lookup(&mut |_| None).unwrap().is_none());
        assert!(Config::from_lookup(&mut |key| if key == "PERSON_BIN" {
            Some("/engine".into())
        } else {
            None
        })
        .is_err());
        assert!(Config::from_lookup(&mut |_| Some("relative".into())).is_err());
    }
    #[tokio::test]
    async fn independent_verifier_rejects_wrong_matte_rgb_blank_and_count() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let output = root.path().join("out");
        let mask = root.path().join("mask");
        let original = [10u8, 20, 30, 128].repeat(6);
        std::fs::write(&source, &original).unwrap();
        let hash = format!("{:x}", Sha256::digest(&original));
        let input = Input {
            source: &source,
            source_hash: &hash,
            width: 3,
            height: 2,
            frames: 1,
            mask_destination: None,
        };
        std::fs::write(&mask, vec![128u8; 512 * 336]).unwrap();
        for (pixel, ok) in [
            ([10, 20, 30, 64], true),
            ([10, 20, 30, 63], false),
            ([10, 20, 30, 200], false),
            ([9, 20, 30, 64], false),
        ] {
            std::fs::write(&output, pixel.repeat(6)).unwrap();
            assert_eq!(
                verify_frames(
                    &input,
                    &output,
                    &mask,
                    512,
                    336,
                    &CancellationToken::new(),
                    tokio::time::Instant::now() + Duration::from_secs(5)
                )
                .await
                .is_ok(),
                ok
            );
        }
        std::fs::write(&mask, vec![0u8; 512 * 336]).unwrap();
        std::fs::write(&output, &original).unwrap();
        assert_eq!(
            verify_frames(
                &input,
                &output,
                &mask,
                512,
                336,
                &CancellationToken::new(),
                tokio::time::Instant::now() + Duration::from_secs(5)
            )
            .await
            .unwrap()
            .2,
            1
        );
        // Authored erase-all must not be treated as automatic blank fallback.
        std::fs::write(&output, [10u8, 20, 30, 0].repeat(6)).unwrap();
        assert_eq!(
            verify_frames_with_fallbacks(
                &input,
                &output,
                &mask,
                512,
                336,
                &CancellationToken::new(),
                tokio::time::Instant::now() + Duration::from_secs(5),
                Some(&[false])
            )
            .await
            .unwrap()
            .2,
            0
        );
        std::fs::write(&output, &original).unwrap();
        assert!(verify_frames_with_fallbacks(
            &input,
            &output,
            &mask,
            512,
            336,
            &CancellationToken::new(),
            tokio::time::Instant::now() + Duration::from_secs(5),
            Some(&[false])
        )
        .await
        .is_err());
        std::fs::write(&mask, vec![0u8; 512 * 336 + 1]).unwrap();
        assert!(verify_frames(
            &input,
            &output,
            &mask,
            512,
            336,
            &CancellationToken::new(),
            tokio::time::Instant::now() + Duration::from_secs(5)
        )
        .await
        .is_err());
    }
}
