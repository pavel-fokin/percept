use std::error::Error;
use std::sync::Arc;

use serde::Deserialize;

use super::{client, Ollama, OpenAi};
use crate::percept::{Model, ModelCatalog, ModelDescriptor, ModelListing, Provider};

/// OpenAI models the catalog offers - a short static list, since
/// OpenAI has no listing endpoint worth querying. One entry today, the
/// model `main` builds with when `PERCEPT_PROVIDER=openai`.
pub const OPENAI_MODEL: &str = "gpt-5.6-luna";
const OPENAI_MODELS: &[&str] = &[OPENAI_MODEL];

/// Every model a run of `percept` can reach: ollama's, listed live from
/// its server, and OpenAI's, from a static list. Holds what building
/// either provider needs, wired in at the entrypoint rather than read
/// from the environment here.
pub struct Catalog {
    ollama_url: String,
    openai_url: String,
    openai_api_key: String,
    openai_reasoning_effort: String,
    client: reqwest::Client,
}

impl Catalog {
    pub fn new(
        ollama_url: String,
        openai_url: String,
        openai_api_key: String,
        openai_reasoning_effort: String,
    ) -> Self {
        Self {
            ollama_url,
            openai_url,
            openai_api_key,
            openai_reasoning_effort,
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
        })
        .collect())
}

fn openai_descriptors() -> Vec<ModelDescriptor> {
    OPENAI_MODELS
        .iter()
        .map(|model| ModelDescriptor {
            provider: Provider::OpenAi,
            model: model.to_string(),
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
            descriptors.extend(openai_descriptors());
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
                self.openai_url.clone(),
                descriptor.model.clone(),
                self.openai_reasoning_effort.clone(),
                self.openai_api_key.clone(),
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
