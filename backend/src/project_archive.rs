//! Portable, self-contained composition project archives.
//!
//! The `.veproj` wire format is deliberately small and dependency-free:
//! a fixed binary header, canonical bounded JSON manifest, then sequential
//! media records with explicit lengths and SHA-256 digests. Media bytes are
//! always streamed; only the bounded manifest is held in memory.

use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use crate::db::{
    valid_composition_source_id, COMPOSITION_PROJECT_MODE, COMPOSITION_PROJECT_SCHEMA_VERSION,
    MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES, MAX_COMPOSITION_PROJECT_NAME_BYTES,
    MAX_COMPOSITION_PROJECT_SOURCES, MAX_LIBRARY_TAGS, MAX_LIBRARY_TAG_BYTES,
    MAX_LIBRARY_TAG_CHARS, MAX_LIBRARY_TITLE_BYTES, MAX_LIBRARY_TITLE_CHARS,
};

pub const ARCHIVE_MEDIA_TYPE: &str = "application/vnd.video-editor.project";
pub const ARCHIVE_EXTENSION: &str = "veproj";
pub const ARCHIVE_FORMAT_VERSION: u16 = 1;
pub const MAX_ARCHIVE_MANIFEST_BYTES: usize = 3 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_ARCHIVE_TOTAL_MEDIA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = MAX_ARCHIVE_TOTAL_MEDIA_BYTES + 4 * 1024 * 1024;
pub const MAX_ARCHIVE_MULTIPART_BYTES: usize = MAX_ARCHIVE_BYTES as usize + 1024 * 1024;

const ARCHIVE_MAGIC: &[u8; 8] = b"VEPROJ\r\n";
const ARCHIVE_FORMAT_NAME: &str = "veproj";
const ENTRY_MAGIC: &[u8; 4] = b"MED1";
const FIXED_HEADER_BYTES: u64 = 8 + 2 + 2 + 4 + 4 + 8;
const FIXED_ENTRY_HEADER_BYTES: u64 = 4 + 2 + 2 + 8 + 32;
const MAX_ARCHIVE_FILENAME_BYTES: usize = 255;
const COMPOSITION_DOCUMENT_SCHEMA_VERSION: u64 = 1;
const STREAM_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectArchiveManifest {
    pub format: String,
    pub schema_version: u16,
    pub project: ArchivedProject,
    pub sources: Vec<ArchivedSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedProject {
    pub schema_version: u32,
    pub mode: String,
    pub name: String,
    pub document: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchivedSource {
    pub source_id: String,
    pub filename: String,
    pub media_type: String,
    pub title: Option<String>,
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub favorite: bool,
    pub tags: Vec<String>,
    pub byte_length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct ArchiveMediaInput {
    pub source_id: String,
    pub filename: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct ParsedProjectArchive {
    pub manifest: ProjectArchiveManifest,
    pub media: Vec<StagedArchiveMedia>,
}

#[derive(Debug)]
pub struct StagedArchiveMedia {
    source_id: String,
    filename: String,
    path: PathBuf,
}

impl StagedArchiveMedia {
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    pub(crate) fn from_test_file(source_id: &str, filename: &str, path: PathBuf) -> Self {
        Self {
            source_id: source_id.into(),
            filename: filename.into(),
            path,
        }
    }
}

impl Drop for StagedArchiveMedia {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
pub enum ArchiveError {
    Invalid(&'static str),
    Limit(&'static str),
    Io(std::io::Error),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) | Self::Limit(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "archive I/O failed: {error}"),
        }
    }
}

impl std::error::Error for ArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Invalid(_) | Self::Limit(_) => None,
        }
    }
}

impl From<std::io::Error> for ArchiveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl ProjectArchiveManifest {
    pub fn new(project: ArchivedProject, sources: Vec<ArchivedSource>) -> Self {
        Self {
            format: ARCHIVE_FORMAT_NAME.to_owned(),
            schema_version: ARCHIVE_FORMAT_VERSION,
            project,
            sources,
        }
    }
}

pub async fn fingerprint_file(path: &Path) -> Result<(u64, String), ArchiveError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; STREAM_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(ArchiveError::Limit("media size overflow"))?;
        if total > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(ArchiveError::Limit("media entry exceeds the archive limit"));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((total, hex_digest(hasher.finalize().into())))
}

