//! Versioned, bounded, disposable completed-analysis storage. No source pixels or credentials.
use crate::domain::{
    filmcraft_analysis_cache::ReadyPaths,
    warp_stabilizer::{Affine, StabilizerPath},
};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
const LIMIT: usize = 32 * 1024 * 1024;
const TOTAL: u64 = 64 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"VKSTAB01";

pub struct PathStore {
    root: PathBuf,
    gate: Mutex<()>,
}
impl PathStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            gate: Mutex::new(()),
        }
    }
    pub async fn load(self: &Arc<Self>, key: [u8; 32]) -> Result<Option<ReadyPaths>> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = store
                .gate
                .lock()
                .map_err(|_| anyhow::anyhow!("trajectory store lock poisoned"))?;
            let path = store.root.join(name(key));
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            ensure!(
                meta.is_file() && meta.len() <= LIMIT as u64,
                "invalid trajectory file"
            );
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take((LIMIT + 1) as u64)
                .read_to_end(&mut bytes)?;
            Ok(Some(decode(&bytes, key)?))
        })
        .await?
    }
    pub async fn save(self: &Arc<Self>, key: [u8; 32], paths: ReadyPaths) -> Result<()> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = store
                .gate
                .lock()
                .map_err(|_| anyhow::anyhow!("trajectory store lock poisoned"))?;
            if paths.values().all(Option::is_none) {
                return Ok(());
            }
            let bytes = encode(key, &paths)?;
            std::fs::create_dir_all(&store.root)?;
            let temporary = store.root.join(format!("{}.tmp", uuid::Uuid::new_v4()));
            let result = (|| -> Result<()> {
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&temporary)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                drop(file);
                std::fs::rename(&temporary, store.root.join(name(key)))?;
                prune(&store.root)?;
                #[cfg(unix)]
                std::fs::File::open(&store.root)?.sync_all()?;
                Ok(())
            })();
            let _ = std::fs::remove_file(temporary);
            result
        })
        .await?
    }
}
fn name(key: [u8; 32]) -> String {
    format!(
        "{}.trajectory",
        key.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
fn encode(key: [u8; 32], paths: &ReadyPaths) -> Result<Vec<u8>> {
    let paths: Vec<_> = paths
        .iter()
        .filter_map(|(k, p)| p.as_ref().map(|p| (k, p)))
        .collect();
    ensure!(paths.len() <= 128, "too many trajectories");
    let mut b = MAGIC.to_vec();
    b.extend(key);
    b.extend((paths.len() as u64).to_le_bytes());
    for (k, p) in paths {
        for v in [
            k.0,
            k.1,
            k.2,
            k.3 as u64,
            k.4 as u64,
            p.w as u64,
            p.h as u64,
            p.fps.to_bits(),
            p.steps.len() as u64,
        ] {
            b.extend(v.to_le_bytes());
        }
        ensure!(p.steps.len() <= 20_000, "trajectory too long");
        for a in &p.steps {
            for v in [a.a, a.b, a.c, a.d, a.e, a.f] {
                ensure!(v.is_finite(), "nonfinite trajectory");
                b.extend(v.to_bits().to_le_bytes());
            }
        }
        ensure!(b.len() + 32 <= LIMIT, "trajectory snapshot too large");
    }
    let digest = Sha256::digest(&b);
    b.extend(digest);
    Ok(b)
}
fn decode(b: &[u8], key: [u8; 32]) -> Result<ReadyPaths> {
    ensure!(b.len() >= 80 && b.len() <= LIMIT, "invalid trajectory size");
    let end = b.len() - 32;
    ensure!(
        Sha256::digest(&b[..end]).as_slice() == &b[end..],
        "trajectory checksum mismatch"
    );
    ensure!(
        &b[..8] == MAGIC && b[8..40] == key,
        "trajectory version/key mismatch"
    );
    let mut pos = 40;
    let mut number = || -> Result<u64> {
        ensure!(pos + 8 <= end, "truncated trajectory");
        let n = u64::from_le_bytes(b[pos..pos + 8].try_into()?);
        pos += 8;
        Ok(n)
    };
    let count = number()?;
    ensure!(count <= 128, "too many trajectories");
    let mut paths = ReadyPaths::new();
    for _ in 0..count {
        let key = (
            number()?,
            number()?,
            number()?,
            usize::try_from(number()?)?,
            u32::try_from(number()?)?,
        );
        let w = usize::try_from(number()?)?;
        let h = usize::try_from(number()?)?;
        let fps = f64::from_bits(number()?);
        let count = usize::try_from(number()?)?;
        ensure!(
            w > 0
                && h > 0
                && w <= 65536
                && h <= 65536
                && fps.is_finite()
                && fps > 0.
                && count <= 20_000,
            "invalid trajectory dimensions/clock"
        );
        let mut steps = Vec::with_capacity(count);
        for _ in 0..count {
            let mut v = [0.; 6];
            for v in &mut v {
                *v = f64::from_bits(number()?);
                ensure!(v.is_finite(), "nonfinite trajectory");
            }
            steps.push(Affine {
                a: v[0],
                b: v[1],
                c: v[2],
                d: v[3],
                e: v[4],
                f: v[5],
            });
        }
        ensure!(
            paths
                .insert(key, Some(Arc::new(StabilizerPath { w, h, fps, steps })))
                .is_none(),
            "duplicate trajectory"
        );
    }
    ensure!(pos == end, "trailing trajectory data");
    ensure!(
        crate::domain::filmcraft_analysis_cache::ready_bytes(&paths) <= LIMIT,
        "trajectory memory budget exceeded"
    );
    Ok(paths)
}
fn prune(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("trajectory") {
            continue;
        }
        let m = std::fs::symlink_metadata(&path)?;
        if !m.is_file() {
            continue;
        }
        total = total.saturating_add(m.len());
        files.push((m.modified()?, path, m.len()));
    }
    files.sort_by_key(|e| e.0);
    let mut count = files.len();
    for (_, path, len) in files {
        if count <= 64 && total <= TOTAL {
            break;
        }
        std::fs::remove_file(path)?;
        count -= 1;
        total -= len;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn paths() -> ReadyPaths {
        [(
            (1, 2, 3, 4, 0),
            Some(Arc::new(StabilizerPath {
                w: 96,
                h: 64,
                fps: 6.,
                steps: vec![
                    Affine {
                        e: -0.0,
                        f: 0.12345678901234567,
                        ..Affine::IDENTITY
                    };
                    4
                ],
            })),
        )]
        .into_iter()
        .collect()
    }
    #[tokio::test]
    async fn cold_restart_preserves_bits_and_key_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(PathStore::new(dir.path().into()));
        let original = paths();
        store.save([1; 32], original.clone()).await.unwrap();
        drop(store);
        let store = Arc::new(PathStore::new(dir.path().into()));
        let read = store.load([1; 32]).await.unwrap().unwrap();
        let a = read.values().next().unwrap().as_ref().unwrap();
        let b = original.values().next().unwrap().as_ref().unwrap();
        assert_eq!(a.steps[0].e.to_bits(), b.steps[0].e.to_bits());
        assert_eq!(a.steps[0].f.to_bits(), b.steps[0].f.to_bits());
        assert!(store.load([2; 32]).await.unwrap().is_none());
        let path = dir.path().join(name([1; 32]));
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[70] ^= 1;
        std::fs::write(path, bytes).unwrap();
        assert!(store.load([1; 32]).await.is_err());
    }
    #[tokio::test]
    async fn bounded_store_evicts_and_ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"keep").unwrap();
        let store = Arc::new(PathStore::new(dir.path().into()));
        for i in 0..65u8 {
            store.save([i; 32], paths()).await.unwrap();
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 65);
        assert_eq!(std::fs::read(dir.path().join("keep.txt")).unwrap(), b"keep");
    }
    #[test]
    fn refuses_wrong_key_truncation_and_nonfinite() {
        let bytes = encode([1; 32], &paths()).unwrap();
        assert!(decode(&bytes, [2; 32]).is_err());
        assert!(decode(&bytes[..79], [1; 32]).is_err());
        let mut p = paths();
        Arc::make_mut(p.values_mut().next().unwrap().as_mut().unwrap()).steps[0].a = f64::NAN;
        assert!(encode([1; 32], &p).is_err());
    }
}
