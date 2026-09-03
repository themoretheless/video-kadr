use anyhow::{bail, ensure, Context, Result};
use reqwest::{header, Client, StatusCode};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use url::Url;

pub const YOUTUBE_UPLOAD_SCOPE: &str = "https://www.googleapis.com/auth/youtube.upload";

#[derive(Clone)]
pub struct TokenCipher {
    key: aead::LessSafeKey,
}

impl TokenCipher {
    pub fn new(key: [u8; 32]) -> Result<Self> {
        let key = aead::UnboundKey::new(&aead::AES_256_GCM, &key)
            .map_err(|_| anyhow::anyhow!("invalid OAuth token encryption key"))?;
        Ok(Self {
            key: aead::LessSafeKey::new(key),
        })
    }

    pub fn encrypt(&self, actor: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0_u8; 12];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| anyhow::anyhow!("generate OAuth token nonce"))?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = plaintext.to_vec();
        self.key
            .seal_in_place_append_tag(nonce, aead::Aad::from(actor.as_bytes()), &mut ciphertext)
            .map_err(|_| anyhow::anyhow!("encrypt OAuth tokens"))?;
        let mut sealed = nonce_bytes.to_vec();
        sealed.extend(ciphertext);
        Ok(sealed)
    }

    pub fn decrypt(&self, actor: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            sealed.len() >= 12 + aead::AES_256_GCM.tag_len(),
            "invalid encrypted OAuth token payload"
        );
        let nonce_bytes: [u8; 12] = sealed[..12].try_into()?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = sealed[12..].to_vec();
        let plaintext = self
            .key
            .open_in_place(nonce, aead::Aad::from(actor.as_bytes()), &mut ciphertext)
            .map_err(|_| anyhow::anyhow!("decrypt OAuth tokens"))?;
        Ok(plaintext.to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YouTubeOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Clone)]
pub struct YouTubeOAuthClient {
    client: Client,
    config: YouTubeOAuthConfig,
    authorization_endpoint: Url,
    token_endpoint: Url,
    upload_endpoint: Url,
    revoke_endpoint: Url,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct YouTubeTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub scope: String,
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: Option<String>,
    token_type: String,
}

#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YouTubeUploadMetadata {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_privacy_status")]
    pub privacy_status: String,
}