pub async fn write_project_archive<W>(
    mut writer: W,
    manifest: &ProjectArchiveManifest,
    media: &[ArchiveMediaInput],
) -> Result<(), ArchiveError>
where
    W: AsyncWrite + Unpin,
{
    let digests = validate_manifest(manifest)?;
    if media.len() != manifest.sources.len() {
        return Err(ArchiveError::Invalid(
            "media entry count does not match manifest",
        ));
    }
    let manifest_bytes = serde_json::to_vec(manifest)
        .map_err(|_| ArchiveError::Invalid("manifest cannot be serialized"))?;
    if manifest_bytes.len() > MAX_ARCHIVE_MANIFEST_BYTES {
        return Err(ArchiveError::Limit("manifest exceeds the archive limit"));
    }
    validate_archive_size(manifest, manifest_bytes.len())?;

    writer.write_all(ARCHIVE_MAGIC).await?;
    writer
        .write_all(&ARCHIVE_FORMAT_VERSION.to_be_bytes())
        .await?;
    writer.write_all(&0_u16.to_be_bytes()).await?;
    writer
        .write_all(&(manifest_bytes.len() as u32).to_be_bytes())
        .await?;
    writer
        .write_all(&(manifest.sources.len() as u32).to_be_bytes())
        .await?;
    writer
        .write_all(&total_media_bytes(manifest)?.to_be_bytes())
        .await?;
    writer.write_all(&manifest_bytes).await?;

    for ((source, input), digest) in manifest.sources.iter().zip(media).zip(digests) {
        if source.source_id != input.source_id || source.filename != input.filename {
            return Err(ArchiveError::Invalid(
                "media entry order does not match manifest",
            ));
        }
        writer.write_all(ENTRY_MAGIC).await?;
        writer
            .write_all(&(source.source_id.len() as u16).to_be_bytes())
            .await?;
        writer
            .write_all(&(source.filename.len() as u16).to_be_bytes())
            .await?;
        writer.write_all(&source.byte_length.to_be_bytes()).await?;
        writer.write_all(&digest).await?;
        writer.write_all(source.source_id.as_bytes()).await?;
        writer.write_all(source.filename.as_bytes()).await?;

        let mut input_file = tokio::fs::File::open(&input.path).await?;
        copy_exact_and_verify(&mut input_file, &mut writer, source.byte_length, &digest).await?;
        let mut trailing = [0_u8; 1];
        if input_file.read(&mut trailing).await? != 0 {
            return Err(ArchiveError::Invalid(
                "media changed while archive was written",
            ));
        }
    }
    writer.flush().await?;
    Ok(())
}

