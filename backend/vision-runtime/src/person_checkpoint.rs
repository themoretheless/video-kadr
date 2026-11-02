//! Immutable completed-frame receipts. A committed prefix survives process death.
//! The host must independently verify every receipt before using it as a mask.
use anyhow::{ensure, Result};
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Context<'a> {
    pub source_hash: &'a str,
    pub frame_hash: &'a str,
    pub model_hash: &'a str,
    pub runtime_hash: &'a str,
    pub width: usize,
    pub height: usize,
    pub mask_width: usize,
    pub mask_height: usize,
    pub total: u64,
}
struct Pending(Option<PathBuf>);
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
pub(super) fn validate_root(root: &Path) -> Result<()> {
    ensure!(root.is_absolute(), "checkpoint root must be absolute");
    let metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir(),
        "checkpoint root must be a real directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o777 == 0o700,
            "checkpoint root is not private"
        );
    }
    ensure!(
        std::fs::read_dir(root)?.next().is_none(),
        "checkpoint root is not empty"
    );
    Ok(())
}
fn sync_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = super::person_sequence::private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
pub(super) fn publish(
    root: &Path,
    ordinal: u64,
    context: Context<'_>,
    rgba: &[u8],
    mask: &[u8],
) -> Result<()> {
    ensure!(
        ordinal < context.total
            && context
                .width
                .checked_mul(context.height)
                .and_then(|n| n.checked_mul(4))
                == Some(rgba.len())
            && context.mask_width.checked_mul(context.mask_height) == Some(mask.len()),
        "invalid checkpoint frame dimensions"
    );
    let directory = root.join(ordinal.to_string());
    std::fs::create_dir(&directory)?;
    let mut pending = Pending(Some(directory.clone()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    }
    sync_file(&directory.join("frame.rgba"), rgba)?;
    sync_file(&directory.join("mask.raw"), mask)?;
    let receipt = serde_json::json!({"schemaVersion":1,"ordinal":ordinal,"context":context,"outputHash":super::digest(rgba),"maskHash":super::digest(mask),"blank":mask.iter().all(|byte|*byte<10)});
    let temporary = directory.join("receipt.pending");
    sync_file(&temporary, &serde_json::to_vec(&receipt)?)?;
    // Only receipt.json denotes a completed frame; never replace an existing one.
    std::fs::hard_link(&temporary, directory.join("receipt.json"))?;
    std::fs::remove_file(temporary)?;
    #[cfg(unix)]
    {
        std::fs::File::open(&directory)?.sync_all()?;
        std::fs::File::open(root)?.sync_all()?;
    }
    pending.0 = None;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipts_are_private_immutable_and_committed_last() {
        let root = std::env::temp_dir().join(format!(
            "checkpoint-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        validate_root(&root).unwrap();
        let context = || Context {
            source_hash: "source",
            frame_hash: "frame",
            model_hash: "model",
            runtime_hash: "runtime",
            width: 1,
            height: 1,
            mask_width: 1,
            mask_height: 1,
            total: 3,
        };
        assert!(publish(&root, 0, context(), &[1, 2, 3, 255], &[]).is_err());
        assert!(!root.join("0").exists());
        publish(&root, 0, context(), &[1, 2, 3, 255], &[0]).unwrap();
        let original = std::fs::read(root.join("0/receipt.json")).unwrap();
        let receipt: serde_json::Value = serde_json::from_slice(&original).unwrap();
        assert_eq!(receipt["outputHash"], super::super::digest(&[1, 2, 3, 255]));
        assert_eq!(receipt["blank"], true);
        assert!(!root.join("0/receipt.pending").exists());
        assert!(publish(&root, 0, context(), &[0; 4], &[255]).is_err());
        assert_eq!(
            std::fs::read(root.join("0/receipt.json")).unwrap(),
            original
        );
        assert!(validate_root(&root).is_err());
        assert!(publish(&root, 3, context(), &[0; 4], &[255]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
