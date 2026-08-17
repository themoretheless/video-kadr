//! Private immutable media assets: images, audio, video, fonts and subtitles.
//!
//! The store mirrors the LUT/library pattern: a persisted index written
//! atomically, server-generated ids validated against an allow-list, and a
//! `resolve` method that is the only way a render feature turns a client id
//! into a private filesystem path. Content type comes from magic bytes and a
//! bounded `ffprobe`, never from the client extension or `Content-Type`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

pub const ASSET_SCHEMA_VERSION: u32 = 1;
/// Body cap for image/audio/video assets.
pub const MAX_MEDIA_ASSET_BYTES: usize = 64 * 1024 * 1024;
/// Body cap for font/subtitle assets.
pub const MAX_TEXT_ASSET_BYTES: usize = 4 * 1024 * 1024;
/// Ids are `ast_` plus a 32-character hex suffix; the allow-list accepts the
/// documented `^ast_[a-zA-Z0-9]{16,}$` shape and rejects anything longer.
const ID_PREFIX: &str = "ast_";
const MIN_ID_SUFFIX_LEN: usize = 16;
const MAX_ID_SUFFIX_LEN: usize = 48;
/// Enough bytes to recognise every container/font/image signature we accept.
const SNIFF_WINDOW_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Image,
    Audio,
    Video,
    Font,
    Subtitle,
}

impl AssetKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "audio" => Some(Self::Audio),
            "video" => Some(Self::Video),
            "font" => Some(Self::Font),
            "subtitle" => Some(Self::Subtitle),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Font => "font",
            Self::Subtitle => "subtitle",
        }
    }

    /// Per-kind body cap from the feature contract.
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Image | Self::Audio | Self::Video => MAX_MEDIA_ASSET_BYTES,
            Self::Font | Self::Subtitle => MAX_TEXT_ASSET_BYTES,
        }
    }

    /// True when recognising the content additionally requires a media probe.
    pub fn needs_probe(self) -> bool {
        matches!(self, Self::Audio | Self::Video)
    }
}

/// Public metadata for one stored asset. `filename` is server-generated and
/// therefore safe to publish; the client name is never persisted as a path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRecord {
    pub schema_version: u32,
    pub id: String,
    pub kind: AssetKind,
    pub filename: String,
    pub mime: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
}

impl AssetRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        kind: AssetKind,
        filename: String,
        mime: String,
        size_bytes: u64,
        sha256: String,
        created_at: u64,
    ) -> Self {
        Self {
            schema_version: ASSET_SCHEMA_VERSION,
            id,
            kind,
            filename,
            mime,
            size_bytes,
            sha256,
            created_at,
            width: None,
            height: None,
            duration: None,
        }
    }

    pub fn with_media_metadata(
        mut self,
        width: Option<u32>,
        height: Option<u32>,
        duration: Option<f64>,
    ) -> Self {
        // Non-finite or negative probe values are dropped instead of persisted.
        self.width = width.filter(|value| *value > 0);
        self.height = height.filter(|value| *value > 0);
        self.duration = duration.filter(|value| value.is_finite() && *value > 0.0);
        self
    }
}

/// Generate a fresh id in the documented `ast_<hex>` shape.
pub fn new_asset_id() -> String {
    format!("{ID_PREFIX}{}", uuid::Uuid::new_v4().simple())
}

