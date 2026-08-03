//! Durable coordination and bounded LRU metadata for disposable preview artifacts.
//!
//! File creation/deletion deliberately remains the caller's responsibility.  The
//! catalog returns evicted locators only after their rows have been removed.

use std::time::Duration;
#[cfg(unix)]
use std::{
    ffi::CString,
    io::Read,
    os::fd::{FromRawFd, RawFd},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Copy)]
pub struct PreviewCacheBudget {
    pub max_bytes: u64,
    pub max_entries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCacheEntry {
    pub key: String,
    pub locator: String,
    pub size_bytes: u64,
    pub checksum: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewCacheLease {
    pub key: String,
    pub token: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewCacheAcquire {
    Ready(PreviewCacheEntry),
    Lease(PreviewCacheLease),
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewCachePublish {
    pub entry: PreviewCacheEntry,
    pub evicted_locators: Vec<String>,
}

#[derive(Clone)]
pub struct PreviewCacheCatalog {
    db: Db,
    budget: PreviewCacheBudget,
}

impl PreviewCacheCatalog {
    pub fn new(db: Db, budget: PreviewCacheBudget) -> Result<Self> {
        if budget.max_bytes == 0
            || budget.max_entries == 0
            || budget.max_bytes > i64::MAX as u64
            || budget.max_entries > i64::MAX as u64
        {
            return Err(anyhow!("preview cache budget must be positive"));
        }
        Ok(Self { db, budget })
    }

    pub(crate) async fn migrate(db: &Db) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS preview_cache_clock (id INTEGER PRIMARY KEY CHECK(id=1), seq INTEGER NOT NULL);\
             INSERT OR IGNORE INTO preview_cache_clock(id, seq) VALUES(1, 0);\
             CREATE TABLE IF NOT EXISTS preview_cache_entries (\
               cache_key TEXT PRIMARY KEY, state TEXT NOT NULL CHECK(state IN ('building','ready')),\
               generation INTEGER NOT NULL, lease_token TEXT, lease_expires_at INTEGER,\
               locator TEXT, size_bytes INTEGER, checksum TEXT, access_seq INTEGER NOT NULL,\
               pin_count INTEGER NOT NULL DEFAULT 0 CHECK(pin_count >= 0));\
             CREATE INDEX IF NOT EXISTS idx_preview_cache_lru ON preview_cache_entries(state, pin_count, access_seq);",
        )
        .execute(db.pool())
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS preview_cache_gc (\
               locator TEXT PRIMARY KEY, checksum TEXT NOT NULL, size_bytes INTEGER NOT NULL,\
               queued_at INTEGER NOT NULL);",
        )
        .execute(db.pool())
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS preview_cache_reservations (\
               cache_key TEXT PRIMARY KEY, generation INTEGER NOT NULL, bytes INTEGER NOT NULL);",
        )
        .execute(db.pool())
        .await?;
        Ok(())
    }

