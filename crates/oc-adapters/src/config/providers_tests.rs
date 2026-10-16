use super::*;
use crate::config::{Source, assemble};

#[test]
fn cfg14_provider_model_compatibility_uses_shared_v2_documents() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v2-config-semantics.json"
    ))
    .unwrap();
    for package in fixture["protocols"].as_array().unwrap() {
        for case in fixture["compatibility"].as_array().unwrap() {
            let documents = std::iter::once(fixture["base"].clone())
                .chain(case["documents"].as_array().unwrap().iter().cloned())
                .chain(std::iter::once(
                    json!({"providers":{"fixture":{"package":package}}}),
                ));
            let sources = documents
                .enumerate()
                .map(|(index, document)| Source {
                    path: format!("/fixture-{index}.json"),
                    trusted: false,
                    text: document.to_string(),
                })
                .collect::<Vec<_>>();
            let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
            let entry = &generation.providers["fixture"];
            let wire = crate::config::provider_wire("fixture", entry).unwrap();
            for binding in std::iter::once(&wire).chain(
                entry
                    .options
                    .request_bindings
                    .values()
                    .map(|request| &request.wire),
            ) {
                let compat = binding
                    .compatibility
                    .get("m")
                    .unwrap_or(&binding.default_compatibility);
                assert_eq!(
                    compat.supports_prompt_cache_key,
                    case["expected"]["supportsPromptCacheKey"].as_bool(),
                    "{} / {package}",
                    case["id"]
                );
                assert_eq!(
                    compat.reasoning_field.as_deref(),
                    case["expected"]["reasoningField"].as_str(),
                    "{} / {package}",
                    case["id"]
                );
                assert_eq!(
                    compat.max_completion_tokens,
                    case["expected"]["maxTokensField"] == "max_completion_tokens",
                    "{} / {package}",
                    case["id"]
                );
            }
        }
    }
}

#[test]
fn go02_canonical_provider_overlays_normalize_to_the_native_entry() {
    let canonical = json!({"providers":{"p":{"package":"@ai-sdk/openai-compatible", "settings":{"baseURL":"https://example.com/v1", "apiKey":"synthetic", "transport":"http"}, "headers":{"X-A":"a"}, "body":{"nested":{"one":1}}, "models":{"catalog":{"modelID":"wire", "package":"@ai-sdk/anthropic", "settings":{"baseURL":"https://example.com/messages", "thinking":{"type":"adaptive"}}, "capabilities":{"tools":true,"input":["text"],"output":["text"]}, "compatibility":{"reasoningField":"reasoning_content"}, "variants":[{"id":"high","settings":{"reasoningEffort":"high"}}]}}}}});
    let p = &document(canonical.as_object().unwrap()).unwrap()["p"];
    assert_eq!(p["npm"], "@ai-sdk/openai-compatible");
    assert_eq!(
        p["options"],
        json!({"baseURL":"https://example.com/v1", "apiKey":"synthetic", "transport":"http",
               "headers":{"X-A":"a"}, "body":{"nested":{"one":1}}})
    );
    let model = &p["models"]["catalog"];
    assert_eq!(model["modelID"], "wire");
    assert_eq!(model["package"], "@ai-sdk/anthropic");
    assert_eq!(model["tool_call"], true);
    assert_eq!(
        model["modalities"],
        json!({"input":["text"],"output":["text"]})
    );
    assert_eq!(
        model["variants"],
        json!([{"id":"high","settings":{"reasoningEffort":"high"}}])
    );
}

#[test]
fn go02_source_merge_preserves_fields_variants_and_connection_provenance() {
    let sources = [
        Source {path:"/untrusted.json".into(),trusted:false,text:json!({"providers":{"p":{"package":"@ai-sdk/openai", "settings":{"baseURL":"https://example.com/v1","apiKey":"synthetic"},"headers":{"X-A":"old"},"body":{"nested":{"one":1}},"models":{"m":{"modelID":"wire","variants":[{"id":"custom-a","body":{"nested":{"one":1}}}]}}}}}).to_string()},
        Source {path:"/trusted.json".into(),trusted:true,text:json!({"providers":{"p":{"name":"Later", "headers":{"x-a":"new"},"body":{"nested":{"two":2}},"models":{"m":{"name":"Display","variants":[{"id":"custom-a","body":{"nested":{"two":2}}},{"id":"custom-b","settings":{}}]}}}}}).to_string()},
    ];
    let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
    let p = &generation.providers["p"];
    assert_eq!(p.options.base_url, "https://example.com/v1");
    assert!(!p.options.endpoint_trusted);
    assert_eq!(
        p.options.endpoint_source.as_deref(),
        Some("/untrusted.json")
    );
    assert_eq!(p.options.headers.len(), 1);
    assert_eq!(p.options.headers["x-a"], "new");
    assert_eq!(p.options.body["nested"], json!({"one":1,"two":2}));
    assert_eq!(p.models["m"]["modelID"], "wire");
    assert_eq!(
        p.models["m"]["variants"][0]["body"]["nested"],
        json!({"one":1,"two":2})
    );
    let catalog = crate::models::ModelCatalog {
        provider: "p".into(),
        models: p.models.clone(),
    };
    let selection = crate::models::select_model(&catalog, "m").unwrap();
    assert!(crate::models::select_variant(&selection, Some("custom-b")).is_ok());
}

#[test]
fn go02_duplicate_variants_and_unsafe_bodies_fail_without_echoing_inputs() {
    for variants in [
        json!([{"id":"x"},{"id":"x"}]),
        json!([{"settings":{}}]),
        json!("bad"),
    ] {
        let input = json!({"providers":{"p":{"models":{"m":{"variants":variants}}}}});
        assert!(document(input.as_object().unwrap()).is_err());
    }
    for input in [
        json!({"providers":{"p":{"body":{"model":"CANARY"}}}}),
        json!({"providers":{"p":{"models":{"m":{"body":{"messages":[]}}}}}}),
    ] {
        let error = document(input.as_object().unwrap()).unwrap_err();
        assert!(!format!("{error:?}").contains("CANARY"));
    }
}
