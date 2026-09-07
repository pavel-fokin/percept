use std::error::Error;
use std::sync::Arc;

use serde::Deserialize;

use super::{client, Fireworks, Ollama, OpenAi};
use crate::percept::{
    Model, ModelCatalog, ModelDescriptor, ModelListing, Provider, ReasoningEffort,
};

const OPENAI_REASONING_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

/// OpenAI models the catalog offers - a short static list, since
/// OpenAI has no listing endpoint worth querying. The `/models` picker
/// shows all three; `OPENAI_MODEL` is the one `main` builds with when
/// `PERCEPT_PROVIDER=openai` and headless runs have no picker.
pub const OPENAI_MODEL: &str = "gpt-5.6-luna";
const OPENAI_MODELS: &[&str] = &[OPENAI_MODEL, "gpt-5.6-terra", "gpt-5.6-sol"];

/// Fireworks models the catalog offers - a short static list, the same
/// reasoning as `OPENAI_MODELS`: Fireworks' own catalog is too large to
/// list live, so `main` builds with this one model when
/// `PERCEPT_PROVIDER=fireworks`.
pub const FIREWORKS_MODEL: &str = "accounts/fireworks/models/glm-5p3";
const FIREWORKS_MODELS: &[&str] = &[FIREWORKS_MODEL];

/// Where a hosted provider lives and the key that authenticates to it -
/// grouped so a call site can't transpose one provider's url with
/// another's key the way two same-typed positional strings would let it.
pub struct ProviderConfig {
    pub url: String,
    pub api_key: String,
}

/// Every model a run of `percept` can reach: ollama's, listed live from
/// its server, and OpenAI's and Fireworks', each from a static list.
/// Holds what building any provider needs, wired in at the entrypoint
/// rather than read from the environment here.
pub struct Catalog {
    ollama_url: String,
    openai: ProviderConfig,
    openai_reasoning_effort: String,
    fireworks: ProviderConfig,
    client: reqwest::Client,
}

impl Catalog {
    pub fn new(
        ollama_url: String,
        openai: ProviderConfig,
        openai_reasoning_effort: String,
        fireworks: ProviderConfig,
    ) -> Self {
        Self {
            ollama_url,
            openai,
            openai_reasoning_effort,
            fireworks,
            client: client(),
        }
    }
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagsModel>,
}

#[derive(Deserialize)]
struct TagsModel {
    name: String,
}

/// Turns ollama's `/api/tags` body into descriptors. A pure function
/// so the parsing is tested without a server.
fn parse_tags(body: &str) -> Result<Vec<ModelDescriptor>, Box<dyn Error + Send + Sync>> {
    let response: TagsResponse = serde_json::from_str(body)?;
    Ok(response
        .models
        .into_iter()
        .map(|model| ModelDescriptor {
            provider: Provider::Ollama,
            model: model.name,
            reasoning_efforts: &[],
        })
        .collect())
}

/// Every model a static-list provider offers, paired with `provider`.
fn static_descriptors(provider: Provider, models: &[&str]) -> Vec<ModelDescriptor> {
    models
        .iter()
        .map(|model| ModelDescriptor {
            provider,
            model: model.to_string(),
            reasoning_efforts: match provider {
                Provider::OpenAi => OPENAI_REASONING_EFFORTS,
                Provider::Ollama | Provider::Fireworks => &[],
            },
        })
        .collect()
}

impl ModelCatalog for Catalog {
    fn list(&self) -> ModelListing {
        let client = self.client.clone();
        let url = format!("{}/api/tags", self.ollama_url);
        Box::pin(async move {
            // A provider a request can't reach is left out, not
            // failed on - the catalog still shows what it can.
            let mut descriptors = fetch_tags(&client, &url).await.unwrap_or_default();
            descriptors.extend(static_descriptors(Provider::OpenAi, OPENAI_MODELS));
            descriptors.extend(static_descriptors(Provider::Fireworks, FIREWORKS_MODELS));
            descriptors
        })
    }

    fn build(&self, descriptor: &ModelDescriptor) -> Result<Arc<dyn Model>, Box<dyn Error>> {
        match descriptor.provider {
            Provider::Ollama => Ok(Arc::new(Ollama::new(
                self.ollama_url.clone(),
                descriptor.model.clone(),
            ))),
            Provider::OpenAi => Ok(Arc::new(OpenAi::new(
                self.openai.url.clone(),
                descriptor.model.clone(),
                self.openai_reasoning_effort.clone(),
                self.openai.api_key.clone(),
            ))),
            Provider::Fireworks => Ok(Arc::new(Fireworks::new(
                self.fireworks.url.clone(),
                descriptor.model.clone(),
                self.fireworks.api_key.clone(),
            ))),
        }
    }
}

async fn fetch_tags(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<ModelDescriptor>, Box<dyn Error + Send + Sync>> {
    let body = client.get(url).send().await?.text().await?;
    parse_tags(&body)
}

#[cfg(test)]
mod tests;