impl YouTubeUploadMetadata {
    pub fn validate(&self) -> Result<()> {
        validate_upload_metadata(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeUploadCheckpoint {
    pub session_url: String,
    pub total_bytes: u64,
    pub confirmed_offset: u64,
}

#[derive(Debug, Deserialize)]
struct UploadedVideo {
    id: String,
}

const UPLOAD_CHUNK_BYTES: usize = 8 * 1024 * 1024;

struct UploadContext<'a> {
    access_token: &'a str,
    path: &'a std::path::Path,
    mime_type: &'a str,
    total: u64,
    progress: &'a tokio::sync::mpsc::UnboundedSender<f64>,
    cancellation: &'a tokio_util::sync::CancellationToken,
    checkpoints: &'a tokio::sync::mpsc::UnboundedSender<YouTubeUploadCheckpoint>,
}

pub(crate) struct YouTubeUploadRequest<'a> {
    pub access_token: &'a str,
    pub path: &'a std::path::Path,
    pub mime_type: &'a str,
    pub progress: &'a tokio::sync::mpsc::UnboundedSender<f64>,
    pub cancellation: &'a tokio_util::sync::CancellationToken,
    pub checkpoints: &'a tokio::sync::mpsc::UnboundedSender<YouTubeUploadCheckpoint>,
}

fn default_privacy_status() -> String {
    "private".into()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeConnectionStatus {
    pub configured: bool,
    pub connected: bool,
}

impl YouTubeOAuthClient {
    pub fn new(config: YouTubeOAuthConfig) -> Result<Self> {
        ensure!(!config.client_id.is_empty(), "YouTube client id is empty");
        ensure!(
            !config.client_secret.is_empty(),
            "YouTube client secret is empty"
        );
        validate_redirect_uri(&config.redirect_uri)?;
        Ok(Self {
            client: Client::builder()
                .user_agent("video-kadr/0.1 youtube-publisher")
                .build()?,
            config,
            authorization_endpoint: Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?,
            token_endpoint: Url::parse("https://oauth2.googleapis.com/token")?,
            upload_endpoint: Url::parse(
                "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status",
            )?,
            revoke_endpoint: Url::parse("https://oauth2.googleapis.com/revoke")?,
        })
    }

    pub fn authorization_url(&self, state: &str) -> Result<String> {
        let mut url = self.authorization_endpoint.clone();
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", YOUTUBE_UPLOAD_SCOPE)
            .append_pair("access_type", "offline")
            .append_pair("include_granted_scopes", "true")
            .append_pair("prompt", "consent")
            .append_pair("state", state);
        Ok(url.into())
    }

    pub async fn exchange_code(&self, code: &str) -> Result<YouTubeTokens> {
        ensure!(
            !code.trim().is_empty(),
            "YouTube authorization code is empty"
        );
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&[
                ("code", code),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("redirect_uri", self.config.redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .context("send YouTube OAuth token exchange")?;
        self.decode_token_response(response).await
    }

    pub async fn refresh_access_token(&self, refresh_token: &str) -> Result<YouTubeTokens> {
        ensure!(
            !refresh_token.trim().is_empty(),
            "YouTube refresh token is empty"
        );
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&[
                ("refresh_token", refresh_token),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("send YouTube OAuth token refresh")?;
        let mut tokens = self.decode_token_response(response).await?;
        tokens.refresh_token = Some(refresh_token.into());
        Ok(tokens)
    }

    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        ensure!(!token.trim().is_empty(), "YouTube revoke token is empty");
        let response = self
            .client
            .post(self.revoke_endpoint.clone())
            .form(&[("token", token)])
            .send()
            .await
            .context("send YouTube OAuth revocation")?;
        ensure!(
            response.status().is_success(),
            "YouTube OAuth revocation failed ({})",
            response.status()
        );
        Ok(())
    }

    async fn decode_token_response(&self, response: reqwest::Response) -> Result<YouTubeTokens> {
        if !response.status().is_success() {
            let status = response.status();
            let error = response
                .json::<OAuthErrorResponse>()
                .await
                .unwrap_or(OAuthErrorResponse {
                    error: None,
                    error_description: None,
                });
            bail!(
                "YouTube OAuth token exchange failed ({status}): {}",
                error
                    .error_description
                    .or(error.error)
                    .unwrap_or_else(|| "provider rejected request".into())
            );
        }
        let token = response
            .json::<TokenResponse>()
            .await
            .context("decode YouTube OAuth token response")?;
        ensure!(
            token.token_type.eq_ignore_ascii_case("Bearer"),
            "YouTube OAuth returned an unsupported token type"
        );
        let scope = token.scope.unwrap_or_else(|| YOUTUBE_UPLOAD_SCOPE.into());
        ensure!(
            scope
                .split_whitespace()
                .any(|value| value == YOUTUBE_UPLOAD_SCOPE),
            "YouTube OAuth response did not grant the upload scope"
        );
        Ok(YouTubeTokens {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: crate::library::now_secs().saturating_add(token.expires_in),
            scope,
            token_type: token.token_type,
        })
    }

    pub(crate) async fn upload_resumable(
        &self,
        request: YouTubeUploadRequest<'_>,
        metadata: &YouTubeUploadMetadata,
    ) -> Result<String> {
        validate_upload_metadata(metadata)?;
        ensure!(
            !request.access_token.is_empty(),
            "YouTube access token is empty"
        );
        ensure!(
            request.mime_type.starts_with("video/"),
            "YouTube upload requires a video MIME type"
        );
        let total = tokio::fs::metadata(request.path).await?.len();
        ensure!(total > 0, "YouTube upload file is empty");
        let body = serde_json::json!({
            "snippet": { "title": metadata.title, "description": metadata.description },
            "status": { "privacyStatus": metadata.privacy_status }
        });
        let response_future = self
            .client
            .post(self.upload_endpoint.clone())
            .bearer_auth(request.access_token)
            .header("X-Upload-Content-Length", total)
            .header("X-Upload-Content-Type", request.mime_type)
            .json(&body)
            .send();
        let response = tokio::select! {
            response = response_future => response.context("start YouTube resumable upload")?,
            _ = request.cancellation.cancelled() => return Ok(String::new()),
        };
        ensure!(
            response.status().is_success(),
            "YouTube rejected resumable upload initialization ({})",
            response.status()
        );
        let location = response
            .headers()
            .get(header::LOCATION)
            .context("YouTube resumable upload response omitted Location")?
            .to_str()
            .context("YouTube resumable Location is not valid text")?;
        let session = validate_upload_session_url(location)?;
        let _ = request.checkpoints.send(YouTubeUploadCheckpoint {
            session_url: session.to_string(),
            total_bytes: total,
            confirmed_offset: 0,
        });
        self.upload_session(
            UploadContext {
                access_token: request.access_token,
                path: request.path,
                mime_type: request.mime_type,
                total,
                progress: request.progress,
                cancellation: request.cancellation,
                checkpoints: request.checkpoints,
            },
            session,
            0,
        )
        .await
    }

    pub(crate) async fn resume_resumable(
        &self,
        request: YouTubeUploadRequest<'_>,
        checkpoint: &YouTubeUploadCheckpoint,
    ) -> Result<Option<String>> {
        let total = tokio::fs::metadata(request.path).await?.len();
        ensure!(
            total == checkpoint.total_bytes && checkpoint.confirmed_offset <= total,
            "YouTube upload checkpoint does not match the output file"
        );
        let session = validate_upload_session_url(&checkpoint.session_url)?;
        let response_future = self
            .client
            .put(session.clone())
            .bearer_auth(request.access_token)
            .header(header::CONTENT_LENGTH, 0)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Vec::new())
            .send();
        let response = tokio::select! {
            response = response_future => response.context("query YouTube resumable upload status")?,
            _ = request.cancellation.cancelled() => return Ok(Some(String::new())),
        };
        if response.status().is_success() {
            return decode_uploaded_video(response).await.map(Some);
        }
        if matches!(response.status(), StatusCode::NOT_FOUND | StatusCode::GONE) {
            return Ok(None);
        }
        ensure!(
            response.status() == StatusCode::PERMANENT_REDIRECT,
            "YouTube resumable status query failed ({})",
            response.status()
        );
        let offset = response
            .headers()
            .get(header::RANGE)
            .map(|_| confirmed_offset(response.headers(), total))
            .transpose()?
            .unwrap_or(0);
        let _ = request.checkpoints.send(YouTubeUploadCheckpoint {
            session_url: session.to_string(),
            total_bytes: total,
            confirmed_offset: offset,
        });
        let _ = request.progress.send(offset as f64 * 100.0 / total as f64);
        self.upload_session(
            UploadContext {
                access_token: request.access_token,
                path: request.path,
                mime_type: request.mime_type,
                total,
                progress: request.progress,
                cancellation: request.cancellation,
                checkpoints: request.checkpoints,
            },
            session,
            offset,
        )
        .await
        .map(Some)
    }

    async fn upload_session(
        &self,
        context: UploadContext<'_>,
        session: Url,
        start_offset: u64,
    ) -> Result<String> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut file = tokio::fs::File::open(context.path).await?;
        let mut offset = start_offset;
        while offset < context.total {
            file.seek(std::io::SeekFrom::Start(offset)).await?;
            let length = usize::try_from((context.total - offset).min(UPLOAD_CHUNK_BYTES as u64))?;
            let mut chunk = vec![0_u8; length];
            file.read_exact(&mut chunk).await?;
            let end = offset + length as u64 - 1;
            let request = self
                .client
                .put(session.clone())
                .bearer_auth(context.access_token)
                .header(header::CONTENT_TYPE, context.mime_type)
                .header(header::CONTENT_LENGTH, length)
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {offset}-{end}/{}", context.total),
                )
                .body(chunk)
                .send();
            let response = tokio::select! {
                response = request => response.context("upload YouTube video chunk")?,
                _ = context.cancellation.cancelled() => return Ok(String::new()),
            };
            if response.status().is_success() {
                let id = decode_uploaded_video(response).await?;
                let _ = context.progress.send(100.0);
                return Ok(id);
            }
            ensure!(
                response.status() == StatusCode::PERMANENT_REDIRECT,
                "YouTube chunk upload failed ({})",
                response.status()
            );
            let confirmed = confirmed_offset(response.headers(), context.total)?;
            ensure!(
                confirmed > offset,
                "YouTube upload did not advance its confirmed offset"
            );
            offset = confirmed;
            let _ = context.checkpoints.send(YouTubeUploadCheckpoint {
                session_url: session.to_string(),
                total_bytes: context.total,
                confirmed_offset: offset,
            });
            let _ = context
                .progress
                .send(offset as f64 * 100.0 / context.total as f64);
        }
        bail!("YouTube upload ended without a video id")
    }

    #[cfg(test)]
    fn with_token_endpoint(mut self, endpoint: Url) -> Self {
        self.token_endpoint = endpoint;
        self
    }

    #[cfg(test)]
    fn with_upload_endpoint(mut self, endpoint: Url) -> Self {
        self.upload_endpoint = endpoint;
        self
    }

    #[cfg(test)]
    fn with_revoke_endpoint(mut self, endpoint: Url) -> Self {
        self.revoke_endpoint = endpoint;
        self
    }
}

async fn decode_uploaded_video(response: reqwest::Response) -> Result<String> {
    let uploaded = response
        .json::<UploadedVideo>()
        .await
        .context("decode uploaded YouTube video")?;
    ensure!(
        !uploaded.id.trim().is_empty(),
        "YouTube upload returned an empty video id"
    );
    Ok(uploaded.id)
}

fn validate_upload_metadata(metadata: &YouTubeUploadMetadata) -> Result<()> {
    let title = metadata.title.trim();
    ensure!(
        !title.is_empty() && title.chars().count() <= 100,
        "YouTube title must contain 1 to 100 characters"
    );
    ensure!(
        metadata.description.chars().count() <= 5_000,
        "YouTube description exceeds 5000 characters"
    );
    ensure!(
        matches!(
            metadata.privacy_status.as_str(),
            "private" | "unlisted" | "public"
        ),
        "unsupported YouTube privacy status"
    );
    Ok(())
}

fn validate_upload_session_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("YouTube returned an invalid resumable upload URL")?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && loopback),
        "YouTube returned an unsafe resumable upload URL"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
        "YouTube returned an unsafe resumable upload URL"
    );
    Ok(url)
}