pub async fn read_project_archive<R>(
    mut reader: R,
    staging_dir: &Path,
) -> Result<ParsedProjectArchive, ArchiveError>
where
    R: AsyncRead + Unpin,
{
    let mut magic = [0_u8; 8];
    read_exact_archive(&mut reader, &mut magic).await?;
    if &magic != ARCHIVE_MAGIC {
        return Err(ArchiveError::Invalid("invalid archive magic"));
    }
    let version = read_u16(&mut reader).await?;
    let flags = read_u16(&mut reader).await?;
    if version != ARCHIVE_FORMAT_VERSION || flags != 0 {
        return Err(ArchiveError::Invalid(
            "unsupported archive version or flags",
        ));
    }
    let manifest_length = read_u32(&mut reader).await? as usize;
    let entry_count = read_u32(&mut reader).await? as usize;
    let declared_total_media = read_u64(&mut reader).await?;
    if manifest_length > MAX_ARCHIVE_MANIFEST_BYTES {
        return Err(ArchiveError::Limit("manifest exceeds the archive limit"));
    }
    if entry_count > MAX_COMPOSITION_PROJECT_SOURCES {
        return Err(ArchiveError::Limit(
            "archive contains too many media entries",
        ));
    }
    if declared_total_media > MAX_ARCHIVE_TOTAL_MEDIA_BYTES {
        return Err(ArchiveError::Limit("archive media exceeds the total limit"));
    }

    let mut manifest_bytes = vec![0_u8; manifest_length];
    read_exact_archive(&mut reader, &mut manifest_bytes).await?;
    let manifest: ProjectArchiveManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| ArchiveError::Invalid("manifest is not valid JSON"))?;
    let canonical = serde_json::to_vec(&manifest)
        .map_err(|_| ArchiveError::Invalid("manifest cannot be canonicalized"))?;
    if canonical != manifest_bytes {
        return Err(ArchiveError::Invalid("manifest JSON is not canonical"));
    }
    let digests = validate_manifest(&manifest)?;
    if manifest.sources.len() != entry_count
        || total_media_bytes(&manifest)? != declared_total_media
    {
        return Err(ArchiveError::Invalid(
            "archive header does not match manifest",
        ));
    }
    validate_archive_size(&manifest, manifest_length)?;

    let mut media = Vec::with_capacity(entry_count);
    for (source, expected_digest) in manifest.sources.iter().zip(digests) {
        let mut entry_magic = [0_u8; 4];
        read_exact_archive(&mut reader, &mut entry_magic).await?;
        if &entry_magic != ENTRY_MAGIC {
            return Err(ArchiveError::Invalid("invalid media entry marker"));
        }
        let source_id_length = read_u16(&mut reader).await? as usize;
        let filename_length = read_u16(&mut reader).await? as usize;
        let media_length = read_u64(&mut reader).await?;
        let mut entry_digest = [0_u8; 32];
        read_exact_archive(&mut reader, &mut entry_digest).await?;
        if source_id_length != source.source_id.len()
            || filename_length != source.filename.len()
            || media_length != source.byte_length
            || entry_digest != expected_digest
        {
            return Err(ArchiveError::Invalid(
                "media entry header does not match manifest",
            ));
        }
        let source_id = read_utf8(&mut reader, source_id_length).await?;
        let filename = read_utf8(&mut reader, filename_length).await?;
        if source_id != source.source_id || filename != source.filename {
            return Err(ArchiveError::Invalid(
                "media entry identity does not match manifest",
            ));
        }

        let path = staging_dir.join(format!("{}.veproj-media", Uuid::new_v4()));
        let staged_media = StagedArchiveMedia {
            source_id,
            filename,
            path,
        };
        let mut staged = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staged_media.path())
            .await?;
        let mut hasher = Sha256::new();
        copy_exact_and_hash(&mut reader, &mut staged, media_length, &mut hasher).await?;
        staged.flush().await?;
        drop(staged);
        if <[u8; 32]>::from(hasher.finalize()) != expected_digest {
            return Err(ArchiveError::Invalid(
                "media checksum does not match manifest",
            ));
        }
        media.push(staged_media);
    }

    let mut trailing = [0_u8; 1];
    if reader.read(&mut trailing).await? != 0 {
        return Err(ArchiveError::Invalid("archive has trailing bytes"));
    }
    Ok(ParsedProjectArchive { manifest, media })
}

