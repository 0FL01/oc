use super::*;
use crate::config::{Source, assemble};

#[test]
fn go02_legacy_and_canonical_provider_overlays_are_equivalent() {
    let legacy = json!({"provider":{"p":{"npm":"@ai-sdk/openai-compatible", "api":"https://example.com/v1", "options":{"apiKey":"synthetic", "headers":{"X-A":"a"}, "extraBody":{"nested":{"one":1}}}, "models":{"catalog":{"id":"wire", "provider":{"npm":"@ai-sdk/anthropic", "api":"https://example.com/messages"}, "options":{"thinking":{"type":"adaptive"}}, "tool_call":true, "modalities":{"input":["text"],"output":["text"]}, "interleaved":{"field":"reasoning_content"}, "variants":{"high":{"reasoningEffort":"high"}}}}}}});
    let canonical = json!({"providers":{"p":{"package":"@ai-sdk/openai-compatible", "settings":{"baseURL":"https://example.com/v1", "apiKey":"synthetic"}, "headers":{"X-A":"a"}, "body":{"nested":{"one":1}}, "models":{"catalog":{"modelID":"wire", "package":"@ai-sdk/anthropic", "settings":{"baseURL":"https://example.com/messages", "thinking":{"type":"adaptive"}}, "capabilities":{"tools":true,"input":["text"],"output":["text"]}, "compatibility":{"reasoningField":"reasoning_content"}, "variants":[{"id":"high","settings":{"reasoningEffort":"high"}}]}}}}});
    let a = document(legacy.as_object().unwrap()).unwrap();
    let b = document(canonical.as_object().unwrap()).unwrap();
    for key in [
        "modelID",
        "package",
        "settings",
        "capabilities",
        "compatibility",
        "variants",
    ] {
        assert_eq!(
            a["p"]["models"]["catalog"][key], b["p"]["models"]["catalog"][key],
            "{key}"
        );
    }
    assert_eq!(a["p"]["options"], b["p"]["options"]);
    assert_eq!(a["p"]["npm"], b["p"]["npm"]);
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
fn go02_conflicting_roots_and_duplicate_variants_fail_without_echoing_inputs() {
    let conflict = json!({"provider":{"p":{"options":{"apiKey":"SECRET_CANARY"}}},"providers":{"p":{"settings":{"apiKey":"OTHER_CANARY"}}}});
    let error = document(conflict.as_object().unwrap()).unwrap_err();
    assert!(!format!("{error:?}").contains("CANARY"));
    for variants in [
        json!([{"id":"x"},{"id":"x"}]),
        json!([{"settings":{}}]),
        json!("bad"),
    ] {
        let input = json!({"providers":{"p":{"models":{"m":{"variants":variants}}}}});
        assert!(document(input.as_object().unwrap()).is_err());
    }
    for input in [
        json!({"providers":{"p":{"settings":{"unknownExecutable":"CANARY"}}}}),
        json!({"providers":{"p":{"body":{"model":"CANARY"}}}}),
        json!({"providers":{"p":{"models":{"m":{"body":{"messages":[]}}}}}}),
    ] {
        let error = document(input.as_object().unwrap()).unwrap_err();
        assert!(!format!("{error:?}").contains("CANARY"));
    }
}