/// Allow-list check applied before an id ever reaches the filesystem. Rejects
/// separators, dots, and anything outside `[a-zA-Z0-9]` after the prefix.
pub fn is_valid_asset_id(id: &str) -> bool {
    let Some(suffix) = id.strip_prefix(ID_PREFIX) else {
        return false;
    };
    (MIN_ID_SUFFIX_LEN..=MAX_ID_SUFFIX_LEN).contains(&suffix.len())
        && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Broad content family a magic-byte signature belongs to. Audio and video
/// share the `Container` family because only a probe can tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentClass {
    Image,
    Font,
    Subtitle,
    Container,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SniffedContent {
    pub class: ContentClass,
    /// Server-chosen stored extension. Never derived from the client name.
    pub extension: &'static str,
    pub mime: &'static str,
}

impl SniffedContent {
    const fn new(class: ContentClass, extension: &'static str, mime: &'static str) -> Self {
        Self {
            class,
            extension,
            mime,
        }
    }

    /// True when this signature can back the kind the client asked for.
    pub fn supports(self, kind: AssetKind) -> bool {
        match kind {
            AssetKind::Image => self.class == ContentClass::Image,
            AssetKind::Font => self.class == ContentClass::Font,
            AssetKind::Subtitle => self.class == ContentClass::Subtitle,
            AssetKind::Audio | AssetKind::Video => self.class == ContentClass::Container,
        }
    }
}

/// Recognise the uploaded bytes from their leading signature. Only the formats
/// the renderer can actually consume are accepted; everything else fails closed.
pub fn sniff_content(bytes: &[u8]) -> Option<SniffedContent> {
    let head = &bytes[..bytes.len().min(SNIFF_WINDOW_BYTES)];
    let starts = |prefix: &[u8]| head.starts_with(prefix);
    let at = |offset: usize, marker: &[u8]| {
        head.len() >= offset + marker.len() && &head[offset..offset + marker.len()] == marker
    };

    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some(SniffedContent::new(ContentClass::Image, "png", "image/png"));
    }
    if starts(b"\xff\xd8\xff") {
        return Some(SniffedContent::new(
            ContentClass::Image,
            "jpg",
            "image/jpeg",
        ));
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some(SniffedContent::new(ContentClass::Image, "gif", "image/gif"));
    }
    if starts(b"RIFF") && at(8, b"WEBP") {
        return Some(SniffedContent::new(
            ContentClass::Image,
            "webp",
            "image/webp",
        ));
    }
    if starts(b"\x00\x00\x01\x00") {
        return Some(SniffedContent::new(
            ContentClass::Image,
            "ico",
            "image/x-icon",
        ));
    }
    if starts(b"\x00\x01\x00\x00") || starts(b"true") {
        return Some(SniffedContent::new(ContentClass::Font, "ttf", "font/ttf"));
    }
    if starts(b"OTTO") {
        return Some(SniffedContent::new(ContentClass::Font, "otf", "font/otf"));
    }
    if starts(b"wOFF") {
        return Some(SniffedContent::new(ContentClass::Font, "woff", "font/woff"));
    }
    if starts(b"wOF2") {
        return Some(SniffedContent::new(
            ContentClass::Font,
            "woff2",
            "font/woff2",
        ));
    }
    if at(4, b"ftyp") {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "mp4",
            "video/mp4",
        ));
    }
    if starts(b"\x1a\x45\xdf\xa3") {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "mkv",
            "video/x-matroska",
        ));
    }
    if starts(b"OggS") {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "ogg",
            "application/ogg",
        ));
    }
    if starts(b"RIFF") && at(8, b"WAVE") {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "wav",
            "audio/wav",
        ));
    }
    if starts(b"RIFF") && at(8, b"AVI ") {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "avi",
            "video/x-msvideo",
        ));
    }
    if starts(b"fLaC") {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "flac",
            "audio/flac",
        ));
    }
    if starts(b"ID3") || (head.len() >= 2 && head[0] == 0xff && (head[1] & 0xe0) == 0xe0) {
        return Some(SniffedContent::new(
            ContentClass::Container,
            "mp3",
            "audio/mpeg",
        ));
    }
    sniff_subtitle(bytes)
}

