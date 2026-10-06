//! Explicit Go qualification: real native runtime, fixed authority, durable budget.
//! This module and its pre-DNS hook do not exist in production builds.
use super::*;
use crate::{auth::GO_BASE_URL, models_dev::PROVIDER, storage::Db};
use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

fn update<T>(
    path: &Path,
    mutate: impl FnOnce(&mut Value) -> Result<T, ProviderError>,
) -> Result<T, ProviderError> {
    let parent = path.parent().ok_or(ProviderError::DispatchRefused)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(parent.join(".go-live.lock"))
        .map_err(|_| ProviderError::DispatchRefused)?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|_| ProviderError::DispatchRefused)?;
    if std::fs::symlink_metadata(path)
        .map_err(|_| ProviderError::DispatchRefused)?
        .file_type()
        .is_symlink()
    {
        return Err(ProviderError::DispatchRefused);
    }
    let bytes = std::fs::read(path).map_err(|_| ProviderError::DispatchRefused)?;
    if bytes.len() > 128 * 1024 {
        return Err(ProviderError::DispatchRefused);
    }
    let mut ledger: Value =
        serde_json::from_slice(&bytes).map_err(|_| ProviderError::DispatchRefused)?;
    if ledger["campaign_id"] != "t53-go-20261006"
        || ledger["generation_limit"] != 24
        || ledger["smoke_output_token_limit"] != 2048
    {
        return Err(ProviderError::DispatchRefused);
    }
    let result = mutate(&mut ledger)?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| ProviderError::DispatchRefused)?;
    serde_json::to_writer_pretty(&mut temp, &ledger).map_err(|_| ProviderError::DispatchRefused)?;
    temp.write_all(b"\n")
        .map_err(|_| ProviderError::DispatchRefused)?;
    temp.as_file()
        .sync_all()
        .map_err(|_| ProviderError::DispatchRefused)?;
    temp.persist(path)
        .map_err(|_| ProviderError::DispatchRefused)?;
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| ProviderError::DispatchRefused)?;
    Ok(result)
}

fn is_followup(request: &Value) -> bool {
    request["input"]
        .as_array()
        .is_some_and(|a| a.iter().any(|i| i["type"] == "function_call_output"))
        || request["messages"].as_array().is_some_and(|a| {
            a.iter().any(|i| {
                i["role"] == "tool"
                    || i["content"]
                        .as_array()
                        .is_some_and(|b| b.iter().any(|p| p["type"] == "tool_result"))
            })
        })
}

pub(super) fn tool_smoke_body(
    body: &[u8],
    protocol: protocol::Protocol,
) -> Result<Vec<u8>, ProviderError> {
    let request: Value = serde_json::from_slice(body).map_err(|_| ProviderError::InvalidConfig)?;
    let read_present = request["tools"].as_array().is_some_and(|tools| {
        tools.iter().any(|tool| match protocol {
            protocol::Protocol::Chat => tool["function"]["name"] == "read",
            protocol::Protocol::Responses | protocol::Protocol::Messages => tool["name"] == "read",
        })
    });
    if !read_present {
        return Err(ProviderError::InvalidConfig);
    }
    // Validate actual tool exposure, not merely the fixture's scalar permissions.
    // Preserve the product's native body; no provider-dependent forced choice.
    Ok(body.to_vec())
}

// These are explicit dated qualification targets, never protocol routing rules.
const CONFLICT_PROBES: [&str; 2] = ["qwen3.8-max", "qwen3.7-plus"];

