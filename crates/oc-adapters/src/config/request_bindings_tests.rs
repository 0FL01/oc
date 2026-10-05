use super::*;
use crate::config::assemble;

#[test]
fn go03_request_templates_capture_api_id_protocol_variant_and_field_origin() {
    let sources = [
        Source { path:"/untrusted.json".into(), trusted:false, text:json!({"providers":{"p":{
            "package":"@ai-sdk/openai", "settings":{"baseURL":"https://example.com/v1","apiKey":"parent"},
            "headers":{"X-Test":"provider"}, "body":{"nested":{"a":1}},
            "models":{"m":{"modelID":"wire-model", "package":"@ai-sdk/anthropic",
                "settings":{"baseURL":"https://example.com/other", "thinking":{"type":"adaptive"}},
                "headers":{"x-test":"model"}, "body":{"nested":{"b":2}},
                "variants":[{"id":"v","settings":{"reasoningEffort":"high"},"headers":{"X-TEST":"variant"},"body":{"nested":{"c":3}}}]}}
        }}}).to_string() },
        Source { path:"/trusted.json".into(), trusted:true, text:json!({"providers":{"p":{"models":{"m":{"name":"Later", "variants":[{"id":"v","body":{"nested":{"d":4}}}]}}}}}).to_string() },
    ];
    let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
    let requests = &generation.providers["p"].options.request_bindings;
    let base = &requests[&("m".into(), None)];
    let variant = &requests[&("m".into(), Some("v".into()))];
    assert_eq!(base.wire.api_model.as_deref(), Some("wire-model"));
    assert_eq!(
        base.wire.protocol,
        crate::provider::protocol::Protocol::Messages
    );
    assert_eq!(base.base_url, "https://example.com/other");
    assert_eq!(base.headers["x-test"], "model");
    assert_eq!(variant.headers["X-TEST"], "variant");
    assert_eq!(variant.wire.settings.effort.as_deref(), Some("high"));
    assert_eq!(
        variant.wire.settings.body["nested"],
        json!({"a":1,"b":2,"c":3,"d":4})
    );
    let ip = "127.0.0.1".parse().unwrap();
    assert!(!crate::endpoint::peer_allowed(
        variant.wire.endpoint.as_ref(),
        ip,
        false
    ));
    assert!(!format!("{generation:?}").contains("parent"));
}

#[test]
fn go01_model_endpoint_is_admitted_before_secret_reads_and_cannot_gain_later_trust() {
    let source = |path: &str, trusted, value: Value| Source {
        path: path.into(),
        trusted,
        text: value.to_string(),
    };
    let a = source(
        "/untrusted.json",
        false,
        json!({"providers":{"p":{
            "settings":{"baseURL":"https://example.com/v1","apiKey":"synthetic"},
            "models":{"m":{"settings":{"apiKey":"{file:private.key}"}}}
        }}}),
    );
    let b = source(
        "/trusted.json",
        true,
        json!({"providers":{"p":{"models":{"m":{"name":"Later"}}}}}),
    );
    assert!(assemble(&[a, b], &BTreeMap::new(), None).is_err());
    let go = source(
        "/trusted.json",
        true,
        json!({"providers":{"opencode-go":{
            "settings":{"baseURL":crate::auth::GO_BASE_URL},
            "models":{"m":{"settings":{"baseURL":"https://foreign.invalid/v1", "apiKey":"{file:private.key}"}}}
        }}}),
    );
    let reads = std::cell::Cell::new(0);
    let raw = super::super::document(
        serde_json::from_str::<Value>(&go.text)
            .unwrap()
            .as_object()
            .unwrap(),
    )
    .unwrap();
    let origins = BTreeMap::new();
    assert!(
        request_bindings(
            "opencode-go",
            &raw["opencode-go"],
            &go.path,
            &origins,
            std::slice::from_ref(&go),
            &BTreeMap::new(),
            &|_, _| {
                reads.set(reads.get() + 1);
                Ok("CANARY".into())
            }
        )
        .is_err()
    );
    assert_eq!(reads.get(), 0);
}

#[test]
fn go01_literal_model_and_variant_ids_cannot_alias_secret_source_provenance() {
    let sources = [
        Source {
            path: "/untrusted.json".into(),
            trusted: false,
            text: json!({"providers":{"p":{
                "settings":{"baseURL":"https://example.com/v1","apiKey":"synthetic"},
                "models":{"m":{"variants":[{"id":"v","settings":{"apiKey":"{file:private.key}"}}]}}
            }}})
            .to_string(),
        },
        Source {
            path: "/trusted.json".into(),
            trusted: true,
            text: json!({"providers":{"p":{
                "models":{"m.variants.v":{"settings":{"apiKey":"another configured key"}}}
            }}})
            .to_string(),
        },
    ];
    let reads = std::cell::Cell::new(0);
    let result =
        super::super::super::assemble_with_reader(&sources, &BTreeMap::new(), None, &|_, _| {
            reads.set(reads.get() + 1);
            Ok("SECRET_CANARY".into())
        });
    assert!(matches!(result, Err(ConfigError::Untrusted { .. })));
    assert_eq!(reads.get(), 0);
}

#[tokio::test]
async fn go01_unready_captured_auxiliary_bindings_never_dispatch() {
    for policy in ["key", "oauth"] {
        let sources = [Source {
            path: "/trusted.json".into(),
            trusted: true,
            text: json!({"providers":{"p":{
                "settings":{"baseURL":"https://example.com/v1","apiKey":"parent"},
                "models":{"main":{},"aux":{"settings":{"apiKey":"","authPolicy":policy}}}
            }}})
            .to_string(),
        }];
        let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
        let requests = &generation.providers["p"].options.request_bindings;
        let mut outer = requests[&("main".into(), None)].clone();
        outer.wire.requests = requests.clone();
        let mut dispatched = 0;
        let result = crate::provider::stream_input_counted(
            &outer,
            "aux",
            None,
            &[],
            &[],
            1,
            &std::sync::atomic::AtomicBool::new(false),
            &mut |_| {},
            &mut || {
                dispatched += 1;
                async { Ok(()) }
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(crate::provider::ProviderError::InvalidConfig)
        ));
        assert_eq!(dispatched, 0);
    }
}