fn validate_manifest(manifest: &ProjectArchiveManifest) -> Result<Vec<[u8; 32]>, ArchiveError> {
    if manifest.format != ARCHIVE_FORMAT_NAME
        || manifest.schema_version != ARCHIVE_FORMAT_VERSION
        || manifest.project.schema_version != COMPOSITION_PROJECT_SCHEMA_VERSION
        || manifest.project.mode != COMPOSITION_PROJECT_MODE
    {
        return Err(ArchiveError::Invalid("unsupported manifest envelope"));
    }
    let name = &manifest.project.name;
    if name.trim().is_empty()
        || name != name.trim()
        || name.len() > MAX_COMPOSITION_PROJECT_NAME_BYTES
        || name.chars().any(char::is_control)
    {
        return Err(ArchiveError::Invalid("invalid project name"));
    }
    let document_bytes = serde_json::to_vec(&manifest.project.document)
        .map_err(|_| ArchiveError::Invalid("project document cannot be serialized"))?;
    if document_bytes.len() > MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES {
        return Err(ArchiveError::Limit("project document exceeds its limit"));
    }
    if manifest.sources.len() > MAX_COMPOSITION_PROJECT_SOURCES {
        return Err(ArchiveError::Limit(
            "archive contains too many media entries",
        ));
    }

    let mut ids: HashSet<&str> = HashSet::with_capacity(manifest.sources.len());
    let mut filenames = HashSet::with_capacity(manifest.sources.len());
    let mut digests = Vec::with_capacity(manifest.sources.len());
    for source in &manifest.sources {
        if !valid_composition_source_id(&source.source_id) || !ids.insert(source.source_id.as_str())
        {
            return Err(ArchiveError::Invalid("duplicate or invalid source id"));
        }
        if !safe_archive_filename(&source.filename)
            || !filenames.insert(source.filename.to_ascii_lowercase())
        {
            return Err(ArchiveError::Invalid("duplicate or unsafe media filename"));
        }
        if !matches!(source.media_type.as_str(), "video" | "audio" | "image") {
            return Err(ArchiveError::Invalid("invalid media type"));
        }
        validate_title(source.title.as_deref())?;
        validate_tags(&source.tags)?;
        if source.byte_length == 0 || source.byte_length > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(ArchiveError::Limit("media entry exceeds its size limit"));
        }
        if source
            .duration
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(ArchiveError::Invalid("invalid media duration"));
        }
        digests.push(parse_digest(&source.sha256)?);
    }
    let document_source_ids = document_source_ids(&manifest.project.document)?;
    let manifest_source_ids: Vec<_> = manifest
        .sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect();
    if document_source_ids != manifest_source_ids {
        return Err(ArchiveError::Invalid(
            "manifest sources do not match project document",
        ));
    }
    validate_document_references(&manifest.project.document, &ids)?;
    let _ = total_media_bytes(manifest)?;
    Ok(digests)
}

fn document_source_ids(document: &Value) -> Result<Vec<&str>, ArchiveError> {
    let document = document.as_object().ok_or(ArchiveError::Invalid(
        "composition document is not an object",
    ))?;
    if document.get("schemaVersion").and_then(Value::as_u64)
        != Some(COMPOSITION_DOCUMENT_SCHEMA_VERSION)
    {
        return Err(ArchiveError::Invalid(
            "unsupported composition document schema",
        ));
    }
    let sources =
        document
            .get("sources")
            .and_then(Value::as_object)
            .ok_or(ArchiveError::Invalid(
                "composition sources are not an object",
            ))?;
    let mut ids = Vec::with_capacity(sources.len());
    for (source_id, source) in sources {
        if !valid_composition_source_id(source_id)
            || source
                .get("id")
                .and_then(Value::as_str)
                .is_none_or(|embedded| embedded != source_id)
        {
            return Err(ArchiveError::Invalid("invalid composition source identity"));
        }
        ids.push(source_id.as_str());
    }
    Ok(ids)
}