    /// Atomically returns a verified-ready descriptor, a producer lease, or Busy.
    /// Expired producers are fenced by a monotonically increasing generation.
    pub async fn acquire(&self, key: &str, now: i64, ttl: Duration) -> Result<PreviewCacheAcquire> {
        validate_key(key)?;
        let ttl = i64::try_from(ttl.as_secs()).context("preview lease TTL overflow")?;
        if ttl <= 0 {
            return Err(anyhow!("preview lease TTL must be positive"));
        }
        let expires = now
            .checked_add(ttl)
            .ok_or_else(|| anyhow!("preview lease expiry overflow"))?;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let seq = next_seq(&mut tx).await?;
        let row = sqlx::query("SELECT state, generation, lease_expires_at, locator, size_bytes, checksum FROM preview_cache_entries WHERE cache_key=?")
            .bind(key).fetch_optional(&mut *tx).await?;
        let outcome = if let Some(row) = row {
            let state: String = row.try_get("state")?;
            if state == "ready" {
                sqlx::query("UPDATE preview_cache_entries SET access_seq=? WHERE cache_key=?")
                    .bind(seq)
                    .bind(key)
                    .execute(&mut *tx)
                    .await?;
                let entry = row_to_entry(key, &row)?;
                if entry.size_bytes == 0 || entry.size_bytes > self.budget.max_bytes {
                    return Err(anyhow!("persisted preview cache size is invalid"));
                }
                PreviewCacheAcquire::Ready(entry)
            } else if row
                .try_get::<Option<i64>, _>("lease_expires_at")?
                .unwrap_or(0)
                > now
            {
                PreviewCacheAcquire::Busy
            } else {
                if let Some(old_locator) = row.try_get::<Option<String>, _>("locator")? {
                    validate_locator_for_key(&old_locator, key)?;
                    sqlx::query("INSERT OR IGNORE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,'expired',0,?)")
                        .bind(old_locator).bind(seq).execute(&mut *tx).await?;
                }
                let generation = row
                    .try_get::<i64, _>("generation")?
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("preview generation overflow"))?;
                sqlx::query("DELETE FROM preview_cache_reservations WHERE cache_key=?")
                    .bind(key)
                    .execute(&mut *tx)
                    .await?;
                let token = Uuid::new_v4().to_string();
                sqlx::query("UPDATE preview_cache_entries SET generation=?, lease_token=?, lease_expires_at=?, locator=NULL, access_seq=? WHERE cache_key=?")
                    .bind(generation).bind(&token).bind(expires).bind(seq).bind(key).execute(&mut *tx).await?;
                PreviewCacheAcquire::Lease(PreviewCacheLease {
                    key: key.into(),
                    token,
                    generation: generation as u64,
                })
            }
        } else {
            // Permit one transient producer beyond the ready-entry budget so
            // publication can atomically evict an old ready entry, while still
            // bounding abandoned/distinct-key building rows.
            let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM preview_cache_entries")
                .fetch_one(&mut *tx)
                .await?;
            let catalog_limit = i64::try_from(self.budget.max_entries)?
                .checked_add(1)
                .ok_or_else(|| anyhow!("preview catalog entry limit overflow"))?;
            if rows >= catalog_limit {
                tx.commit().await?;
                return Ok(PreviewCacheAcquire::Busy);
            }
            let token = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO preview_cache_entries(cache_key,state,generation,lease_token,lease_expires_at,access_seq) VALUES(?,'building',1,?,?,?)")
                .bind(key).bind(&token).bind(expires).bind(seq).execute(&mut *tx).await?;
            PreviewCacheAcquire::Lease(PreviewCacheLease {
                key: key.into(),
                token,
                generation: 1,
            })
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// Fenced publication plus byte/entry bounded LRU eviction in one transaction.
    pub async fn publish(
        &self,
        lease: &PreviewCacheLease,
        locator: &str,
        size_bytes: u64,
        checksum: &str,
    ) -> Result<PreviewCachePublish> {
        validate_locator_for_key(locator, &lease.key)?;
        validate_checksum(checksum)?;
        if size_bytes == 0 || size_bytes > self.budget.max_bytes {
            return Err(anyhow!("preview artifact exceeds cache budget"));
        }
        let size = i64::try_from(size_bytes).context("preview artifact size overflow")?;
        let generation = i64::try_from(lease.generation).context("preview generation overflow")?;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let seq = next_seq(&mut tx).await?;
        let changed = sqlx::query("UPDATE preview_cache_entries SET state='ready', size_bytes=?, checksum=?, lease_token=NULL, lease_expires_at=NULL, access_seq=? WHERE cache_key=? AND state='building' AND generation=? AND lease_token=? AND locator=? AND EXISTS (SELECT 1 FROM preview_cache_reservations r WHERE r.cache_key=preview_cache_entries.cache_key AND r.generation=preview_cache_entries.generation AND r.bytes>=?)")
            .bind(size).bind(checksum).bind(seq).bind(&lease.key).bind(generation).bind(&lease.token).bind(locator).bind(size).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            sqlx::query("INSERT OR IGNORE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,'stale',0,?)")
                .bind(locator).bind(seq).execute(&mut *tx).await?;
            tx.commit().await?;
            return Err(anyhow!("stale or unknown preview cache lease"));
        }
        sqlx::query("DELETE FROM preview_cache_reservations WHERE cache_key=? AND generation=?")
            .bind(&lease.key)
            .bind(generation)
            .execute(&mut *tx)
            .await?;
        loop {
            let totals = sqlx::query("SELECT COUNT(*) AS entries, COALESCE(SUM(size_bytes),0) AS bytes FROM preview_cache_entries WHERE state='ready'").fetch_one(&mut *tx).await?;
            let entries: i64 = totals.try_get("entries")?;
            let bytes: i64 = totals.try_get("bytes")?;
            if entries <= self.budget.max_entries as i64 && bytes <= self.budget.max_bytes as i64 {
                break;
            }
            let victim = sqlx::query("SELECT cache_key, locator FROM preview_cache_entries WHERE state='ready' AND cache_key<>? ORDER BY access_seq, cache_key LIMIT 1")
                .bind(&lease.key).fetch_optional(&mut *tx).await?;
            let Some(victim) = victim else {
                return Err(anyhow!(
                    "preview cache budget cannot be satisfied while entries are pinned"
                ));
            };
            let victim_key: String = victim.try_get("cache_key")?;
            let victim_locator: String = victim.try_get("locator")?;
            let victim_checksum: String =
                sqlx::query_scalar("SELECT checksum FROM preview_cache_entries WHERE cache_key=?")
                    .bind(&victim_key)
                    .fetch_one(&mut *tx)
                    .await?;
            let victim_size: i64 = sqlx::query_scalar(
                "SELECT size_bytes FROM preview_cache_entries WHERE cache_key=?",
            )
            .bind(&victim_key)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("INSERT OR REPLACE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,?,?,?)")
                .bind(&victim_locator).bind(victim_checksum).bind(victim_size).bind(seq).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM preview_cache_entries WHERE cache_key=?")
                .bind(victim_key)
                .execute(&mut *tx)
                .await?;
        }
        let entry = PreviewCacheEntry {
            key: lease.key.clone(),
            locator: locator.into(),
            size_bytes,
            checksum: checksum.into(),
            generation: lease.generation,
        };
        tx.commit().await?;
        Ok(PreviewCachePublish {
            entry,
            // Physical deletion is performed by `reconcile_files` while a DB
            // write lock prevents an ABA re-publication race.
            evicted_locators: Vec::new(),
        })
    }

