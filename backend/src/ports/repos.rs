//! Persistence ports (DIP). Handlers/services depend on these traits; `Db`
//! remains the production adapter and tests can swap in-memory fakes.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde_json::Value;

use crate::db::Db;
use crate::model::Job;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait RenderCache: Send + Sync {
    fn cache_get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<(Value, String)>>>;
    fn cache_put<'a>(
        &'a self,
        key: &'a str,
        output: &'a Value,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<()>>;
    fn cache_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<bool>>;
}

pub trait JobRepo: Send + Sync {
    fn load_job<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Job>>>;
}

pub trait ProjectRepo: Send + Sync {
    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<()>>;
}

pub trait MediaRepo: Send + Sync {
    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<()>>;
}

impl RenderCache for Db {
    fn cache_get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<(Value, String)>>> {
        Box::pin(async move { self.cache_get(key).await })
    }

    fn cache_put<'a>(
        &'a self,
        key: &'a str,
        output: &'a Value,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.cache_put(key, output, filename).await })
    }

    fn cache_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move { self.cache_delete(key).await })
    }
}

impl JobRepo for Db {
    fn load_job<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Job>>> {
        Box::pin(async move { self.load_job(id).await })
    }
}

impl ProjectRepo for Db {
    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let _ = self.load_job("__ping__").await?;
            Ok(())
        })
    }
}

impl MediaRepo for Db {
    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

// Also implement for Arc<Db> to support AppState field
impl RenderCache for Arc<Db> {
    fn cache_get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<(Value, String)>>> {
        Box::pin(async move { self.as_ref().cache_get(key).await })
    }

    fn cache_put<'a>(
        &'a self,
        key: &'a str,
        output: &'a Value,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.as_ref().cache_put(key, output, filename).await })
    }

    fn cache_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move { self.as_ref().cache_delete(key).await })
    }
}

/// In-memory render cache for handler tests without SQLite.
#[derive(Default)]
pub struct MemoryRenderCache {
    entries: Mutex<HashMap<String, (Value, String)>>,
}

impl RenderCache for MemoryRenderCache {
    fn cache_get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<(Value, String)>>> {
        Box::pin(async move { Ok(self.entries.lock().expect("cache lock").get(key).cloned()) })
    }

    fn cache_put<'a>(
        &'a self,
        key: &'a str,
        output: &'a Value,
        filename: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.entries
                .lock()
                .expect("cache lock")
                .insert(key.to_owned(), (output.clone(), filename.to_owned()));
            Ok(())
        })
    }

    fn cache_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            Ok(self
                .entries
                .lock()
                .expect("cache lock")
                .remove(key)
                .is_some())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn memory_render_cache_round_trips() {
        let cache = MemoryRenderCache::default();
        assert!(cache.cache_get("k").await.unwrap().is_none());
        cache
            .cache_put("k", &json!({"id": "out"}), "out.mp4")
            .await
            .unwrap();
        let hit = cache.cache_get("k").await.unwrap().unwrap();
        assert_eq!(hit.1, "out.mp4");
        assert!(cache.cache_delete("k").await.unwrap());
        assert!(cache.cache_get("k").await.unwrap().is_none());
    }
}
