//! Optional streaming packaging boundary after encode.
//!
//! Ordinary file export does not import or depend on this module. HLS and DASH
//! adapters consume an already-encoded input and publish a verified bundle.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::artifacts::{safe_relative_path, ArtifactFile};
use crate::domain::artifact_graph::Fingerprint;
use crate::runtime::cpu_pool::CpuPool;

const OUTPUT_BUNDLE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageFormat {
    Hls,
    Dash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "policy", content = "expiresAt")]
pub enum RetentionPolicy {
    Persistent,
    Ephemeral(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputBundle {
    pub schema_version: u32,
    pub format: PackageFormat,
    pub source_fingerprint: Fingerprint,
    pub manifest: ArtifactFile,
    pub init_segments: Vec<ArtifactFile>,
    pub segments: Vec<ArtifactFile>,
    pub retention: RetentionPolicy,
}

impl OutputBundle {
    #[allow(clippy::too_many_arguments)]
    pub async fn inspect(
        root: &Path,
        format: PackageFormat,
        source_fingerprint: Fingerprint,
        manifest: &Path,
        init_segments: &[PathBuf],
        segments: &[PathBuf],
        retention: RetentionPolicy,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<Self> {
        if init_segments.is_empty() || segments.is_empty() {
            return Err(anyhow!("streaming bundle needs init and media segments"));
        }
        let manifest = ArtifactFile::inspect(
            root,
            &safe_relative_path(manifest)?,
            pool,
            cancellation.child_token(),
        )
        .await?;
        let mut inspected_init = Vec::with_capacity(init_segments.len());
        for path in init_segments {
            inspected_init
                .push(ArtifactFile::inspect(root, path, pool, cancellation.child_token()).await?);
        }
        let mut inspected_segments = Vec::with_capacity(segments.len());
        for path in segments {
            inspected_segments
                .push(ArtifactFile::inspect(root, path, pool, cancellation.child_token()).await?);
        }
        let bundle = Self {
            schema_version: OUTPUT_BUNDLE_SCHEMA_VERSION,
            format,
            source_fingerprint,
            manifest,
            init_segments: inspected_init,
            segments: inspected_segments,
            retention,
        };
        bundle.validate_paths()?;
        Ok(bundle)
    }

    pub async fn verify(
        &self,
        root: &Path,
        pool: &CpuPool,
        cancellation: CancellationToken,
    ) -> Result<()> {
        if self.schema_version != OUTPUT_BUNDLE_SCHEMA_VERSION {
            return Err(anyhow!("unsupported output bundle schema"));
        }
        self.validate_paths()?;
        self.manifest
            .verify(root, pool, cancellation.child_token())
            .await?;
        for file in self.init_segments.iter().chain(&self.segments) {
            file.verify(root, pool, cancellation.child_token()).await?;
        }
        Ok(())
    }

    fn validate_paths(&self) -> Result<()> {
        if self.init_segments.is_empty() || self.segments.is_empty() {
            return Err(anyhow!("streaming bundle needs init and media segments"));
        }
        let mut paths = BTreeSet::new();
        for file in std::iter::once(&self.manifest)
            .chain(&self.init_segments)
            .chain(&self.segments)
        {
            crate::artifacts::safe_relative_token(&file.path)?;
            if !paths.insert(&file.path) {
                return Err(anyhow!("duplicate output bundle path: {}", file.path));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagingCommand {
    pub program: &'static str,
    pub args: Vec<String>,
    pub expected_manifest: PathBuf,
    pub expected_init_segments: Vec<PathBuf>,
    pub segment_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedInput {
    pub path: PathBuf,
    pub has_audio: bool,
}

pub trait PackagingAdapter: Send + Sync {
    fn format(&self) -> PackageFormat;
    fn command(
        &self,
        encoded_input: &EncodedInput,
        output_directory: &Path,
    ) -> Result<PackagingCommand>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ShakaHlsAdapter;

impl PackagingAdapter for ShakaHlsAdapter {
    fn format(&self) -> PackageFormat {
        PackageFormat::Hls
    }

    fn command(
        &self,
        encoded_input: &EncodedInput,
        output_directory: &Path,
    ) -> Result<PackagingCommand> {
        safe_descriptor_path(&encoded_input.path)?;
        safe_descriptor_path(output_directory)?;
        let video_init = output_directory.join("video-init.mp4");
        let video_template = output_directory.join("video-$Number$.m4s");
        let playlist = output_directory.join("video.m3u8");
        let manifest = output_directory.join("master.m3u8");
        let video_descriptor = format!(
            "input={},stream=video,init_segment={},segment_template={},playlist_name={}",
            encoded_input.path.display(),
            video_init.display(),
            video_template.display(),
            playlist.display()
        );
        let mut args = vec![video_descriptor];
        let mut init_segments = vec![video_init];
        if encoded_input.has_audio {
            let audio_init = output_directory.join("audio-init.mp4");
            let audio_template = output_directory.join("audio-$Number$.m4s");
            let audio_playlist = output_directory.join("audio.m3u8");
            args.push(format!(
                "input={},stream=audio,init_segment={},segment_template={},playlist_name={},hls_group_id=audio,hls_name=main",
                encoded_input.path.display(),
                audio_init.display(),
                audio_template.display(),
                audio_playlist.display()
            ));
            init_segments.push(audio_init);
        }
        args.extend([
            "--hls_master_playlist_output".into(),
            manifest.to_string_lossy().into_owned(),
        ]);
        Ok(PackagingCommand {
            program: "packager",
            args,
            expected_manifest: manifest,
            expected_init_segments: init_segments,
            segment_directory: output_directory.to_path_buf(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ShakaDashAdapter;

impl PackagingAdapter for ShakaDashAdapter {
    fn format(&self) -> PackageFormat {
        PackageFormat::Dash
    }

    fn command(
        &self,
        encoded_input: &EncodedInput,
        output_directory: &Path,
    ) -> Result<PackagingCommand> {
        safe_descriptor_path(&encoded_input.path)?;
        safe_descriptor_path(output_directory)?;
        let video_init = output_directory.join("video-init.mp4");
        let video_template = output_directory.join("video-$Number$.m4s");
        let manifest = output_directory.join("manifest.mpd");
        let video_descriptor = format!(
            "input={},stream=video,init_segment={},segment_template={}",
            encoded_input.path.display(),
            video_init.display(),
            video_template.display()
        );
        let mut args = vec![video_descriptor];
        let mut init_segments = vec![video_init];
        if encoded_input.has_audio {
            let audio_init = output_directory.join("audio-init.mp4");
            let audio_template = output_directory.join("audio-$Number$.m4s");
            args.push(format!(
                "input={},stream=audio,init_segment={},segment_template={}",
                encoded_input.path.display(),
                audio_init.display(),
                audio_template.display()
            ));
            init_segments.push(audio_init);
        }
        args.extend([
            "--mpd_output".into(),
            manifest.to_string_lossy().into_owned(),
        ]);
        Ok(PackagingCommand {
            program: "packager",
            args,
            expected_manifest: manifest,
            expected_init_segments: init_segments,
            segment_directory: output_directory.to_path_buf(),
        })
    }
}

fn safe_descriptor_path(path: &Path) -> Result<()> {
    let token = path
        .to_str()
        .ok_or_else(|| anyhow!("packager paths must be UTF-8"))?;
    if token
        .chars()
        .any(|character| matches!(character, ',' | '\n' | '\r'))
    {
        return Err(anyhow!("packager path contains descriptor separators"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::runtime::cpu_pool::CpuPoolConfig;

    use super::*;

    fn pool() -> CpuPool {
        CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 4,
        })
        .unwrap()
    }

    #[test]
    fn hls_and_dash_are_isolated_command_adapters() {
        let input = EncodedInput {
            path: PathBuf::from("/encoded/input.mp4"),
            has_audio: true,
        };
        let output = Path::new("/package/out");
        let hls = ShakaHlsAdapter.command(&input, output).unwrap();
        let dash = ShakaDashAdapter.command(&input, output).unwrap();
        assert_eq!(hls.program, "packager");
        assert!(hls.expected_manifest.ends_with("master.m3u8"));
        assert!(dash.expected_manifest.ends_with("manifest.mpd"));
        assert_eq!(hls.expected_init_segments.len(), 2);
        assert!(hls
            .args
            .iter()
            .any(|argument| argument.contains("stream=audio")));
        assert_ne!(hls.args, dash.args);
        assert!(ShakaHlsAdapter
            .command(
                &EncodedInput {
                    path: PathBuf::from("/encoded/bad,input.mp4"),
                    has_audio: false,
                },
                output,
            )
            .is_err());
    }

    #[tokio::test]
    async fn output_bundle_verifies_every_declared_file() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(root.path().join("hls"))
            .await
            .unwrap();
        tokio::fs::write(root.path().join("hls/master.m3u8"), b"manifest")
            .await
            .unwrap();
        tokio::fs::write(root.path().join("hls/init.mp4"), b"init")
            .await
            .unwrap();
        tokio::fs::write(root.path().join("hls/0001.m4s"), b"segment")
            .await
            .unwrap();
        let bundle = OutputBundle::inspect(
            root.path(),
            PackageFormat::Hls,
            Fingerprint::digest(b"encoded"),
            Path::new("hls/master.m3u8"),
            &[PathBuf::from("hls/init.mp4")],
            &[PathBuf::from("hls/0001.m4s")],
            RetentionPolicy::Persistent,
            &pool(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        bundle
            .verify(root.path(), &pool(), CancellationToken::new())
            .await
            .unwrap();
        tokio::fs::write(root.path().join("hls/0001.m4s"), b"corrupt")
            .await
            .unwrap();
        assert!(bundle
            .verify(root.path(), &pool(), CancellationToken::new())
            .await
            .is_err());
    }
}
