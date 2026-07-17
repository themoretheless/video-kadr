use std::path::PathBuf;

use anyhow::{anyhow, Result};
use video_editor_backend::backup::{create_snapshot, restore_snapshot, verify_snapshot};
use video_editor_backend::db::Db;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, storage, backup_root] if command == "create" => {
            let storage = PathBuf::from(storage);
            let db = Db::open(&storage).await?;
            let snapshot = create_snapshot(&storage, &PathBuf::from(backup_root), &db).await?;
            println!("{}", snapshot.display());
        }
        [command, snapshot] if command == "verify" => {
            let manifest = verify_snapshot(&PathBuf::from(snapshot)).await?;
            println!(
                "verified {} files root={}",
                manifest.files.len(),
                manifest.root_hash
            );
        }
        [command, snapshot, target] if command == "restore" => {
            restore_snapshot(&PathBuf::from(snapshot), &PathBuf::from(target)).await?;
            println!("{}", target);
        }
        _ => {
            return Err(anyhow!(
                "usage: backup create <storage> <backup-root> | verify <snapshot> | restore <snapshot> <target>"
            ));
        }
    }
    Ok(())
}