pub(super) fn reserve(
    path: &Path,
    url: &str,
    body: &[u8],
    protocol: protocol::Protocol,
) -> Result<u64, ProviderError> {
    let suffix = match protocol {
        protocol::Protocol::Responses => "responses",
        protocol::Protocol::Chat => "chat/completions",
        protocol::Protocol::Messages => "messages",
    };
    if url != format!("{GO_BASE_URL}/{suffix}") {
        return Err(ProviderError::DispatchRefused);
    }
    let request: Value =
        serde_json::from_slice(body).map_err(|_| ProviderError::DispatchRefused)?;
    let tokens = ["max_output_tokens", "max_tokens", "max_completion_tokens"]
        .iter()
        .find_map(|name| request[*name].as_u64())
        .ok_or(ProviderError::DispatchRefused)?;
    if !(1..=2048).contains(&tokens) || request["stream"] != true || !request["model"].is_string() {
        return Err(ProviderError::DispatchRefused);
    }
    update(path, |ledger| {
        let count = ledger["physical_generation_requests"]
            .as_u64()
            .ok_or(ProviderError::DispatchRefused)?;
        if count >= 24 {
            return Err(ProviderError::DispatchRefused);
        }
        if ledger["attempts"].as_array().map_or(0, Vec::len) as u64 != count {
            return Err(ProviderError::DispatchRefused);
        }
        let sequence = count + 1;
        let followup = is_followup(&request);
        let lane = if request["model"]
            .as_str()
            .is_some_and(|model| CONFLICT_PROBES.contains(&model))
        {
            "protocol_probe"
        } else if followup {
            "tool_followup"
        } else {
            "main"
        };
        ledger["physical_generation_requests"] = sequence.into();
        ledger[lane] = (ledger[lane].as_u64().unwrap_or(0) + 1).into();
        ledger["generation_started"] = true.into();
        ledger["status"] = "running".into();
        ledger["accounting_enforcement_qualified"] = true.into();
        ledger.as_object_mut().unwrap().remove("blocker");
        let offered_tools = request["tools"]
            .as_array()
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| {
                        tool["name"]
                            .as_str()
                            .or_else(|| tool["function"]["name"].as_str())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let attempt = json!({
            "sequence": sequence,
            "model": request["model"],
            "protocol": suffix,
            "lane": lane,
            "has_tool_result": followup,
            "max_output_tokens": tokens,
            "outcome": "reserved_before_dns",
            "offered_tools": offered_tools,
            "forced_read": request.get("tool_choice").is_some(),
        });
        ledger
            .as_object_mut()
            .unwrap()
            .entry("attempts")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or(ProviderError::DispatchRefused)?
            .push(attempt);
        Ok(sequence)
    })
}

pub(super) fn finish(
    path: &Path,
    sequence: u64,
    result: &Result<Generation, ProviderError>,
) -> Result<(), ProviderError> {
    update(path, |ledger| {
        let entry = ledger["attempts"]
            .as_array_mut()
            .and_then(|a| a.iter_mut().find(|v| v["sequence"] == sequence))
            .ok_or(ProviderError::DispatchRefused)?;
        match result {
            Ok(output) => {
                entry["outcome"] = "complete".into();
                entry["finish"] = format!("{:?}", output.finish).into();
                entry["text_bytes"] = output.text.len().into();
                entry["item_types"] = json!(
                    output
                        .output
                        .iter()
                        .filter_map(|i| i["type"].as_str())
                        .collect::<Vec<_>>()
                );
                entry["tool_calls"] = output
                    .output
                    .iter()
                    .filter(|i| i["type"] == "function_call")
                    .count()
                    .into();
            }
            Err(error) => {
                entry["outcome"] = "failed".into();
                // Never persist provider prose, headers, body, prompts or credentials.
                entry["failure"] = match error {
                    ProviderError::Request(f) => {
                        json!({"kind":format!("{:?}",f.kind),"http_status":f.http_status,"output_committed":f.output_committed})
                    }
                    ProviderError::OutputStructure { stage, code } => {
                        json!({"stage":format!("{stage:?}"),"code":format!("{code:?}")})
                    }
                    _ => json!({"category":"local_or_transport"}),
                };
            }
        }
        Ok(())
    })
}

fn seed() -> Value {
    json!({"campaign_id":"t53-go-20261006","generation_limit":24,"smoke_output_token_limit":2048,"physical_generation_requests":0})
}

#[test]
fn go06_budget_is_durable_shared_and_refuses_before_dns_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.json");
    std::fs::write(&path, seed().to_string()).unwrap();
    let body = br#"{"model":"catalog-model","stream":true,"max_output_tokens":2048}"#;
    for n in 1..=24 {
        assert_eq!(
            reserve(
                &path,
                &format!("{GO_BASE_URL}/responses"),
                body,
                protocol::Protocol::Responses
            )
            .unwrap(),
            n
        );
        finish(&path, n, &Err(ProviderError::Cancelled)).unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        reserve(
            &path,
            &format!("{GO_BASE_URL}/responses"),
            body,
            protocol::Protocol::Responses
        ),
        Err(ProviderError::DispatchRefused)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        reserve(
            &path,
            "https://foreign.invalid/responses",
            body,
            protocol::Protocol::Responses
        ),
        Err(ProviderError::DispatchRefused)
    );
    assert_eq!(
        reserve(
            &path,
            &format!("{GO_BASE_URL}/responses"),
            br#"{"model":"m","stream":true,"max_output_tokens":2049}"#,
            protocol::Protocol::Responses
        ),
        Err(ProviderError::DispatchRefused)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn go06_exhausted_campaign_blocks_every_wire_before_transport_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ledger.json");
    let mut value = seed();
    value["physical_generation_requests"] = 24.into();
    std::fs::write(&path, value.to_string()).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    db.create_session("budget").unwrap();
    let context = context::RequestContext::capture(&db, root.path(), "budget").unwrap();
    for protocol in [
        protocol::Protocol::Responses,
        protocol::Protocol::Chat,
        protocol::Protocol::Messages,
    ] {
        let config = ResponsesConfig {
            base_url: GO_BASE_URL.into(),
            api_key: "synthetic".into(),
            timeout: None,
            chunk_timeout_ms: 1000,
            connect_timeout: Duration::from_secs(1),
            allow_private: false,
            headers: BTreeMap::new(),
            set_cache_key: false,
            wire: WireBinding {
                go: true,
                protocol,
                context: Some(context.clone()),
                messages_bearer: true,
                live_campaign: Some(path.clone()),
                ..Default::default()
            },
        };
        let mut dispatched = 0;
        let result = stream_input_counted(
            &config,
            "catalog-model",
            None,
            &[InputItem::message(InputRole::User, "must not send")],
            &[ToolDef {
                name: "read".into(),
                description: "read".into(),
                parameters: json!({"type":"object"}),
            }],
            2048,
            &AtomicBool::new(false),
            &mut |_| {},
            &mut || {
                dispatched += 1;
                async { Ok(()) }
            },
        )
        .await;
        assert_eq!(result, Err(ProviderError::DispatchRefused));
        assert_eq!(dispatched, 0);
    }
}

#[test]
fn go06_smoke_validates_read_exposure_without_altering_any_wire_body() {
    for (protocol, tools, result) in [
        (
            protocol::Protocol::Responses,
            json!([{"name":"read"}]),
            json!({"input":[{"type":"function_call_output"}]}),
        ),
        (
            protocol::Protocol::Chat,
            json!([{"function":{"name":"read"}}]),
            json!({"messages":[{"role":"tool"}]}),
        ),
        (
            protocol::Protocol::Messages,
            json!([{"name":"read"}]),
            json!({"messages":[{"role":"user","content":[{"type":"tool_result"}]}]}),
        ),
    ] {
        let first = json!({"tools":tools,"model":"exact","stream":true,"max_tokens":2048});
        let body: Value = serde_json::from_slice(
            &tool_smoke_body(&serde_json::to_vec(&first).unwrap(), protocol).unwrap(),
        )
        .unwrap();
        assert_eq!(body, first);
        let mut followup = result;
        followup["tools"] = tools;
        let body: Value = serde_json::from_slice(
            &tool_smoke_body(&serde_json::to_vec(&followup).unwrap(), protocol).unwrap(),
        )
        .unwrap();
        assert_eq!(body, followup);
        assert!(tool_smoke_body(br#"{"tools":[]}"#, protocol).is_err());
    }
}

#[tokio::test]
#[ignore = "explicit authorized Go live campaign; <=24 durable requests, no key logging"]
async fn go06_bounded_native_go_live() {
    use crate::{
        config::{Generation, Permission},
        patch::ProtectedGlobs,
        runtime::{Runtime, TurnParams, TurnStatus},
        tools::ToolRoots,
    };
    use std::sync::atomic::AtomicBool;
    assert_eq!(
        std::env::var("OC_GO_LIVE").as_deref(),
        Ok("1"),
        "explicit opt-in required"
    );
    let key = std::env::var("OPENCODE_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .expect("authorized Go input required");
    let ledger_path = std::path::PathBuf::from(
        std::env::var("OC_GO_LIVE_LEDGER").expect("existing campaign ledger required"),
    );
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let token = "GO_NATIVE_ROUNDTRIP_7c91";
    std::fs::write(project.join("go-smoke.txt"), token).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    let client =
        crate::discovery::ReqwestDiscoveryClient::new(std::time::Duration::from_secs(10)).unwrap();
    let public = db
        .public_catalog()
        .refresh(
            &db,
            &client,
            &BTreeMap::new(),
            crate::composition::go_catalog::now_ms(),
            true,
        )
        .await;
    assert!(
        public.failure.is_none() && !public.models.is_empty(),
        "public catalog unavailable (no generation)"
    );
    let mut composition = crate::composition::load_local_with_env(
        &project,
        BTreeMap::from([("OPENCODE_API_KEY".into(), key)]),
    )
    .await
    .unwrap();
    composition.resolve_credentials(&db).await.unwrap();
    composition.attach_public_catalog(&db).await;
    let mut choices = Vec::new();
    for protocol in [
        protocol::Protocol::Responses,
        protocol::Protocol::Chat,
        protocol::Protocol::Messages,
    ] {
        let selected = composition
            .catalog
            .models
            .iter()
            .filter(|(id, row)| {
                row["tool_call"] == true
                    && composition.provider.for_selection(id, None).wire.protocol == protocol
                    && !composition
                        .provider
                        .for_selection(id, None)
                        .wire
                        .unsupported
            })
            .min_by(|a, b| {
                let cost = |v: &Value| {
                    v["cost"]["input"].as_f64().unwrap_or(f64::MAX)
                        + v["cost"]["output"].as_f64().unwrap_or(f64::MAX)
                };
                cost(a.1).total_cmp(&cost(b.1)).then_with(|| a.0.cmp(b.0))
            })
            .map(|(id, _)| id.clone())
            .expect("required exact protocol representative absent");
        choices.push(selected);
    }
    for id in CONFLICT_PROBES {
        assert!(
            composition.catalog.models.contains_key(id),
            "dated conflict row absent; no guessed replacement"
        );
        if !choices.iter().any(|s| s == id) {
            choices.push(id.into());
        }
    }
    composition.provider.wire.live_campaign = Some(ledger_path.clone());
    for config in composition.provider.wire.requests.values_mut() {
        config.wire.live_campaign = Some(ledger_path.clone());
    }
    let generation = Generation {
        permissions: BTreeMap::from([
            ("*".into(), Permission::Deny),
            ("read".into(), Permission::Allow),
        ]),
        // This is a programmatic fixture authority, not a scalar override of
        // Composition's independently compiled ordered default rules.
        permission_rules: Default::default(),
        ..composition.generation.clone()
    };
    assert!(
        crate::permissions::PermissionRules::default().actions_visible(
            &generation.permissions,
            &["read"],
            false
        )
    );
    let runtime = Runtime::new(
        &db,
        "go-live",
        generation,
        ProtectedGlobs { patterns: vec![] },
        crate::files::Files::new(&project, db.root()).unwrap(),
        crate::shell::Shell::new(&project).unwrap(),
        BTreeMap::new(),
        ToolRoots {
            project: project.clone(),
            data: db.root().into(),
        },
        None,
        false,
        crate::dcp_auto::DcpConfig::default(),
    )
    .unwrap();
    runtime
        .publish_provider_state(composition.provider_state.clone())
        .unwrap();
    for (index, model) in choices.iter().enumerate() {
        let already = update(&ledger_path, |ledger| {
            Ok(ledger["results"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|r| r["model"] == *model && r["pass"] == true)
            }))
        })
        .unwrap();
        if already {
            continue;
        }
        let session = format!("go-live-{index}");
        runtime.create_session(&session).unwrap();
        let cancel = AtomicBool::new(false);
        let variant = crate::models::ordered_variants(&composition.catalog.models[model])
            .iter()
            .find(|(id, _)| *id == "none")
            .map(|_| "none".to_string());
        let result = runtime.run_turn(TurnParams {
            session: session.clone(),
            prompt: "Use the read tool exactly once to read go-smoke.txt. Then reply with only the exact token from that file. Do not use any other tool or task. Keep reasoning minimal.".into(),
            invocation: None,
            catalog: &composition.catalog,
            model_id: model.clone(),
            variant: variant.clone(),
            max_output: 2048,
            provider: composition.provider.clone(),
            cancel: &cancel,
        }).await;
        let (pass, calls, usage) = match &result {
            Ok(r) => (
                r.status == TurnStatus::Completed && r.calls.len() == 1 && r.text.contains(token),
                r.calls.len(),
                r.usage,
            ),
            Err(_) => (false, 0, None),
        };
        let bound = composition
            .provider
            .for_selection(model, variant.as_deref())
            .provenance(PROVIDER, model)
            .unwrap();
        update(&ledger_path, |ledger| {
            let row = json!({
                "model": model, "protocol": bound.protocol, "variant": variant,
                "pass": pass, "native_read_calls": calls, "usage": usage,
            });
            ledger
                .as_object_mut()
                .unwrap()
                .entry("results")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or(ProviderError::DispatchRefused)?
                .push(row);
            ledger["status"] = if pass {
                "running"
            } else {
                "qualification_failed"
            }
            .into();
            Ok(())
        })
        .unwrap();
        println!(
            "GO_LIVE model={model} protocol={:?} pass={pass} native_read_calls={calls}",
            bound.protocol
        );
        assert!(
            pass,
            "Go qualification failed; inspect sanitized campaign facts, not raw responses"
        );
        assert_eq!(db.list_tool_ops(&session).unwrap().len(), 1);
    }
    update(&ledger_path, |ledger| {
        ledger["status"] = "qualified".into();
        ledger["required_authorized_input"] =
            "OC_API_KEY (mapped only in test process to OPENCODE_API_KEY)".into();
        Ok(())
    })
    .unwrap();
}