/// Subtitles have no binary signature, so the structure is validated instead:
/// valid UTF-8 with at least one cue arrow, WebVTT additionally by its header.
fn sniff_subtitle(bytes: &[u8]) -> Option<SniffedContent> {
    if bytes.len() > MAX_TEXT_ASSET_BYTES {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.contains('\0') || !text.contains("-->") {
        return None;
    }
    if text.trim_start().starts_with("WEBVTT") {
        return Some(SniffedContent::new(
            ContentClass::Subtitle,
            "vtt",
            "text/vtt",
        ));
    }
    // SubRip cues open with a numeric counter line.
    let first = text.lines().find(|line| !line.trim().is_empty())?;
    first
        .trim()
        .parse::<u32>()
        .ok()
        .map(|_| SniffedContent::new(ContentClass::Subtitle, "srt", "application/x-subrip"))
}

/// JSON-index-backed asset store. The index lives next to the served directory
/// (never inside it) so `/files/assets` cannot expose the catalogue.
#[derive(Clone)]
pub struct AssetStore {
    records: Arc<RwLock<HashMap<String, AssetRecord>>>,
    index_path: PathBuf,
    assets_dir: PathBuf,
}

impl AssetStore {
    /// Load `storage/assets.json` (empty when missing or corrupt). Reading the
    /// small index synchronously keeps `AppState::new` non-async, and `resolve`
    /// stays a plain lookup for the render features that call it.
    pub fn load(storage: PathBuf) -> Self {
        let index_path = storage.join("assets.json");
        let assets_dir = storage.join("assets");
        let records = match std::fs::read(&index_path) {
            Ok(bytes) => serde_json::from_slice::<Vec<AssetRecord>>(&bytes)
                .unwrap_or_default()
                .into_iter()
                .filter(|record| is_valid_asset_id(&record.id))
                .map(|record| (record.id.clone(), record))
                .collect(),
            Err(_) => HashMap::new(),
        };
        Self {
            records: Arc::new(RwLock::new(records)),
            index_path,
            assets_dir,
        }
    }

    pub fn assets_dir(&self) -> &Path {
        &self.assets_dir
    }

    /// Turn a client-supplied id into a private path, or `None` when the id is
    /// malformed, unknown, or points at an asset of a different kind. This is
    /// the only sanctioned id-to-path conversion for render features.
    pub fn resolve(&self, id: &str, expected: AssetKind) -> Option<PathBuf> {
        let record = self.get(id)?;
        if record.kind != expected || !is_safe_stored_filename(&record.id, &record.filename) {
            return None;
        }
        Some(self.assets_dir.join(&record.filename))
    }

    pub fn get(&self, id: &str) -> Option<AssetRecord> {
        if !is_valid_asset_id(id) {
            return None;
        }
        self.read().get(id).cloned()
    }

    /// Newest first, matching the library listing order.
    pub fn list(&self) -> Vec<AssetRecord> {
        let mut records: Vec<_> = self.read().values().cloned().collect();
        records.sort_by_key(|record| std::cmp::Reverse(record.created_at));
        records
    }

    /// Persist first, then commit to memory, so a failed write never reports a
    /// stored asset that disappears on restart.
    pub async fn insert(&self, record: AssetRecord) -> bool {
        if !is_valid_asset_id(&record.id) || !is_safe_stored_filename(&record.id, &record.filename)
        {
            tracing::warn!("assets: refusing to index a record with an unsafe id or filename");
            return false;
        }
        let mut next: Vec<_> = self.read().values().cloned().collect();
        next.retain(|existing| existing.id != record.id);
        next.push(record.clone());
        if let Err(error) = self.save(&next).await {
            tracing::error!(error = %error, "assets: persist failed on insert");
            return false;
        }
        self.write().insert(record.id.clone(), record);
        true
    }

    /// Remove the index entry, then the file. The file is deleted only after a
    /// successful save so a persist failure never orphans a listed asset.
    pub async fn remove(&self, id: &str) -> bool {
        let Some(record) = self.get(id) else {
            return false;
        };
        let next: Vec<_> = self
            .read()
            .values()
            .filter(|existing| existing.id != record.id)
            .cloned()
            .collect();
        if let Err(error) = self.save(&next).await {
            tracing::error!(error = %error, "assets: persist failed on remove, keeping entry");
            return false;
        }
        self.write().remove(&record.id);
        let _ = tokio::fs::remove_file(self.assets_dir.join(&record.filename)).await;
        true
    }

    /// Temp file plus rename, so a crash never leaves a partial index.
    async fn save(&self, records: &[AssetRecord]) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(records)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if let Some(parent) = self.index_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let temporary = self.index_path.with_extension("json.tmp");
        tokio::fs::write(&temporary, &json).await?;
        tokio::fs::rename(&temporary, &self.index_path).await
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<String, AssetRecord>> {
        // A poisoned lock only means some writer panicked; the map itself stays
        // structurally valid, so recovering beats taking the process down.
        self.records
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, AssetRecord>> {
        self.records
            .write()
            .unwrap_or_else(|error| error.into_inner())
    }
}

/// A stored filename is always `<id>.<lowercase ascii extension>`.
fn is_safe_stored_filename(id: &str, filename: &str) -> bool {
    let Some(extension) = filename
        .strip_prefix(id)
        .and_then(|rest| rest.strip_prefix('.'))
    else {
        return false;
    };
    !extension.is_empty()
        && extension.len() <= 8
        && extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, kind: AssetKind, extension: &str) -> AssetRecord {
        AssetRecord::new(
            id.to_owned(),
            kind,
            format!("{id}.{extension}"),
            "image/png".into(),
            10,
            "sha".into(),
            1,
        )
    }

    #[test]
    fn ids_follow_the_documented_allow_list() {
        let generated = new_asset_id();
        assert!(generated.starts_with("ast_"));
        assert!(is_valid_asset_id(&generated));
        for rejected in [
            "",
            "ast_",
            "ast_short",
            "../../etc/passwd",
            "ast_../../etc/passwd",
            "ast_aaaaaaaaaaaaaaa.",
            "lut_aaaaaaaaaaaaaaaa",
            "ast_aaaaaaaaaaaaaaaa/../b",
        ] {
            assert!(!is_valid_asset_id(rejected), "{rejected} must be rejected");
        }
    }

    #[test]
    fn magic_bytes_decide_the_content_family() {
        let png = sniff_content(b"\x89PNG\r\n\x1a\n rest").unwrap();
        assert_eq!(png.class, ContentClass::Image);
        assert_eq!(png.extension, "png");
        assert!(png.supports(AssetKind::Image));
        assert!(!png.supports(AssetKind::Video));

        let mp4 = sniff_content(b"\x00\x00\x00\x18ftypisom").unwrap();
        assert_eq!(mp4.class, ContentClass::Container);
        assert!(mp4.supports(AssetKind::Audio) && mp4.supports(AssetKind::Video));

        let font = sniff_content(b"OTTO\x00\x01").unwrap();
        assert!(font.supports(AssetKind::Font));

        assert_eq!(
            sniff_content(b"WEBVTT\n\n00:00.000 --> 00:01.000\nhi\n")
                .unwrap()
                .extension,
            "vtt"
        );
        assert_eq!(
            sniff_content(b"1\n00:00:00,000 --> 00:00:01,000\nhi\n")
                .unwrap()
                .extension,
            "srt"
        );
        // An HTML payload with a .png name is not an image.
        assert!(sniff_content(b"<html><body>hi</body></html>").is_none());
        assert!(sniff_content(b"").is_none());
    }

    #[test]
    fn per_kind_body_caps_match_the_contract() {
        assert_eq!(AssetKind::Video.max_bytes(), MAX_MEDIA_ASSET_BYTES);
        assert_eq!(AssetKind::Image.max_bytes(), MAX_MEDIA_ASSET_BYTES);
        assert_eq!(AssetKind::Font.max_bytes(), MAX_TEXT_ASSET_BYTES);
        assert_eq!(AssetKind::Subtitle.max_bytes(), MAX_TEXT_ASSET_BYTES);
        assert!(AssetKind::Audio.needs_probe() && AssetKind::Video.needs_probe());
        assert!(!AssetKind::Image.needs_probe());
    }

    #[tokio::test]
    async fn resolve_is_kind_scoped_and_survives_a_reload() {
        let directory = tempfile::tempdir().unwrap();
        let storage = directory.path().to_path_buf();
        let id = new_asset_id();
        let store = AssetStore::load(storage.clone());
        assert!(store.insert(record(&id, AssetKind::Image, "png")).await);

        assert_eq!(
            store.resolve(&id, AssetKind::Image),
            Some(storage.join("assets").join(format!("{id}.png")))
        );
        assert!(store.resolve(&id, AssetKind::Video).is_none());
        assert!(store.resolve("../../library", AssetKind::Image).is_none());

        let reloaded = AssetStore::load(storage);
        assert_eq!(reloaded.list().len(), 1);
        assert!(reloaded.resolve(&id, AssetKind::Image).is_some());
    }

    #[tokio::test]
    async fn remove_drops_the_entry_and_its_file() {
        let directory = tempfile::tempdir().unwrap();
        let storage = directory.path().to_path_buf();
        let id = new_asset_id();
        let store = AssetStore::load(storage.clone());
        tokio::fs::create_dir_all(store.assets_dir()).await.unwrap();
        let path = store.assets_dir().join(format!("{id}.png"));
        tokio::fs::write(&path, b"x").await.unwrap();
        store.insert(record(&id, AssetKind::Image, "png")).await;

        assert!(store.remove(&id).await);
        assert!(!store.remove(&id).await);
        assert!(store.get(&id).is_none());
        assert!(tokio::fs::metadata(&path).await.is_err());
    }

    #[tokio::test]
    async fn unsafe_filenames_are_never_indexed() {
        let directory = tempfile::tempdir().unwrap();
        let store = AssetStore::load(directory.path().to_path_buf());
        let id = new_asset_id();
        let mut tampered = record(&id, AssetKind::Image, "png");
        tampered.filename = format!("{id}.png/../../library.json");

        assert!(!store.insert(tampered).await);
        assert!(store.list().is_empty());
    }
}
