use super::*;

fn catalog() -> Catalog {
    Catalog::new(
        "http://localhost:11434".to_string(),
        ProviderConfig {
            url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
        },
        "low".to_string(),
        ProviderConfig {
            url: "https://api.fireworks.ai/inference/v1".to_string(),
            api_key: "fw-test".to_string(),
        },
    )
}

#[test]
fn an_ollama_descriptor_builds_an_ollama_model() {
    let descriptor = ModelDescriptor {
        provider: Provider::Ollama,
        model: "gemma4".to_string(),
        reasoning_efforts: &[],
    };

    let model = catalog().build(&descriptor).unwrap();

    assert_eq!(model.name(), "gemma4");
}

#[test]
fn an_openai_descriptor_builds_an_openai_model() {
    for name in ["gpt-5.6-luna", "gpt-5.6-terra", "gpt-5.6-sol"] {
        let descriptor = ModelDescriptor {
            provider: Provider::OpenAi,
            model: name.to_string(),
            reasoning_efforts: &[],
        };

        let model = catalog().build(&descriptor).unwrap();

        assert_eq!(model.name(), name);
    }
}

#[tokio::test]
async fn listing_offers_every_openai_model() {
    // No server listens here, so ollama drops out and only the static
    // lists remain.
    let unreachable = Catalog::new(
        "http://127.0.0.1:1".to_string(),
        ProviderConfig {
            url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
        },
        "low".to_string(),
        ProviderConfig {
            url: "https://api.fireworks.ai/inference/v1".to_string(),
            api_key: "fw-test".to_string(),
        },
    );

    let openai: Vec<String> = unreachable
        .list()
        .await
        .into_iter()
        .filter(|d| d.provider == Provider::OpenAi)
        .map(|d| d.model)
        .collect();

    assert_eq!(openai, ["gpt-5.6-luna", "gpt-5.6-terra", "gpt-5.6-sol"]);
}

#[tokio::test]
async fn listing_offers_openai_reasoning_efforts() {
    let unreachable = Catalog::new(
        "http://127.0.0.1:1".to_string(),
        ProviderConfig {
            url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
        },
        "low".to_string(),
        ProviderConfig {
            url: "https://api.fireworks.ai/inference/v1".to_string(),
            api_key: "fw-test".to_string(),
        },
    );

    let descriptors = unreachable.list().await;
    let openai: Vec<&ModelDescriptor> = descriptors
        .iter()
        .filter(|descriptor| descriptor.provider == Provider::OpenAi)
        .collect();

    assert!(openai.iter().all(|descriptor| {
        descriptor.reasoning_efforts
            == [
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ]
    }));
}

#[test]
fn a_fireworks_descriptor_builds_a_fireworks_model() {
    let descriptor = ModelDescriptor {
        provider: Provider::Fireworks,
        model: FIREWORKS_MODEL.to_string(),
        reasoning_efforts: &[],
    };

    let model = catalog().build(&descriptor).unwrap();

    assert_eq!(model.name(), FIREWORKS_MODEL);
}

#[test]
fn tags_response_parses_into_ollama_descriptors() {
    let body = r#"{"models":[{"name":"gemma4:latest"},{"name":"llama3"}]}"#;

    let descriptors = parse_tags(body).unwrap();

    assert_eq!(
        descriptors,
        vec![
            ModelDescriptor {
                provider: Provider::Ollama,
                model: "gemma4:latest".to_string(),
                reasoning_efforts: &[],
            },
            ModelDescriptor {
                provider: Provider::Ollama,
                model: "llama3".to_string(),
                reasoning_efforts: &[],
            },
        ]
    );
}

#[test]
fn a_malformed_tags_response_is_an_error() {
    assert!(parse_tags("not json").is_err());
}

#[tokio::test]
async fn listing_falls_back_to_openai_entries_when_ollama_is_unreachable() {
    // No server listens on this port, so the request fails outright.
    let unreachable = Catalog::new(
        "http://127.0.0.1:1".to_string(),
        ProviderConfig {
            url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
        },
        "low".to_string(),
        ProviderConfig {
            url: "https://api.fireworks.ai/inference/v1".to_string(),
            api_key: "fw-test".to_string(),
        },
    );

    let descriptors = unreachable.list().await;

    let mut expected = static_descriptors(Provider::OpenAi, OPENAI_MODELS);
    expected.extend(static_descriptors(Provider::Fireworks, FIREWORKS_MODELS));
    assert_eq!(descriptors, expected);
}
