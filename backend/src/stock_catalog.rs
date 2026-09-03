use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{ensure, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const PEXELS_API: &str = "https://api.pexels.com/v1";
const SEARCH_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_SEARCH_CACHE_ENTRIES: usize = 128;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum StockKind {
    Photo,
    Video,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum StockOrientation {
    Landscape,
    Portrait,
    Square,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockAsset {
    pub provider_id: u64,
    pub media_type: &'static str,
    pub title: String,
    pub author: String,
    pub author_url: String,
    pub source_page_url: String,
    pub preview_url: String,
    pub import_url: String,
    pub width: u32,
    pub height: u32,
    pub duration: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockSearchResult {
    pub provider: &'static str,
    pub provider_url: &'static str,
    pub page: u32,
    pub total_results: u64,
    pub assets: Vec<StockAsset>,
}

#[derive(Clone)]
pub struct PexelsClient {
    client: Client,
    api_key: String,
    cache: Arc<Mutex<HashMap<SearchCacheKey, CachedSearch>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SearchCacheKey {
    query: String,
    kind: StockKind,
    orientation: Option<StockOrientation>,
    page: u32,
}

struct CachedSearch {
    inserted_at: Instant,
    result: StockSearchResult,
}

impl PexelsClient {
    pub fn new(api_key: String) -> Result<Self> {
        ensure!(
            !api_key.trim().is_empty(),
            "PEXELS_API_KEY must not be empty"
        );
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("video-kadr/0.1 stock-catalog")
                .build()?,
            api_key,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn search(
        &self,
        query: &str,
        kind: StockKind,
        orientation: Option<StockOrientation>,
        page: u32,
    ) -> Result<StockSearchResult> {
        let query = query.trim();
        ensure!(
            !query.is_empty() && query.chars().count() <= 100,
            "invalid stock search query"
        );
        ensure!((1..=1_000).contains(&page), "invalid stock search page");
        let cache_key = SearchCacheKey {
            query: query.to_lowercase(),
            kind,
            orientation,
            page,
        };
        if let Some(result) = self.cached(&cache_key).await {
            return Ok(result);
        }
        let endpoint = match kind {
            StockKind::Photo => "search",
            StockKind::Video => "videos/search",
        };
        let mut request = self
            .client
            .get(format!("{PEXELS_API}/{endpoint}"))
            .header("Authorization", &self.api_key)
            .query(&[
                ("query", query),
                ("page", &page.to_string()),
                ("per_page", "24"),
            ]);
        if let Some(orientation) = orientation {
            let value = match orientation {
                StockOrientation::Landscape => "landscape",
                StockOrientation::Portrait => "portrait",
                StockOrientation::Square => "square",
            };
            request = request.query(&[("orientation", value)]);
        }
        let response = request.send().await.context("Pexels request failed")?;
        ensure!(
            response.status().is_success(),
            "Pexels returned HTTP {}",
            response.status()
        );
        let result = match kind {
            StockKind::Photo => normalize_photos(response.json().await?),
            StockKind::Video => normalize_videos(response.json().await?),
        }?;
        self.remember(cache_key, result.clone()).await;
        Ok(result)
    }

    async fn cached(&self, key: &SearchCacheKey) -> Option<StockSearchResult> {
        let mut cache = self.cache.lock().await;
        cache.retain(|_, entry| entry.inserted_at.elapsed() < SEARCH_CACHE_TTL);
        cache.get(key).map(|entry| entry.result.clone())
    }

    async fn remember(&self, key: SearchCacheKey, result: StockSearchResult) {
        let mut cache = self.cache.lock().await;
        if cache.len() >= MAX_SEARCH_CACHE_ENTRIES {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.inserted_at)
                .map(|(key, _)| key.clone())
            {
                cache.remove(&oldest);
            }
        }
        cache.insert(
            key,
            CachedSearch {
                inserted_at: Instant::now(),
                result,
            },
        );
    }
}

#[derive(Deserialize)]
struct PhotoSearch {
    page: u32,
    total_results: u64,
    photos: Vec<Photo>,
}
#[derive(Deserialize)]
struct Photo {
    id: u64,
    width: u32,
    height: u32,
    url: String,
    photographer: String,
    photographer_url: String,
    alt: Option<String>,
    src: PhotoSources,
}
#[derive(Deserialize)]
struct PhotoSources {
    original: String,
    large2x: Option<String>,
    large: Option<String>,
    medium: Option<String>,
}

fn normalize_photos(value: PhotoSearch) -> Result<StockSearchResult> {
    let assets = value
        .photos
        .into_iter()
        .filter_map(|photo| {
            let import_url = photo
                .src
                .large2x
                .or(photo.src.large)
                .or(photo.src.medium)
                .unwrap_or(photo.src.original);
            valid_https(&photo.url).then_some(())?;
            valid_https(&photo.photographer_url).then_some(())?;
            valid_https(&import_url).then_some(())?;
            Some(StockAsset {
                provider_id: photo.id,
                media_type: "image",
                title: photo
                    .alt
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| format!("Pexels photo {}", photo.id)),
                author: photo.photographer,
                author_url: photo.photographer_url,
                source_page_url: photo.url,
                preview_url: import_url.clone(),
                import_url,
                width: photo.width,
                height: photo.height,
                duration: None,
            })
        })
        .collect();
    Ok(StockSearchResult {
        provider: "Pexels",
        provider_url: "https://www.pexels.com",
        page: value.page,
        total_results: value.total_results,
        assets,
    })
}

#[derive(Deserialize)]
struct VideoSearch {
    page: u32,
    total_results: u64,
    videos: Vec<Video>,
}
#[derive(Deserialize)]
struct Video {
    id: u64,
    width: u32,
    height: u32,
    duration: u32,
    url: String,
    image: String,
    user: VideoUser,
    video_files: Vec<VideoFile>,
}
#[derive(Deserialize)]
struct VideoUser {
    name: String,
    url: String,
}
#[derive(Deserialize)]
struct VideoFile {
    file_type: String,
    width: Option<u32>,
    height: Option<u32>,
    link: String,
}

fn normalize_videos(value: VideoSearch) -> Result<StockSearchResult> {
    let assets = value
        .videos
        .into_iter()
        .filter_map(|video| {
            let file = video
                .video_files
                .into_iter()
                .filter(|file| file.file_type == "video/mp4" && valid_https(&file.link))
                .max_by_key(|file| {
                    let width = file.width.unwrap_or(0);
                    (width <= 1920, width, file.height.unwrap_or(0))
                })?;
            (valid_https(&video.url) && valid_https(&video.user.url) && valid_https(&video.image))
                .then_some(())?;
            Some(StockAsset {
                provider_id: video.id,
                media_type: "video",
                title: format!("Pexels video {}", video.id),
                author: video.user.name,
                author_url: video.user.url,
                source_page_url: video.url,
                preview_url: video.image,
                import_url: file.link,
                width: file.width.unwrap_or(video.width),
                height: file.height.unwrap_or(video.height),
                duration: Some(video.duration),
            })
        })
        .collect();
    Ok(StockSearchResult {
        provider: "Pexels",
        provider_url: "https://www.pexels.com",
        page: value.page,
        total_results: value.total_results,
        assets,
    })
}

fn valid_https(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_licensed_photo_metadata_and_rejects_non_https_assets() {
        let input: PhotoSearch = serde_json::from_value(json!({"page":1,"total_results":2,"photos":[
            {"id":7,"width":1200,"height":800,"url":"https://www.pexels.com/photo/7","photographer":"Ada","photographer_url":"https://www.pexels.com/@ada","alt":"Ocean","src":{"original":"https://images.pexels.com/7.jpg","large2x":"https://images.pexels.com/7-large.jpg"}},
            {"id":8,"width":1,"height":1,"url":"https://www.pexels.com/photo/8","photographer":"Bad","photographer_url":"https://www.pexels.com/@bad","alt":"Bad","src":{"original":"http://127.0.0.1/private"}}
        ]})).unwrap();
        let result = normalize_photos(input).unwrap();
        assert_eq!(result.assets.len(), 1);
        assert_eq!(result.assets[0].author, "Ada");
        assert_eq!(
            result.assets[0].import_url,
            "https://images.pexels.com/7-large.jpg"
        );
    }

    #[tokio::test]
    async fn cache_normalizes_query_case_and_evicts_oldest_entry_at_capacity() {
        let client = PexelsClient::new("test-key".into()).unwrap();
        let result = StockSearchResult {
            provider: "Pexels",
            provider_url: "https://www.pexels.com",
            page: 1,
            total_results: 0,
            assets: Vec::new(),
        };
        for index in 0..MAX_SEARCH_CACHE_ENTRIES {
            client
                .remember(
                    SearchCacheKey {
                        query: format!("query-{index}"),
                        kind: StockKind::Photo,
                        orientation: None,
                        page: 1,
                    },
                    result.clone(),
                )
                .await;
        }
        client
            .remember(
                SearchCacheKey {
                    query: "replacement".into(),
                    kind: StockKind::Video,
                    orientation: Some(StockOrientation::Landscape),
                    page: 2,
                },
                result,
            )
            .await;

        let cache = client.cache.lock().await;
        assert_eq!(cache.len(), MAX_SEARCH_CACHE_ENTRIES);
        assert!(cache.keys().any(|key| key.query == "replacement"));
    }
}
