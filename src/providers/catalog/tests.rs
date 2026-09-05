use super::*;

fn catalog() -> Catalog {
    Catalog::new(
        "http://localhost:11434".to_string(),
        "https://api.openai.com/v1".to_string(),
        "sk-test".to_string(),
        "low".to_string(),
    )
}

#[test]
fn an_ollama_descriptor_builds_an_ollama_model() {
    let descriptor = ModelDescriptor {
        provider: Provider::Ollama,
        model: "gemma4".to_string(),
    };

    let model = catalog().build(&descriptor).unwrap();

    assert_eq!(model.name(), "gemma4");
}

#[test]
fn an_openai_descriptor_builds_an_openai_model() {
    let descriptor = ModelDescriptor {
        provider: Provider::OpenAi,
        model: "gpt-5.6-luna".to_string(),
    };

    let model = catalog().build(&descriptor).unwrap();

    assert_eq!(model.name(), "gpt-5.6-luna");
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
            },
            ModelDescriptor {
                provider: Provider::Ollama,
                model: "llama3".to_string(),
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
        "https://api.openai.com/v1".to_string(),
        "sk-test".to_string(),
        "low".to_string(),
    );

    let descriptors = unreachable.list().await;

    assert_eq!(descriptors, openai_descriptors());
}
