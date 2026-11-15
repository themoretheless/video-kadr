use anyhow::{ensure, Result};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{sync::Mutex, time::Duration};

pub const MODEL: &str = "qwen2.5vl:3b";

#[derive(Debug)]
pub(crate) struct Managed {
    pub origin: Url,
    pub installer: String,
    phase: Mutex<Phase>,
    install_admission: tokio::sync::Mutex<()>,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Missing,
    Installing,
    Failed,
    Ready,
    Offline,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub model: &'static str,
    pub phase: Phase,
    pub can_install: bool,
}

impl Managed {
    pub fn new(origin: &str, installer: String) -> Result<Self> {
        let origin = Url::parse(origin)?;
        let loopback = match origin.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        ensure!(
            origin.scheme() == "http"
                && loopback
                && origin.path() == "/"
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "SCENE_DESCRIPTION_OLLAMA_URL requires a loopback HTTP origin"
        );
        ensure!(
            !installer.trim().is_empty(),
            "SCENE_DESCRIPTION_INSTALLER is required"
        );
        Ok(Self {
            origin,
            installer,
            phase: Mutex::new(Phase::Missing),
            install_admission: tokio::sync::Mutex::new(()),
        })
    }

    pub async fn installed_digest(&self, client: &Client) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct Models {
            models: Vec<Model>,
        }
        #[derive(Deserialize)]
        struct Model {
            name: String,
            digest: String,
        }

        let response = client
            .get(self.origin.join("/api/tags")?)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        let models: Models = bounded_json(response).await?;
        Ok(models
            .models
            .into_iter()
            .find(|item| item.name == MODEL)
            .map(|item| item.digest)
            .filter(|digest| {
                digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            }))
    }

    pub async fn status(&self, client: &Client, actor: &str) -> Status {
        let phase = match self.installed_digest(client).await {
            Ok(Some(_)) => Phase::Ready,
            Ok(None) => *self.phase.lock().unwrap(),
            Err(_) => Phase::Offline,
        };
        Status {
            model: MODEL,
            phase,
            can_install: actor == self.installer,
        }
    }

    pub async fn install(self: &std::sync::Arc<Self>, client: &Client) -> Result<Phase> {
        let _admission = self.install_admission.lock().await;
        if self.installed_digest(client).await?.is_some() {
            return Ok(Phase::Ready);
        }
        {
            let mut phase = self.phase.lock().unwrap();
            if matches!(*phase, Phase::Installing) {
                return Ok(Phase::Installing);
            }
            *phase = Phase::Installing;
        }
        let managed = self.clone();
        let client = client.clone();
        tokio::spawn(async move {
            let result = managed.pull(&client).await;
            // Ollama owns the stored model. A successful pull is verified against
            // its catalog at read time, including after the app restarts.
            *managed.phase.lock().unwrap() = if result.is_ok() {
                Phase::Missing
            } else {
                Phase::Failed
            };
        });
        Ok(Phase::Installing)
    }

    async fn pull(&self, client: &Client) -> Result<()> {
        #[derive(Deserialize)]
        struct Pull {
            status: String,
        }
        let response = client
            .post(self.origin.join("/api/pull")?)
            .timeout(Duration::from_secs(3600))
            .json(&json!({"model": MODEL, "stream": false}))
            .send()
            .await?;
        let result: Pull = bounded_json(response).await?;
        ensure!(result.status == "success", "Model pull failed");
        ensure!(
            self.installed_digest(client).await?.is_some(),
            "Model missing after pull"
        );
        Ok(())
    }
}

async fn bounded_json<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T> {
    ensure!(response.status().is_success(), "Unexpected Ollama response");
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            chunk.len() <= 1024 * 1024 - bytes.len(),
            "Ollama response exceeds limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
