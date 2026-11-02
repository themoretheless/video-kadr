//! Ephemeral learned identity recovery. Source/mask ownership is established by
//! spherical_tracking before entry; every input byte is committed before inference.
use crate::{domain::video_region::Attachment, state::AppState};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
pub(super) struct Recovery {
    pub masks: Vec<Option<Vec<u8>>>,
    pub states: Vec<String>,
}
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) async fn recover(
    state: &AppState,
    actor: &str,
    region: &Attachment,
    times: &[u64],
    lease: &super::concat_video_region_cache::Lease,
    spherical: Option<&crate::domain::spherical_video::SphericalVideo>,
    parameter_start_us: u64,
    cancel: &CancellationToken,
) -> Result<Recovery> {
    let owned = cancel.child_token();
    let _guard = owned.clone().drop_guard();
    let operation = recover_inner(
        state,
        actor,
        region,
        times,
        lease,
        spherical,
        parameter_start_us,
        &owned,
    );
    tokio::select! {
        biased;
        _ = owned.cancelled() => anyhow::bail!("semantic identity analysis cancelled"),
        result = tokio::time::timeout(Duration::from_secs(7200), operation) => result.context("semantic identity analysis exceeded its two-hour total budget")?,
    }
}
fn neural_budget(samples: usize, exemplars: usize) -> Result<Duration> {
    ensure!(
        (2..=60).contains(&samples) && exemplars <= 12 && exemplars < samples,
        "invalid bounded neural identity workload"
    );
    // Measured host inference is about20s/sample; include gallery inference,
    // 2x per-sample headroom and explicit model-startup allowance. Never trim.
    Ok(Duration::from_secs(120 + 40 * (samples + exemplars) as u64))
}
async fn recover_inner(
    state: &AppState,
    actor: &str,
    region: &Attachment,
    times: &[u64],
    lease: &super::concat_video_region_cache::Lease,
    spherical: Option<&crate::domain::spherical_video::SphericalVideo>,
    parameter_start_us: u64,
    cancel: &CancellationToken,
) -> Result<Recovery> {
    let (config, _model) = super::sam_models::config_for_owner(state, actor)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    ensure!(
        times.len() >= 2 && times.len() <= 60,
        "invalid neural reidentification sample count"
    );
    crate::services::local_sam_reidentify::require_capability(&state.process_runtime, &config)
        .await?;
    let source = super::cutout_jobs::source(state, &region.source_id, actor).await?;
    let original =
        crate::project_archive::fingerprint_file_with_limit(&source, 64 * 1024 * 1024 * 1024)
            .await?;
    ensure!(
        original.1 == region.source_hash,
        "semantic original content changed"
    );
    // A seed must come from a measured, verified saved SAM plane. Never seed an
    // occluded/ambiguous frame from a fabricated bounding box.
    let mut seed_index = None;
    let mut seed_bytes = vec![];
    let mut saved_views = vec![];
    for (i, &time) in times.iter().enumerate() {
        let ordinal = lease.reader.ordinal(time)? as u64;
        if region.forward_lost_at.is_some_and(|at| ordinal >= at)
            || region.backward_lost_at.is_some_and(|at| ordinal <= at)
        {
            continue;
        }
        let mask = lease.reader.read(&region.source_hash, time, cancel)?;
        if mask.probabilities.iter().filter(|v| **v >= 230).count() >= 4 {
            if seed_index.is_none() {
                seed_index = Some(i);
                seed_bytes = mask.probabilities.clone();
            } else if saved_views.len() < 12 {
                saved_views.push((i, mask.probabilities.clone()));
            }
        }
    }
    let seed_index =
        seed_index.context("selected interval has no verified visible SAM identity seed")?;
    let root = state.storage.join("spherical-reidentification-stages");
    tokio::fs::create_dir_all(&root).await?;
    let folder = root.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir(&folder).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let stage = Stage(folder);
    let raw = stage.0.join("frames.rgba");
    let seed = stage.0.join("seed.gray");
    let masks = stage.0.join("identity.masks");
    let scale = (512. / f64::from(region.width))
        .min(256. / f64::from(region.height))
        .min(1.);
    let dual =
        spherical.is_some_and(|p| p.input == crate::domain::spherical_video::Input::DualFisheye);
    let width = if dual {
        512
    } else {
        (f64::from(region.width) * scale).round().max(1.) as usize
    };
    let height = if dual {
        256
    } else {
        (f64::from(region.height) * scale).round().max(1.) as usize
    };
    let body = spherical.filter(|_| dual).map(|p| {
        let mut p = p.clone();
        p.output = crate::domain::spherical_video::Output::Equirectangular;
        p.stabilization_mode = crate::domain::spherical_video::StabilizationMode::Disabled;
        p.pan = 0.;
        p.tilt = 0.;
        p.roll = 0.;
        p
    });
    let selected = times.to_vec();
    let path = source.clone();
    let destination = raw.clone();
    let token = cancel.clone();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(cancel.is_cancelled()));
    let monitor = flag.clone();
    let observe = cancel.clone();
    let watcher = tokio::spawn(async move {
        observe.cancelled().await;
        monitor.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let raw_result = tokio::task::spawn_blocking(
        move || -> Result<(String, Vec<u8>, Vec<(usize, Vec<u8>)>)> {
            let mut projected_seed = seed_bytes;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options.open(&destination)?;
            let mut hash = Sha256::new();
            for (index, time) in selected.into_iter().enumerate() {
                ensure!(!token.is_cancelled(), "neural frame preparation cancelled");
                let mut frame = crate::domain::group_media::decode_source_frame_policy_layout(
                    std::path::Path::new("ffmpeg"),
                    &path,
                    crate::domain::video_clock::FrameTime::Microseconds(time),
                    width,
                    height,
                    Duration::from_secs(30),
                    &flag,
                    true,
                    body.as_ref().is_some_and(|p| {
                        p.source_layout
                            == crate::domain::spherical_video::SourceLayout::DualVideoStreams
                    }),
                )?;
                if let Some(parameters) = &body {
                    let time = (time - parameter_start_us) as f64 / 1_000_000.;
                    let mut checkpoint = || {
                        ensure!(
                            !token.is_cancelled(),
                            "neural panorama preparation cancelled"
                        );
                        Ok(())
                    };
                    for (_, mask) in saved_views.iter_mut().filter(|(i, _)| *i == index) {
                        *mask = parameters.project_tracking_mask(
                            mask,
                            256,
                            256,
                            time,
                            &frame,
                            &mut checkpoint,
                        )?;
                    }
                    if index == seed_index {
                        projected_seed = parameters.project_tracking_mask(
                            &projected_seed,
                            256,
                            256,
                            time,
                            &frame,
                            &mut checkpoint,
                        )?;
                    }
                    let mut linear =
                        crate::domain::linear_color_frame::LinearColorFrame::from_surface(
                            &frame,
                            &mut checkpoint,
                        )?;
                    parameters.apply_linear(&mut linear, time, &mut checkpoint)?;
                    frame = linear.to_surface(&mut checkpoint)?;
                }
                hash.update(&frame.rgba);
                output.write_all(&frame.rgba)?;
            }
            output.sync_all()?;
            Ok((
                format!("{:x}", hash.finalize()),
                projected_seed,
                saved_views,
            ))
        },
    )
    .await;
    watcher.abort();
    let (raw_hash, seed_bytes, saved_views) = raw_result??;
    let seed_hash = format!("{:x}", Sha256::digest(&seed_bytes));
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(&seed).await?;
    use tokio::io::AsyncWriteExt;
    file.write_all(&seed_bytes).await?;
    file.sync_all().await?;
    drop(file);
    let mut exemplars = vec![];
    for (index, bytes) in saved_views {
        let mask = stage.0.join(format!("view-{index}.gray"));
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(&mask).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        exemplars.push(crate::services::local_sam_reidentify::Exemplar {
            frame: index as u64,
            mask,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        });
    }
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let metadata = crate::services::local_sam_reidentify::reidentify(
        &state.process_runtime,
        &config,
        crate::services::local_sam_reidentify::Input {
            source: &raw,
            source_hash: &raw_hash,
            seed: &seed,
            seed_hash: &seed_hash,
            width: width as u32,
            height: height as u32,
            frames: times.len() as u64,
            seed_frame: seed_index as u64,
            exemplars: &exemplars,
        },
        &masks,
        cancel,
        neural_budget(times.len(), exemplars.len())?,
        &tx,
    )
    .await?;
    ensure!(
        crate::project_archive::fingerprint_file_with_limit(&source, 64 * 1024 * 1024 * 1024)
            .await?
            == original,
        "semantic original changed during neural identity matching"
    );
    let mut file = std::fs::File::open(&masks)?;
    let mut planes = Vec::with_capacity(times.len());
    for (i, label) in metadata.states.iter().enumerate() {
        ensure!(!cancel.is_cancelled(), "neural identity request cancelled");
        if matches!(label.as_str(), "seed" | "tracked" | "reacquired") {
            let mut plane = vec![0; 65536];
            file.seek(SeekFrom::Start(i as u64 * 65536))?;
            file.read_exact(&mut plane)?;
            planes.push(Some(plane));
        } else {
            planes.push(None);
        }
    }
    Ok(Recovery {
        masks: planes,
        states: metadata.states,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_advertised_neural_workload_fits_cancellable_adapter_deadline() {
        let full = neural_budget(60, 12).unwrap();
        assert_eq!(full, Duration::from_secs(3000));
        assert!(full > Duration::from_secs(600));
        assert!(full <= Duration::from_secs(3600));
        assert!(full > Duration::from_secs((60 + 12) * 20));
        assert!(neural_budget(61, 12).is_err());
        assert!(neural_budget(60, 13).is_err());
        assert!(neural_budget(2, 2).is_err());
    }
}