    pub async fn register_build_locator(
        &self,
        lease: &PreviewCacheLease,
        locator: &str,
        reserved_bytes: u64,
    ) -> Result<()> {
        validate_locator_for_key(locator, &lease.key)?;
        if reserved_bytes == 0 || reserved_bytes > self.budget.max_bytes {
            return Err(anyhow!("preview build reservation exceeds cache budget"));
        }
        let reserved = i64::try_from(reserved_bytes)?;
        let generation = i64::try_from(lease.generation)?;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE preview_cache_entries SET locator=? WHERE cache_key=? AND state='building' AND generation=? AND lease_token=?")
            .bind(locator).bind(&lease.key).bind(i64::try_from(lease.generation)?).bind(&lease.token)
            .execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(anyhow!("stale or unknown preview cache lease"));
        }
        loop {
            let used: i64 = sqlx::query_scalar("SELECT COALESCE((SELECT SUM(size_bytes) FROM preview_cache_entries WHERE state='ready'),0)+COALESCE((SELECT SUM(bytes) FROM preview_cache_reservations),0)")
                .fetch_one(&mut *tx).await?;
            if used
                .checked_add(reserved)
                .ok_or_else(|| anyhow!("preview reservation overflow"))?
                <= self.budget.max_bytes as i64
            {
                break;
            }
            let victim = sqlx::query("SELECT cache_key,locator,checksum,size_bytes FROM preview_cache_entries WHERE state='ready' AND cache_key<>? ORDER BY access_seq,cache_key LIMIT 1")
                .bind(&lease.key).fetch_optional(&mut *tx).await?;
            let Some(victim) = victim else {
                return Err(anyhow!("preview cache has insufficient reserved capacity"));
            };
            let victim_key: String = victim.try_get("cache_key")?;
            sqlx::query("INSERT OR REPLACE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,?,?,?)")
                .bind(victim.try_get::<String,_>("locator")?)
                .bind(victim.try_get::<String,_>("checksum")?)
                .bind(victim.try_get::<i64,_>("size_bytes")?)
                .bind(next_seq(&mut tx).await?).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM preview_cache_entries WHERE cache_key=?")
                .bind(victim_key)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT OR REPLACE INTO preview_cache_reservations(cache_key,generation,bytes) VALUES(?,?,?)")
            .bind(&lease.key).bind(generation).bind(reserved).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn abandon(&self, lease: &PreviewCacheLease) -> Result<bool> {
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let locator = sqlx::query_scalar::<_, Option<String>>("SELECT locator FROM preview_cache_entries WHERE cache_key=? AND state='building' AND generation=? AND lease_token=?")
            .bind(&lease.key).bind(i64::try_from(lease.generation)?).bind(&lease.token)
            .fetch_optional(&mut *tx).await?.flatten();
        if let Some(locator) = locator {
            validate_locator_for_key(&locator, &lease.key)?;
            sqlx::query("INSERT OR IGNORE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,'abandoned',0,?)")
                .bind(locator).bind(next_seq(&mut tx).await?).execute(&mut *tx).await?;
        }
        let removed = sqlx::query("DELETE FROM preview_cache_entries WHERE cache_key=? AND state='building' AND generation=? AND lease_token=?")
            .bind(&lease.key).bind(i64::try_from(lease.generation)?).bind(&lease.token)
            .execute(&mut *tx).await?.rows_affected() == 1;
        sqlx::query("DELETE FROM preview_cache_reservations WHERE cache_key=? AND generation=?")
            .bind(&lease.key)
            .bind(i64::try_from(lease.generation)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(removed)
    }

    pub async fn invalidate_ready(&self, key: &str, checksum: &str) -> Result<Option<String>> {
        validate_key(key)?;
        validate_checksum(checksum)?;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT locator,size_bytes FROM preview_cache_entries WHERE cache_key=? AND state='ready' AND checksum=?")
            .bind(key).bind(checksum).fetch_optional(&mut *tx).await?;
        let locator = if let Some(row) = row {
            let locator: String = row.try_get("locator")?;
            validate_locator_for_key(&locator, key)?;
            let size: i64 = row.try_get("size_bytes")?;
            sqlx::query("INSERT OR REPLACE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,?,?,?)")
                .bind(&locator).bind(checksum).bind(size).bind(next_seq(&mut tx).await?).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM preview_cache_entries WHERE cache_key=?")
                .bind(key)
                .execute(&mut *tx)
                .await?;
            Some(locator)
        } else {
            None
        };
        tx.commit().await?;
        Ok(locator)
    }

    /// Drops expired unfinished rows; callers may then remove abandoned staging files.
    pub async fn reconcile(&self, now: i64) -> Result<u64> {
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let expired = sqlx::query("SELECT cache_key,locator FROM preview_cache_entries WHERE state='building' AND lease_expires_at<=?")
            .bind(now).fetch_all(&mut *tx).await?;
        for row in &expired {
            if let Some(locator) = row.try_get::<Option<String>, _>("locator")? {
                let cache_key: String = row.try_get("cache_key")?;
                validate_locator_for_key(&locator, &cache_key)?;
                sqlx::query("INSERT OR IGNORE INTO preview_cache_gc(locator,checksum,size_bytes,queued_at) VALUES(?,'expired',0,?)")
                    .bind(locator).bind(now).execute(&mut *tx).await?;
            }
        }
        let removed = sqlx::query(
            "DELETE FROM preview_cache_entries WHERE state='building' AND lease_expires_at<=?",
        )
        .bind(now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query("DELETE FROM preview_cache_reservations WHERE cache_key NOT IN (SELECT cache_key FROM preview_cache_entries WHERE state='building')")
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(removed)
    }

    /// Drain durable tombstones. The SQLite writer lock spans unlink, so a new
    /// publication cannot reuse a locator while an older generation is deleted.
    pub async fn reconcile_files(&self, root: &std::path::Path, now: i64) -> Result<u64> {
        self.reconcile(now).await?;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let rows = sqlx::query("SELECT locator FROM preview_cache_gc ORDER BY queued_at,locator")
            .fetch_all(&mut *tx)
            .await?;
        let mut removed = 0;
        for row in rows {
            let locator: String = row.try_get("locator")?;
            validate_locator(&locator)?;
            let referenced: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM preview_cache_entries WHERE state='ready' AND locator=?",
            )
            .bind(&locator)
            .fetch_one(&mut *tx)
            .await?;
            if referenced == 0 {
                secure_unlink_locator(root, &locator).await?;
                sqlx::query("DELETE FROM preview_cache_gc WHERE locator=?")
                    .bind(locator)
                    .execute(&mut *tx)
                    .await?;
                removed += 1;
            }
        }
        tx.commit().await?;
        Ok(removed)
    }

    /// Read a ready artifact while holding the SQLite writer lock. Eviction and
    /// invalidation therefore cannot unlink it between authorization and open;
    /// no crash-leaking process-global pins are required.
    pub async fn read_ready_bytes(
        &self,
        root: &std::path::Path,
        key: &str,
        locator: &str,
        checksum: &str,
    ) -> Result<Vec<u8>> {
        validate_key(key)?;
        validate_locator_for_key(locator, key)?;
        validate_checksum(checksum)?;
        let mut tx = self.db.pool().begin_with("BEGIN IMMEDIATE").await?;
        let size: Option<i64> = sqlx::query_scalar("SELECT size_bytes FROM preview_cache_entries WHERE cache_key=? AND state='ready' AND locator=? AND checksum=?")
            .bind(key).bind(locator).bind(checksum).fetch_optional(&mut *tx).await?;
        let size = size.ok_or_else(|| anyhow!("preview cache entry is no longer ready"))?;
        let size = u64::try_from(size)?;
        if size == 0 || size > self.budget.max_bytes {
            return Err(anyhow!("persisted preview cache size is invalid"));
        }
        let bytes = secure_read_locator(root, locator, size).await?;
        if bytes.len() as u64 != size {
            return Err(anyhow!("preview cache file size changed while reading"));
        }
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if actual != checksum {
            return Err(anyhow!("preview cache file checksum mismatch"));
        }
        tx.commit().await?;
        Ok(bytes)
    }
}

async fn next_seq(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<i64> {
    Ok(
        sqlx::query_scalar("UPDATE preview_cache_clock SET seq=seq+1 WHERE id=1 RETURNING seq")
            .fetch_one(&mut **tx)
            .await?,
    )
}

fn row_to_entry(key: &str, row: &sqlx::sqlite::SqliteRow) -> Result<PreviewCacheEntry> {
    let entry = PreviewCacheEntry {
        key: key.into(),
        locator: row.try_get("locator")?,
        size_bytes: u64::try_from(row.try_get::<i64, _>("size_bytes")?)?,
        checksum: row.try_get("checksum")?,
        generation: u64::try_from(row.try_get::<i64, _>("generation")?)?,
    };
    validate_key(&entry.key)?;
    validate_locator_for_key(&entry.locator, &entry.key)?;
    validate_checksum(&entry.checksum)?;
    Ok(entry)
}
fn validate_key(value: &str) -> Result<()> {
    validate_hex(value, "cache key")
}
fn validate_checksum(value: &str) -> Result<()> {
    validate_hex(value, "checksum")
}
fn validate_hex(value: &str, label: &str) -> Result<()> {
    if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(anyhow!("invalid preview {label}"))
    }
}
fn locator_key(value: &str) -> Result<&str> {
    let path = std::path::Path::new(value);
    let parts: Vec<_> = path.components().collect();
    if value.len() > 512 || path.is_absolute() || parts.len() != 4 {
        return Err(anyhow!("invalid preview artifact locator"));
    }
    let normal: Option<Vec<&str>> = parts
        .iter()
        .map(|part| match part {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect();
    let normal = normal.ok_or_else(|| anyhow!("invalid preview artifact locator"))?;
    let key = normal[2];
    let file = std::path::Path::new(normal[3]);
    let stem = file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let extension = file
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if normal[0] != "frames"
        || normal[1].len() != 2
        || key.len() != 64
        || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
        || normal[1] != &key[..2]
        || Uuid::parse_str(stem).is_err()
        || !matches!(extension, "jpg" | "webp")
    {
        return Err(anyhow!("invalid preview artifact locator"));
    }
    Ok(key)
}

fn validate_locator(value: &str) -> Result<()> {
    locator_key(value).map(|_| ())
}

fn validate_locator_for_key(value: &str, expected_key: &str) -> Result<()> {
    validate_key(expected_key)?;
    if locator_key(value)? != expected_key {
        return Err(anyhow!("preview locator key mismatch"));
    }
    Ok(())
}

#[cfg(unix)]
fn cstring(value: &std::ffi::OsStr) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(value.as_bytes()).map_err(|_| anyhow!("preview path contains NUL"))
}

#[cfg(unix)]
fn open_directory_at(parent: RawFd, name: &std::ffi::OsStr) -> Result<RawFd> {
    let name = cstring(name)?;
    let fd = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(fd)
    }
}

#[cfg(unix)]
fn open_parent_dir(root: &std::path::Path, locator: &str) -> Result<(std::fs::File, CString)> {
    validate_locator(locator)?;
    let root_name = cstring(root.as_os_str())?;
    let root_fd = unsafe {
        libc::open(
            root_name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut current = unsafe { std::fs::File::from_raw_fd(root_fd) };
    let path = std::path::Path::new(locator);
    let mut components: Vec<_> = path.components().collect();
    let final_name = match components.pop() {
        Some(std::path::Component::Normal(value)) => cstring(value)?,
        _ => return Err(anyhow!("invalid preview artifact locator")),
    };
    use std::os::fd::AsRawFd;
    for component in components {
        let std::path::Component::Normal(value) = component else {
            return Err(anyhow!("invalid preview artifact locator"));
        };
        let fd = open_directory_at(current.as_raw_fd(), value)?;
        current = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    Ok((current, final_name))
}

async fn secure_read_locator(
    root: &std::path::Path,
    locator: &str,
    expected_size: u64,
) -> Result<Vec<u8>> {
    #[cfg(unix)]
    {
        let root = root.to_owned();
        let locator = locator.to_owned();
        tokio::task::spawn_blocking(move || {
            use std::os::fd::AsRawFd;
            let (parent, name) = open_parent_dir(&root, &locator)?;
            let fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            let metadata = file.metadata()?;
            if !metadata.file_type().is_file() || metadata.len() != expected_size {
                return Err(anyhow!("preview locator is not a regular file"));
            }
            let mut bytes = Vec::new();
            file.take(expected_size.saturating_add(1))
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .await
        .map_err(|error| anyhow!("preview read task failed: {error}"))?
    }
    #[cfg(not(unix))]
    Err(anyhow!(
        "race-free preview cache file access is unsupported on this platform"
    ))
}

async fn secure_unlink_locator(root: &std::path::Path, locator: &str) -> Result<()> {
    #[cfg(unix)]
    {
        let root = root.to_owned();
        let locator = locator.to_owned();
        tokio::task::spawn_blocking(move || {
            use std::os::fd::AsRawFd;
            let (parent, name) = open_parent_dir(&root, &locator)?;
            for target in [
                name.clone(),
                CString::new(format!(
                    "{}.json",
                    std::path::Path::new(name.to_str()?)
                        .file_stem()
                        .and_then(|v| v.to_str())
                        .ok_or_else(|| anyhow!("invalid preview filename"))?
                ))?,
            ] {
                let result = unsafe { libc::unlinkat(parent.as_raw_fd(), target.as_ptr(), 0) };
                if result < 0 {
                    let error = std::io::Error::last_os_error();
                    if error.kind() != std::io::ErrorKind::NotFound {
                        return Err(error.into());
                    }
                }
            }
            Ok(())
        })
        .await
        .map_err(|error| anyhow!("preview unlink task failed: {error}"))?
    }
    #[cfg(not(unix))]
    Err(anyhow!(
        "race-free preview cache file access is unsupported on this platform"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(n: u8) -> String {
        format!("{n:02x}").repeat(32)
    }
    async fn catalog(bytes: u64, entries: u64) -> (PreviewCacheCatalog, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).await.unwrap();
        (
            PreviewCacheCatalog::new(
                db,
                PreviewCacheBudget {
                    max_bytes: bytes,
                    max_entries: entries,
                },
            )
            .unwrap(),
            dir,
        )
    }
    fn lease(value: PreviewCacheAcquire) -> PreviewCacheLease {
        match value {
            PreviewCacheAcquire::Lease(v) => v,
            _ => panic!("expected lease"),
        }
    }
    fn locator(cache_key: &str, lease: &PreviewCacheLease) -> String {
        format!(
            "frames/{}/{}/{}.jpg",
            &cache_key[..2],
            cache_key,
            lease.token
        )
    }
    async fn publish(
        cache: &PreviewCacheCatalog,
        lease: &PreviewCacheLease,
        locator: &str,
        size: u64,
        checksum: &str,
    ) -> Result<PreviewCachePublish> {
        cache.register_build_locator(lease, locator, size).await?;
        cache.publish(lease, locator, size, checksum).await
    }
    #[tokio::test]
    async fn concurrent_acquire_has_one_producer() {
        let (cache, _dir) = catalog(100, 2).await;
        let cache_key = key(1);
        let (a, b) = tokio::join!(
            cache.acquire(&cache_key, 10, Duration::from_secs(5)),
            cache.acquire(&cache_key, 10, Duration::from_secs(5))
        );
        let values = [a.unwrap(), b.unwrap()];
        assert_eq!(
            values
                .iter()
                .filter(|v| matches!(v, PreviewCacheAcquire::Lease(_)))
                .count(),
            1
        );
        assert_eq!(
            values
                .iter()
                .filter(|v| matches!(v, PreviewCacheAcquire::Busy))
                .count(),
            1
        );
    }
    #[tokio::test]
    async fn stale_generation_cannot_publish() {
        let (cache, _dir) = catalog(100, 2).await;
        let old = lease(
            cache
                .acquire(&key(1), 10, Duration::from_secs(1))
                .await
                .unwrap(),
        );
        let old_locator = locator(&key(1), &old);
        cache
            .register_build_locator(&old, &old_locator, 10)
            .await
            .unwrap();
        let new = lease(
            cache
                .acquire(&key(1), 12, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        assert!(cache
            .publish(&old, &old_locator, 10, &key(9))
            .await
            .is_err());
        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM preview_cache_gc")
            .fetch_one(cache.db.pool())
            .await
            .unwrap();
        assert!(
            queued > 0,
            "rejected stale publication must remain GC-visible"
        );
        publish(&cache, &new, &locator(&key(1), &new), 10, &key(8))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_cross_key_locator_fails_closed_without_takeover() {
        let (cache, _dir) = catalog(100, 2).await;
        let cache_key = key(1);
        let build = lease(
            cache
                .acquire(&cache_key, 10, Duration::from_secs(1))
                .await
                .unwrap(),
        );
        let wrong = locator(&key(2), &build);
        sqlx::query("UPDATE preview_cache_entries SET locator=? WHERE cache_key=?")
            .bind(&wrong)
            .bind(&cache_key)
            .execute(cache.db.pool())
            .await
            .unwrap();

        assert!(cache
            .acquire(&cache_key, 12, Duration::from_secs(1))
            .await
            .is_err());
        let row =
            sqlx::query("SELECT generation,locator FROM preview_cache_entries WHERE cache_key=?")
                .bind(&cache_key)
                .fetch_one(cache.db.pool())
                .await
                .unwrap();
        assert_eq!(row.get::<i64, _>("generation"), 1);
        assert_eq!(row.get::<String, _>("locator"), wrong);
    }

    #[tokio::test]
    async fn publish_evicts_oldest_unpinned_entry() {
        let (cache, _dir) = catalog(20, 2).await;
        let a = lease(
            cache
                .acquire(&key(1), 1, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        publish(&cache, &a, &locator(&key(1), &a), 10, &key(7))
            .await
            .unwrap();
        let b = lease(
            cache
                .acquire(&key(2), 1, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        publish(&cache, &b, &locator(&key(2), &b), 10, &key(8))
            .await
            .unwrap();
        // Touch A, making B the least recently used entry.
        assert!(matches!(
            cache
                .acquire(&key(1), 2, Duration::from_secs(5))
                .await
                .unwrap(),
            PreviewCacheAcquire::Ready(_)
        ));
        let c = lease(
            cache
                .acquire(&key(3), 2, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        let published = publish(&cache, &c, &locator(&key(3), &c), 10, &key(9))
            .await
            .unwrap();
        assert!(published.evicted_locators.is_empty());
    }

    #[tokio::test]
    async fn expired_build_reconcile_removes_registered_orphan() {
        let (cache, dir) = catalog(20, 2).await;
        let build = lease(
            cache
                .acquire(&key(1), 1, Duration::from_secs(1))
                .await
                .unwrap(),
        );
        let locator = locator(&key(1), &build);
        cache
            .register_build_locator(&build, &locator, 10)
            .await
            .unwrap();
        let path = dir.path().join(&locator);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"orphan").await.unwrap();
        cache.reconcile_files(dir.path(), 3).await.unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn tampered_ready_locator_fails_closed() {
        let (cache, _dir) = catalog(20, 2).await;
        let build = lease(
            cache
                .acquire(&key(1), 1, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        publish(&cache, &build, &locator(&key(1), &build), 10, &key(7))
            .await
            .unwrap();
        sqlx::query("UPDATE preview_cache_entries SET locator='../source' WHERE cache_key=?")
            .bind(key(1))
            .execute(cache.db.pool())
            .await
            .unwrap();
        assert!(cache
            .acquire(&key(1), 2, Duration::from_secs(5))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn concurrent_build_reservations_cannot_exceed_byte_budget() {
        let (cache, _dir) = catalog(10, 3).await;
        let a = lease(
            cache
                .acquire(&key(1), 1, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        let b = lease(
            cache
                .acquire(&key(2), 1, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        cache
            .register_build_locator(&a, &locator(&key(1), &a), 6)
            .await
            .unwrap();
        assert!(cache
            .register_build_locator(&b, &locator(&key(2), &b), 6)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn locator_embedded_key_must_match_lease_and_row_key() {
        let (cache, _dir) = catalog(20, 2).await;
        let build = lease(
            cache
                .acquire(&key(1), 1, Duration::from_secs(5))
                .await
                .unwrap(),
        );
        let foreign = locator(&key(2), &build);
        assert!(cache
            .register_build_locator(&build, &foreign, 10)
            .await
            .is_err());

        sqlx::query("UPDATE preview_cache_entries SET state='ready',locator=?,size_bytes=1,checksum=? WHERE cache_key=?")
            .bind(&foreign).bind(key(9)).bind(key(1)).execute(cache.db.pool()).await.unwrap();
        assert!(cache
            .acquire(&key(1), 2, Duration::from_secs(5))
            .await
            .is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn parent_symlink_is_rejected_before_read_or_unlink() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), root.path().join("frames")).unwrap();
        let lease = PreviewCacheLease {
            key: key(1),
            token: Uuid::new_v4().to_string(),
            generation: 1,
        };
        assert!(
            secure_read_locator(root.path(), &locator(&key(1), &lease), 1)
                .await
                .is_err()
        );
    }

    #[test]
    fn locator_topology_binds_prefix_key_uuid_and_extension() {
        let lease = PreviewCacheLease {
            key: key(1),
            token: Uuid::new_v4().to_string(),
            generation: 1,
        };
        assert!(validate_locator(&locator(&key(1), &lease)).is_ok());
        assert!(validate_locator(&format!("frames/ff/{}/{}.jpg", key(1), lease.token)).is_err());
        assert!(validate_locator(&format!("frames/01/{}/not-a-uuid.png", key(1))).is_err());
    }
}