fn validate_document_references(
    value: &Value,
    source_ids: &HashSet<&str>,
) -> Result<(), ArchiveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if key == "sourceId" {
                    let source_id = child
                        .as_str()
                        .ok_or(ArchiveError::Invalid("clip sourceId is not a string"))?;
                    if !source_ids.contains(source_id) {
                        return Err(ArchiveError::Invalid("clip references an unknown source"));
                    }
                }
                validate_document_references(child, source_ids)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_document_references(item, source_ids)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_title(title: Option<&str>) -> Result<(), ArchiveError> {
    if let Some(title) = title {
        if title.is_empty()
            || title != title.trim()
            || title.len() > MAX_LIBRARY_TITLE_BYTES
            || title.chars().count() > MAX_LIBRARY_TITLE_CHARS
            || title.chars().any(char::is_control)
        {
            return Err(ArchiveError::Invalid("invalid library title"));
        }
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> Result<(), ArchiveError> {
    if tags.len() > MAX_LIBRARY_TAGS {
        return Err(ArchiveError::Invalid("too many library tags"));
    }
    let mut unique = HashSet::with_capacity(tags.len());
    for tag in tags {
        if tag.is_empty()
            || tag != tag.trim()
            || tag.len() > MAX_LIBRARY_TAG_BYTES
            || tag.chars().count() > MAX_LIBRARY_TAG_CHARS
            || tag.chars().any(char::is_control)
            || !unique.insert(tag.to_lowercase())
        {
            return Err(ArchiveError::Invalid("invalid or duplicate library tag"));
        }
    }
    Ok(())
}

pub fn safe_archive_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename.len() <= MAX_ARCHIVE_FILENAME_BYTES
        && filename
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        && filename.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && matches!(
            Path::new(filename)
                .components()
                .collect::<Vec<_>>()
                .as_slice(),
            [Component::Normal(_)]
        )
}

fn validate_archive_size(
    manifest: &ProjectArchiveManifest,
    manifest_length: usize,
) -> Result<u64, ArchiveError> {
    let mut total = FIXED_HEADER_BYTES
        .checked_add(manifest_length as u64)
        .ok_or(ArchiveError::Limit("archive size overflow"))?;
    for source in &manifest.sources {
        total = total
            .checked_add(FIXED_ENTRY_HEADER_BYTES)
            .and_then(|value| value.checked_add(source.source_id.len() as u64))
            .and_then(|value| value.checked_add(source.filename.len() as u64))
            .and_then(|value| value.checked_add(source.byte_length))
            .ok_or(ArchiveError::Limit("archive size overflow"))?;
    }
    if total > MAX_ARCHIVE_BYTES {
        return Err(ArchiveError::Limit("archive exceeds the total size limit"));
    }
    Ok(total)
}

fn total_media_bytes(manifest: &ProjectArchiveManifest) -> Result<u64, ArchiveError> {
    let total = manifest.sources.iter().try_fold(0_u64, |total, source| {
        total
            .checked_add(source.byte_length)
            .ok_or(ArchiveError::Limit("archive media size overflow"))
    })?;
    if total > MAX_ARCHIVE_TOTAL_MEDIA_BYTES {
        return Err(ArchiveError::Limit("archive media exceeds the total limit"));
    }
    Ok(total)
}

fn parse_digest(value: &str) -> Result<[u8; 32], ArchiveError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ArchiveError::Invalid("invalid SHA-256 digest"));
    }
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| ArchiveError::Invalid("invalid SHA-256 digest"))?;
    }
    if hex_digest(digest) != value {
        return Err(ArchiveError::Invalid("SHA-256 digest must be lowercase"));
    }
    Ok(digest)
}

fn hex_digest(digest: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

async fn copy_exact_and_verify<R, W>(
    reader: &mut R,
    writer: &mut W,
    length: u64,
    expected_digest: &[u8; 32],
) -> Result<(), ArchiveError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut hasher = Sha256::new();
    copy_exact_and_hash(reader, writer, length, &mut hasher).await?;
    if <[u8; 32]>::from(hasher.finalize()) != *expected_digest {
        return Err(ArchiveError::Invalid(
            "media changed while archive was written",
        ));
    }
    Ok(())
}

async fn copy_exact_and_hash<R, W>(
    reader: &mut R,
    writer: &mut W,
    mut remaining: u64,
    hasher: &mut Sha256,
) -> Result<(), ArchiveError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0_u8; STREAM_BUFFER_BYTES];
    while remaining > 0 {
        let limit = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ArchiveError::Limit("media length does not fit this platform"))?;
        let read = reader.read(&mut buffer[..limit]).await?;
        if read == 0 {
            return Err(ArchiveError::Invalid("archive or media entry is truncated"));
        }
        writer.write_all(&buffer[..read]).await?;
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(())
}

async fn read_exact_archive<R>(reader: &mut R, bytes: &mut [u8]) -> Result<(), ArchiveError>
where
    R: AsyncRead + Unpin,
{
    match reader.read_exact(bytes).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err(ArchiveError::Invalid("archive is truncated"))
        }
        Err(error) => Err(ArchiveError::Io(error)),
    }
}

async fn read_u16<R>(reader: &mut R) -> Result<u16, ArchiveError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = [0_u8; 2];
    read_exact_archive(reader, &mut bytes).await?;
    Ok(u16::from_be_bytes(bytes))
}

async fn read_u32<R>(reader: &mut R) -> Result<u32, ArchiveError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = [0_u8; 4];
    read_exact_archive(reader, &mut bytes).await?;
    Ok(u32::from_be_bytes(bytes))
}

async fn read_u64<R>(reader: &mut R) -> Result<u64, ArchiveError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = [0_u8; 8];
    read_exact_archive(reader, &mut bytes).await?;
    Ok(u64::from_be_bytes(bytes))
}

