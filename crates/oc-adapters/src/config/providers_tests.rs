use super::*;
use crate::config::{Source, assemble};

#[test]
fn w5_provider_unknown_fields_do_not_override_known_siblings() {
    let raw = json!({"unknown":"ignored","provider":{"legacy":{"options":{"apiKey":"ignored"}}},"providers":{"p":{
        "package":"@ai-sdk/openai","npm":"@ai-sdk/anthropic","unknown":"ignored",
        "options":{"baseURL":"https://ignored.invalid/v1"},
        "settings":{"baseURL":"https://example.com/v1","apiKey":"synthetic"},
        "models":{"m":{"modelID":"wire","unknown":"ignored","tool_call":false,
            "capabilities":{"tools":true,"input":["text"],"output":["text"],"unknown":false},
            "variants":[{"id":"custom","unknown":"ignored","body":{"seed":7}}]}}
    }}});
    let generation = assemble(
        &[Source {
            path: "/fixture.json".into(),
            trusted: false,
            text: raw.to_string(),
        }],
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(generation.providers["legacy"].options.api_key, "ignored");
    let provider = &generation.providers["p"];
    assert_eq!(provider.npm.as_deref(), Some("@ai-sdk/openai"));
    assert_eq!(provider.options.base_url, "https://example.com/v1");
    assert_eq!(provider.models["m"]["modelID"], "wire");
    assert_eq!(provider.models["m"]["tool_call"], true);
    assert!(provider.models["m"].get("unknown").is_none());
    assert_eq!(provider.models["m"]["variants"][0]["body"]["seed"], 7);
    assert!(provider.models["m"]["variants"][0].get("unknown").is_none());
}

#[test]
fn w9_legacy_provider_and_model_overlays_use_native_entry_precedence() {
    let raw = json!({
        "provider": {
            "p": {"npm":"@ai-sdk/anthropic", "options":{"apiKey":"legacy-only"}},
            "legacy": {
                "npm":"@ai-sdk/openai-compatible", "env":["FIXTURE_KEY"],
                "options":{"baseURL":"https://example.com/v1", "headers":{"X-P":"p"}, "body":{"seed":1}},
                "models":{"catalog":{
                    "id":"wire", "options":{"baseURL":"https://example.com/old-options", "temperature":0.5,"reasoningEffort":"high"},
                    "headers":{"X-M":"m"},
                    "variants":{"fast":{"temperature":0.2}},
                    "provider":{"npm":"@ai-sdk/openai-compatible","api":"https://example.com/model"},
                    "tool_call":true,"modalities":{"input":["text"],"output":["text"]},
                    "limit":{"context":65536,"output":4096}
                }, "native-url": {
                    "settings":{"baseURL":"https://example.com/native"},
                    "options":{"baseURL":"https://example.com/old-options"},
                    "provider":{"api":"https://example.com/legacy-api"}
                }}
            }
        },
        "providers":{"p":{"package":"@ai-sdk/openai","name":"native"}}
    });
    let entries = document(raw.as_object().unwrap()).unwrap();
    assert_eq!(entries["p"]["npm"], "@ai-sdk/openai");
    assert!(
        entries["p"].get("options").is_none(),
        "native entry must not inherit legacy credentials"
    );
    let p = &entries["legacy"];
    assert_eq!(p["options"]["headers"]["X-P"], "p");
    assert_eq!(p["options"]["body"]["seed"], 1);
    let m = &p["models"]["catalog"];
    assert_eq!(m["modelID"], "wire");
    assert_eq!(m["package"], "@ai-sdk/openai-compatible");
    assert_eq!(m["settings"]["baseURL"], "https://example.com/model");
    assert_eq!(m["settings"]["reasoningEffort"], "high");
    assert_eq!(m["body"]["temperature"], 0.5);
    assert_eq!(m["variants"][0]["id"], "fast");
    assert_eq!(m["variants"][0]["body"]["temperature"], 0.2);
    assert_eq!(m["tool_call"], true);
    assert_eq!(
        p["models"]["native-url"]["settings"]["baseURL"],
        "https://example.com/native"
    );
    for bad in [
        json!({"provider":{"p":{"options":{"headers":false}}}}),
        json!({"provider":{"p":false}}),
    ] {
        assert!(document(bad.as_object().unwrap()).is_err());
    }
}

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
        models: p.models.clone().into(),
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
