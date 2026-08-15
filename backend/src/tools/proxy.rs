//! FFmpeg adapter for the proxy-generation port.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::analysis::proxy::{ProxyCodec, ProxyEncoder, ProxyProfile, SourceIdentity};
use crate::process_control::ProcessRuntime;

use super::{run_ffmpeg, Done};

#[derive(Debug, Clone)]
pub struct FfmpegProxyEncoder {
    runtime: ProcessRuntime,
}

impl FfmpegProxyEncoder {
    pub fn new(runtime: ProcessRuntime) -> Self {
        Self { runtime }
    }
}

#[axum::async_trait]
impl ProxyEncoder for FfmpegProxyEncoder {
    async fn generate(
        &self,
        source: &SourceIdentity,
        profile: &ProxyProfile,
        staging_path: &Path,
        progress: &mpsc::UnboundedSender<f64>,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let args = build_proxy_args(&source.original_path, staging_path, profile);
        match run_ffmpeg(
            &self.runtime,
            &args,
            source.duration_seconds,
            progress,
            cancellation,
            Duration::from_secs(2 * 60 * 60),
        )
        .await?
        {
            Done::Completed => Ok(()),
            Done::Cancelled => Err(anyhow!("proxy generation cancelled")),
        }
    }
}

pub fn build_proxy_args(input: &Path, output: &Path, profile: &ProxyProfile) -> Vec<String> {
    let mut args = vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        format!("scale='min({},iw)':-2", profile.max_width),
    ];
    match profile.codec {
        ProxyCodec::H264 => {
            args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "veryfast".into(),
                "-crf".into(),
                profile.quality.to_string(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-movflags".into(),
                "+faststart".into(),
            ]);
        }
        ProxyCodec::ProresProxy => {
            args.extend([
                "-c:v".into(),
                "prores_ks".into(),
                "-profile:v".into(),
                "0".into(),
                "-pix_fmt".into(),
                "yuv422p10le".into(),
            ]);
        }
    }
    if profile.include_audio {
        args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "96k".into()]);
    } else {
        args.push("-an".into());
    }
    args.push(output.to_string_lossy().into_owned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_arguments_are_bounded_and_do_not_touch_source() {
        let args = build_proxy_args(
            Path::new("/source.mp4"),
            Path::new("/staging/proxy.mp4"),
            &ProxyProfile::default(),
        );
        assert_eq!(args.first().map(String::as_str), Some("-y"));
        assert!(args.contains(&"libx264".into()));
        assert!(args.contains(&"scale='min(960,iw)':-2".into()));
        assert_eq!(args.last().map(String::as_str), Some("/staging/proxy.mp4"));
    }
}