async fn read_utf8<R>(reader: &mut R, length: usize) -> Result<String, ArchiveError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = vec![0_u8; length];
    read_exact_archive(reader, &mut bytes).await?;
    String::from_utf8(bytes).map_err(|_| ArchiveError::Invalid("entry identity is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn project(source_id: &str) -> ArchivedProject {
        ArchivedProject {
            schema_version: COMPOSITION_PROJECT_SCHEMA_VERSION,
            mode: COMPOSITION_PROJECT_MODE.into(),
            name: "Portable cut".into(),
            document: json!({
                "schemaVersion": 1,
                "sources": {(source_id): {"id": source_id}},
                "tracks": [{"clips": [{"sourceId": source_id}]}]
            }),
        }
    }

    async fn fixture() -> (
        tempfile::TempDir,
        ProjectArchiveManifest,
        Vec<ArchiveMediaInput>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.wav");
        tokio::fs::write(&path, b"portable-media").await.unwrap();
        let (byte_length, sha256) = fingerprint_file(&path).await.unwrap();
        let manifest = ProjectArchiveManifest::new(
            project("source-a"),
            vec![ArchivedSource {
                source_id: "source-a".into(),
                filename: "source.wav".into(),
                media_type: "audio".into(),
                title: Some("Voice".into()),
                duration: Some(1.0),
                width: None,
                height: None,
                favorite: true,
                tags: vec!["dialogue".into()],
                byte_length,
                sha256,
            }],
        );
        let inputs = vec![ArchiveMediaInput {
            source_id: "source-a".into(),
            filename: "source.wav".into(),
            path,
        }];
        (directory, manifest, inputs)
    }

    #[tokio::test]
    async fn golden_archive_is_deterministic_and_round_trips_streaming_media() {
        let (directory, manifest, inputs) = fixture().await;
        let first = directory.path().join("first.veproj");
        let second = directory.path().join("second.veproj");
        write_project_archive(
            tokio::fs::File::create(&first).await.unwrap(),
            &manifest,
            &inputs,
        )
        .await
        .unwrap();
        write_project_archive(
            tokio::fs::File::create(&second).await.unwrap(),
            &manifest,
            &inputs,
        )
        .await
        .unwrap();
        let first_bytes = tokio::fs::read(&first).await.unwrap();
        assert_eq!(first_bytes, tokio::fs::read(&second).await.unwrap());
        assert_eq!(&first_bytes[..8], ARCHIVE_MAGIC);
        assert_eq!(
            hex_digest(Sha256::digest(&first_bytes).into()),
            "db08399c9da6be05d52055e1eaba32fa64671ae24e457dd32b7413097ecd373f"
        );

        let parsed = read_project_archive(
            tokio::fs::File::open(first).await.unwrap(),
            directory.path(),
        )
        .await
        .unwrap();
        assert_eq!(parsed.manifest, manifest);
        assert_eq!(parsed.media[0].source_id(), "source-a");
        assert_eq!(
            tokio::fs::read(parsed.media[0].path()).await.unwrap(),
            b"portable-media"
        );
    }

    #[tokio::test]
    async fn parser_rejects_checksum_tampering_and_trailing_bytes() {
        let (directory, manifest, inputs) = fixture().await;
        let archive = directory.path().join("archive.veproj");
        write_project_archive(
            tokio::fs::File::create(&archive).await.unwrap(),
            &manifest,
            &inputs,
        )
        .await
        .unwrap();
        let mut bytes = tokio::fs::read(&archive).await.unwrap();
        *bytes.last_mut().unwrap() ^= 0x01;
        let tampered = directory.path().join("tampered.veproj");
        tokio::fs::write(&tampered, &bytes).await.unwrap();
        assert!(matches!(
            read_project_archive(
                tokio::fs::File::open(tampered).await.unwrap(),
                directory.path()
            )
            .await,
            Err(ArchiveError::Invalid(
                "media checksum does not match manifest"
            ))
        ));

        let valid = directory.path().join("valid.veproj");
        write_project_archive(
            tokio::fs::File::create(&valid).await.unwrap(),
            &manifest,
            &inputs,
        )
        .await
        .unwrap();
        let mut trailing = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&valid)
            .await
            .unwrap();
        trailing.write_all(b"x").await.unwrap();
        drop(trailing);
        assert!(matches!(
            read_project_archive(
                tokio::fs::File::open(valid).await.unwrap(),
                directory.path()
            )
            .await,
            Err(ArchiveError::Invalid("archive has trailing bytes"))
        ));
    }

    #[tokio::test]
    async fn parser_rejects_noncanonical_json_version_and_preallocation_limit_bypass() {
        let (directory, manifest, inputs) = fixture().await;
        let valid = directory.path().join("canonical.veproj");
        write_project_archive(
            tokio::fs::File::create(&valid).await.unwrap(),
            &manifest,
            &inputs,
        )
        .await
        .unwrap();
        let bytes = tokio::fs::read(&valid).await.unwrap();
        let manifest_length = u32::from_be_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let pretty_manifest = serde_json::to_vec_pretty(&manifest).unwrap();
        let mut noncanonical = Vec::new();
        noncanonical.extend_from_slice(&bytes[..12]);
        noncanonical.extend_from_slice(&(pretty_manifest.len() as u32).to_be_bytes());
        noncanonical.extend_from_slice(&bytes[16..28]);
        noncanonical.extend_from_slice(&pretty_manifest);
        noncanonical.extend_from_slice(&bytes[28 + manifest_length..]);
        let noncanonical_path = directory.path().join("noncanonical.veproj");
        tokio::fs::write(&noncanonical_path, noncanonical)
            .await
            .unwrap();
        assert!(matches!(
            read_project_archive(
                tokio::fs::File::open(noncanonical_path).await.unwrap(),
                directory.path()
            )
            .await,
            Err(ArchiveError::Invalid("manifest JSON is not canonical"))
        ));

        let mut unsupported = bytes.clone();
        unsupported[8..10].copy_from_slice(&2_u16.to_be_bytes());
        let unsupported_path = directory.path().join("unsupported.veproj");
        tokio::fs::write(&unsupported_path, unsupported)
            .await
            .unwrap();
        assert!(matches!(
            read_project_archive(
                tokio::fs::File::open(unsupported_path).await.unwrap(),
                directory.path()
            )
            .await,
            Err(ArchiveError::Invalid(
                "unsupported archive version or flags"
            ))
        ));

        let mut oversized_header = Vec::new();
        oversized_header.extend_from_slice(ARCHIVE_MAGIC);
        oversized_header.extend_from_slice(&ARCHIVE_FORMAT_VERSION.to_be_bytes());
        oversized_header.extend_from_slice(&0_u16.to_be_bytes());
        oversized_header
            .extend_from_slice(&((MAX_ARCHIVE_MANIFEST_BYTES + 1) as u32).to_be_bytes());
        oversized_header.extend_from_slice(&0_u32.to_be_bytes());
        oversized_header.extend_from_slice(&0_u64.to_be_bytes());
        assert!(matches!(
            read_project_archive(std::io::Cursor::new(oversized_header), directory.path()).await,
            Err(ArchiveError::Limit("manifest exceeds the archive limit"))
        ));
    }

    #[tokio::test]
    async fn manifest_rejects_duplicate_ids_and_traversal_filenames() {
        let (_directory, mut manifest, _inputs) = fixture().await;
        let mut duplicate = manifest.sources[0].clone();
        duplicate.filename = "other.wav".into();
        manifest.sources.push(duplicate);
        assert!(matches!(
            validate_manifest(&manifest),
            Err(ArchiveError::Invalid("duplicate or invalid source id"))
        ));

        manifest.sources.pop();
        manifest.sources[0].filename = "../source.wav".into();
        assert!(matches!(
            validate_manifest(&manifest),
            Err(ArchiveError::Invalid("duplicate or unsafe media filename"))
        ));

        let (_directory, mut manifest, _inputs) = fixture().await;
        manifest.project.document["sources"]["source-b"] = json!({"id": "source-b"});
        let mut second = manifest.sources[0].clone();
        second.source_id = "source-b".into();
        manifest.sources.push(second);
        assert!(matches!(
            validate_manifest(&manifest),
            Err(ArchiveError::Invalid("duplicate or unsafe media filename"))
        ));
    }
}