fn confirmed_offset(headers: &header::HeaderMap, total: u64) -> Result<u64> {
    let range = headers
        .get(header::RANGE)
        .context("YouTube 308 response omitted Range")?
        .to_str()
        .context("YouTube upload Range is invalid text")?;
    let end = range
        .strip_prefix("bytes=0-")
        .context("YouTube upload Range has an unsupported format")?
        .parse::<u64>()
        .context("YouTube upload Range offset is invalid")?;
    let next = end
        .checked_add(1)
        .context("YouTube upload Range overflow")?;
    ensure!(next <= total, "YouTube confirmed offset exceeds file size");
    Ok(next)
}

pub fn validate_redirect_uri(value: &str) -> Result<()> {
    let url = Url::parse(value).context("YOUTUBE_REDIRECT_URI must be a valid URL")?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && loopback),
        "YOUTUBE_REDIRECT_URI must use HTTPS or loopback HTTP"
    );
    ensure!(
        url.fragment().is_none(),
        "YOUTUBE_REDIRECT_URI cannot contain a fragment"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::State,
        http::HeaderMap,
        response::IntoResponse,
        routing::{post, put},
        Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn authorization_url_has_narrow_scope_and_csrf_state() {
        let client = YouTubeOAuthClient::new(YouTubeOAuthConfig {
            client_id: "client id".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://127.0.0.1:8080/api/publish/youtube/callback".into(),
        })
        .unwrap();
        let url = Url::parse(&client.authorization_url("opaque-state").unwrap()).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params["scope"], YOUTUBE_UPLOAD_SCOPE);
        assert_eq!(params["state"], "opaque-state");
        assert_eq!(params["access_type"], "offline");
        assert!(!url.as_str().contains("secret"));
    }

    #[tokio::test]
    async fn code_exchange_validates_granted_scope() {
        let app = Router::new()
            .route(
                "/token",
                post(|| async {
                    axum::Json(serde_json::json!({
                        "access_token": "access",
                        "expires_in": 3600,
                        "refresh_token": "refresh",
                        "scope": YOUTUBE_UPLOAD_SCOPE,
                        "token_type": "Bearer"
                    }))
                }),
            )
            .route(
                "/revoke",
                post(|body: String| async move {
                    assert_eq!(body, "token=refresh");
                    StatusCode::OK
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = YouTubeOAuthClient::new(YouTubeOAuthConfig {
            client_id: "id".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://127.0.0.1/callback".into(),
        })
        .unwrap()
        .with_token_endpoint(Url::parse(&format!("http://{address}/token")).unwrap())
        .with_revoke_endpoint(Url::parse(&format!("http://{address}/revoke")).unwrap());
        let tokens = client.exchange_code("code").await.unwrap();
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh"));
        assert!(tokens.expires_at > crate::library::now_secs());
        let refreshed = client.refresh_access_token("kept-refresh").await.unwrap();
        assert_eq!(refreshed.refresh_token.as_deref(), Some("kept-refresh"));
        client.revoke_token("refresh").await.unwrap();
    }

    #[tokio::test]
    async fn resumable_upload_continues_from_provider_confirmed_offset() {
        #[derive(Clone)]
        struct MockState {
            location: String,
            puts: Arc<AtomicUsize>,
        }
        async fn start(State(state): State<MockState>) -> impl IntoResponse {
            (StatusCode::OK, [(header::LOCATION, state.location)])
        }
        async fn upload(
            State(state): State<MockState>,
            headers: HeaderMap,
            body: Bytes,
        ) -> impl IntoResponse {
            if headers[header::CONTENT_RANGE] == "bytes */10" {
                return (
                    StatusCode::PERMANENT_REDIRECT,
                    [(header::RANGE, "bytes=0-3")],
                    String::new(),
                );
            }
            let call = state.puts.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                assert_eq!(headers[header::CONTENT_RANGE], "bytes 0-9/10");
                assert_eq!(&body[..], b"0123456789");
                return (
                    StatusCode::PERMANENT_REDIRECT,
                    [(header::RANGE, "bytes=0-3")],
                    String::new(),
                );
            }
            assert_eq!(headers[header::CONTENT_RANGE], "bytes 4-9/10");
            assert_eq!(&body[..], b"456789");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"id":"video-123"}"#.into(),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = MockState {
            location: format!("http://{address}/session"),
            puts: Arc::new(AtomicUsize::new(0)),
        };
        let calls = state.puts.clone();
        let app = Router::new()
            .route("/upload", post(start))
            .route("/session", put(upload))
            .route("/expired", put(|| async { StatusCode::GONE }))
            .with_state(state);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = YouTubeOAuthClient::new(YouTubeOAuthConfig {
            client_id: "id".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://127.0.0.1/callback".into(),
        })
        .unwrap()
        .with_upload_endpoint(Url::parse(&format!("http://{address}/upload")).unwrap());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("video.mp4");
        tokio::fs::write(&path, b"0123456789").await.unwrap();
        let (progress, mut updates) = tokio::sync::mpsc::unbounded_channel();
        let (checkpoints, mut checkpoint_updates) = tokio::sync::mpsc::unbounded_channel();
        let id = client
            .upload_resumable(
                YouTubeUploadRequest {
                    access_token: "access",
                    path: &path,
                    mime_type: "video/mp4",
                    progress: &progress,
                    cancellation: &tokio_util::sync::CancellationToken::new(),
                    checkpoints: &checkpoints,
                },
                &YouTubeUploadMetadata {
                    title: "Release".into(),
                    description: "Description".into(),
                    privacy_status: "unlisted".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(id, "video-123");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(updates.recv().await, Some(40.0));
        assert_eq!(updates.recv().await, Some(100.0));
        assert_eq!(checkpoint_updates.recv().await.unwrap().confirmed_offset, 0);
        assert_eq!(checkpoint_updates.recv().await.unwrap().confirmed_offset, 4);
        let (resume_progress, mut resume_updates) = tokio::sync::mpsc::unbounded_channel();
        let (resume_checkpoints, mut resumed_checkpoints) = tokio::sync::mpsc::unbounded_channel();
        let resumed = client
            .resume_resumable(
                YouTubeUploadRequest {
                    access_token: "access",
                    path: &path,
                    mime_type: "video/mp4",
                    progress: &resume_progress,
                    cancellation: &tokio_util::sync::CancellationToken::new(),
                    checkpoints: &resume_checkpoints,
                },
                &YouTubeUploadCheckpoint {
                    session_url: format!("http://{address}/session"),
                    total_bytes: 10,
                    confirmed_offset: 9,
                },
            )
            .await
            .unwrap();
        assert_eq!(resumed.as_deref(), Some("video-123"));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(resume_updates.recv().await, Some(40.0));
        assert_eq!(resume_updates.recv().await, Some(100.0));
        assert_eq!(
            resumed_checkpoints.recv().await.unwrap().confirmed_offset,
            4
        );
        let expired = client
            .resume_resumable(
                YouTubeUploadRequest {
                    access_token: "access",
                    path: &path,
                    mime_type: "video/mp4",
                    progress: &resume_progress,
                    cancellation: &tokio_util::sync::CancellationToken::new(),
                    checkpoints: &resume_checkpoints,
                },
                &YouTubeUploadCheckpoint {
                    session_url: format!("http://{address}/expired"),
                    total_bytes: 10,
                    confirmed_offset: 4,
                },
            )
            .await
            .unwrap();
        assert_eq!(expired, None);
    }

    #[test]
    fn token_cipher_authenticates_actor_and_ciphertext() {
        let cipher = TokenCipher::new([7; 32]).unwrap();
        let sealed = cipher.encrypt("alice", b"refresh-token").unwrap();
        assert_ne!(sealed, b"refresh-token");
        assert_eq!(cipher.decrypt("alice", &sealed).unwrap(), b"refresh-token");
        assert!(cipher.decrypt("bob", &sealed).is_err());
        let mut tampered = sealed;
        *tampered.last_mut().unwrap() ^= 1;
        assert!(cipher.decrypt("alice", &tampered).is_err());
    }
}
