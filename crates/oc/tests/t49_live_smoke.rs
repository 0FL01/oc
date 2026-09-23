//! Explicit, one-shot OpenProxy smoke. Never included in the normal offline suite.
//! A durable reservation is consumed before starting the binary; an interrupted
//! run is not retried automatically. This does not unlock the T27 campaign.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SESSION: &str = "t49-live-model-identity";
const PROMPT: &str = "Reply with one short sentence: connection established.";
const MAIN_OUTPUT: u64 = 768;
const TITLE_OUTPUT: u64 = 256;
const CAMPAIGN_LIMIT: usize = 24;
const _: () = assert!(MAIN_OUTPUT <= 2048 && TITLE_OUTPUT <= 2048);

fn worst_case_requests() -> usize {
    // One headless main turn, at most MAX_ROUNDS generations and one ancillary
    // title generation; the adapter retries each at most MAX_ATTEMPTS times.
    // The fixture has no subagents/MCP and denies all tool execution.
    (oc_adapters::runtime::MAX_ROUNDS as usize + 1) * oc_adapters::provider::MAX_ATTEMPTS
}

fn reserve_once(path: &Path, model: &str, attempts: usize) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let record = serde_json::json!({
        "model": model,
        "reserved_generation_requests": attempts,
        "campaign_limit": CAMPAIGN_LIMIT,
        "state": "attempted; do not reset or retry after unknown external effect"
    });
    file.write_all(record.to_string().as_bytes())?;
    file.sync_all()
}

#[test]
fn single_run_worst_case_fits_campaign_and_reservation_survives_retry() {
    assert!(worst_case_requests() <= CAMPAIGN_LIMIT);
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("reservation");
    reserve_once(&ledger, "fixture/model", worst_case_requests()).unwrap();
    assert!(reserve_once(&ledger, "fixture/model", worst_case_requests()).is_err());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(ledger).unwrap()).unwrap()["reserved_generation_requests"],
        worst_case_requests()
    );
}

#[test]
#[ignore = "explicit one-shot OpenProxy route check with the reissued test key"]
fn t49_rotated_key_route_probe() {
    assert_eq!(std::env::var("OC_T49_LIVE_OPT_IN").as_deref(), Ok("1"));
    assert_eq!(std::env::var("OC_T49_ROTATED_KEY").as_deref(), Ok("1"));
    let model = std::env::var("OC_T49_ROUTE_MODEL").expect("exact requested wire model ID");
    assert!(!model.trim().is_empty());
    let url = std::env::var("LUDKA2_API_URL").expect("product test API URL");
    let key = std::env::var("LUDKA2_API_KEY").expect("product test API key");
    assert!(!url.is_empty() && !key.is_empty());

    // The original campaign remains fully reserved. A newly reissued key and
    // the owner's explicit retry request permit this *separate*, fixed-ID
    // campaign; never reset, delete or replay the earlier one.
    let owned = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
        .join(".local");
    let reserved: usize = [
        "t49-live-reservation.json",
        "t49-live-diagnostic-reservation.json",
        "t49-live-diagnostic2-reservation.json",
        "t49-live-route-reservation.json",
    ]
    .iter()
    .map(|name| {
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(owned.join(name)).expect("existing campaign ledger"))
                .expect("valid campaign ledger");
        assert_eq!(record["campaign_limit"], CAMPAIGN_LIMIT);
        record["reserved_generation_requests"]
            .as_u64()
            .expect("reserved attempts") as usize
    })
    .sum();
    assert_eq!(reserved, 24, "original campaign must remain fully reserved");
    let attempts = oc_adapters::provider::MAX_ATTEMPTS;
    assert!(attempts <= CAMPAIGN_LIMIT);
    reserve_once(
        &owned.join("t49-live-rotated-route-reservation.json"),
        &model,
        attempts,
    )
    .expect("rotated-key route check already reserved; never retry an unknown request");

    let generation = probe_route(&model, url, key);
    assert!(
        !generation.text.trim().is_empty(),
        "empty successful response"
    );
    println!(
        "T49 OpenProxy route: completed for {model}; usage present: {}",
        generation.usage.is_some()
    );
}

#[test]
#[ignore = "explicit single original-model probe under the reissued-key campaign"]
fn t49_rotated_key_original_model_probe() {
    assert_eq!(std::env::var("OC_T49_LIVE_OPT_IN").as_deref(), Ok("1"));
    assert_eq!(std::env::var("OC_T49_ROTATED_KEY").as_deref(), Ok("1"));
    let model = std::env::var("OC_TEST_MODEL").expect("original selected wire model ID");
    let url = std::env::var("LUDKA2_API_URL").expect("product test API URL");
    let key = std::env::var("LUDKA2_API_KEY").expect("product test API key");
    assert!(!model.trim().is_empty() && !url.is_empty() && !key.is_empty());
    let owned = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
        .join(".local");
    let original: serde_json::Value = serde_json::from_slice(
        &fs::read(owned.join("t49-live-reservation.json")).expect("original model reservation"),
    )
    .unwrap();
    assert_eq!(original["model"], model);
    let route: serde_json::Value = serde_json::from_slice(
        &fs::read(owned.join("t49-live-rotated-route-reservation.json"))
            .expect("new-key route reservation"),
    )
    .unwrap();
    let product: serde_json::Value = serde_json::from_slice(
        &fs::read(owned.join("t49-live-rotated-product-reservation.json"))
            .expect("new-key product reservation"),
    )
    .unwrap();
    let used = route["reserved_generation_requests"].as_u64().unwrap()
        + product["reserved_generation_requests"].as_u64().unwrap();
    assert_eq!(used, 20);
    assert!(used + oc_adapters::provider::MAX_ATTEMPTS as u64 <= CAMPAIGN_LIMIT as u64);
    reserve_once(
        &owned.join("t49-live-rotated-original-reservation.json"),
        &model,
        oc_adapters::provider::MAX_ATTEMPTS,
    )
    .expect("original-model probe already reserved; do not retry");
    let generation = probe_route(&model, url, key);
    println!(
        "T49 original model route: completed; text present: {}; usage present: {}",
        !generation.text.trim().is_empty(),
        generation.usage.is_some()
    );
}

fn probe_route(model: &str, url: String, key: String) -> oc_adapters::provider::Generation {
    let config = oc_adapters::provider::ResponsesConfig {
        base_url: url,
        api_key: key,
        timeout: None,
        chunk_timeout_ms: 12_000,
        connect_timeout: Duration::from_secs(8),
        allow_private: false,
        headers: Default::default(),
        set_cache_key: false,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(35),
            oc_adapters::provider::stream_input_observed(
                &config,
                model,
                None,
                &[oc_adapters::provider::InputItem::message(
                    oc_adapters::provider::InputRole::User,
                    PROMPT,
                )],
                &[],
                256,
                &cancel,
                &mut |_| {},
            ),
        )
        .await
    });
    match result {
        Ok(Ok(generation)) => generation,
        Ok(Err(error)) => panic!("T49 OpenProxy route typed outcome for {model}: {error}"),
        Err(_) => panic!("T49 OpenProxy route watchdog expired"),
    }
}

#[test]
#[ignore = "explicit, bounded real API smoke with pre-existing product test credentials"]
fn t49_one_shot_live_model_identity() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(one_shot_live_model_identity(false));
}

#[test]
#[ignore = "explicit one-shot product-binary smoke with the reissued OpenProxy test key"]
fn t49_rotated_key_product_model_identity() {
    assert_eq!(std::env::var("OC_T49_ROTATED_KEY").as_deref(), Ok("1"));
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(one_shot_live_model_identity(true));
}

async fn one_shot_live_model_identity(rotated: bool) {
    assert_eq!(std::env::var("OC_T49_LIVE_OPT_IN").as_deref(), Ok("1"));
    let qualified = std::env::var(if rotated {
        "OC_T49_ROUTE_MODEL"
    } else {
        "OC_TEST_MODEL"
    })
    .expect("explicit requested wire model ID");
    // Provider namespace belongs to local configuration. Remote catalog IDs
    // retain their slashes and must reach Responses as the exact wire model.
    let provider = "t49proxy";
    let model = qualified.as_str();
    assert!(!model.is_empty() && !model.contains('#'));
    let url = std::env::var("LUDKA2_API_URL").expect("product test API URL");
    let key = std::env::var("LUDKA2_API_KEY").expect("product test API key");
    assert!(!url.is_empty() && !key.is_empty());
    assert!(worst_case_requests() <= CAMPAIGN_LIMIT);
    // The fixed path is intentional: crashes and reruns cannot mint a new budget.
    // This file contains no secret or response content; never delete it to retry.
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let owned = repo.join(".local");
    assert!(
        owned.is_dir() && owned.join("tmp").is_dir(),
        "owned private test directory"
    );
    if rotated {
        let previous: serde_json::Value = serde_json::from_slice(
            &fs::read(owned.join("t49-live-rotated-route-reservation.json"))
                .expect("prior route check reservation"),
        )
        .expect("valid prior reservation");
        assert_eq!(previous["model"], qualified);
        assert_eq!(
            previous["reserved_generation_requests"],
            oc_adapters::provider::MAX_ATTEMPTS
        );
        assert!(oc_adapters::provider::MAX_ATTEMPTS + worst_case_requests() <= CAMPAIGN_LIMIT);
    }
    let fixture = tempfile::Builder::new()
        .prefix("t49-live-")
        .tempdir_in(owned.join("tmp"))
        .unwrap();
    let home = fixture.path().join("home");
    let config = home.join("config/opencode");
    let project = fixture.path().join("project");
    let data = home.join("data/oc");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&project).unwrap();
    let config_body = serde_json::json!({
        "model": format!("{provider}/{model}"),
        "provider": { provider: {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": "{env:LUDKA2_API_URL}", "apiKey": "{env:LUDKA2_API_KEY}"},
            "models": { model: {"name": model, "limit": {"context": 32768, "output": MAIN_OUTPUT}} }
        }},
        "permissions": {"*": "deny"},
        "dcp": {"enabled": false}
    });
    fs::write(config.join("opencode.json"), config_body.to_string()).unwrap();
    // Preflight: verify product config and selected exact model without a paid
    // generation. No credential, URL or raw configuration is printed.
    let environment = std::collections::BTreeMap::from([
        ("HOME".to_string(), home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.join("config").to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            home.join("data").to_string_lossy().into_owned(),
        ),
        ("LUDKA2_API_URL".to_string(), url),
        ("LUDKA2_API_KEY".to_string(), key),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &data, environment.clone())
            .await
            .unwrap_or_else(|_| panic!("isolated product config preflight failed"));
    let session = oc_core::domain::SessionId(SESSION.into());
    app.create_session(session.clone()).await.unwrap();
    let selection = app
        .session_selection(
            session,
            false,
            oc_core::queries::SessionSelectionAction::Current,
        )
        .await
        .expect("effective catalog selection");
    assert_eq!(selection.model_id, model);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    if std::env::var("OC_T49_PREFLIGHT_ONLY").as_deref() == Ok("1") {
        return;
    }
    // Reserve worst-case usage before the first possible generation. An unknown
    // outcome conservatively consumes the entire reservation across restarts.
    reserve_once(
        &owned.join(if rotated {
            "t49-live-rotated-product-reservation.json"
        } else {
            "t49-live-reservation.json"
        }),
        &qualified,
        worst_case_requests(),
    )
    .expect("one-shot campaign already reserved; no automatic retry");
    let mut child = Command::new(env!("CARGO_BIN_EXE_oc"))
        .env_clear()
        .envs(&environment)
        .current_dir(&project)
        .args([
            "--data-dir",
            data.to_str().unwrap(),
            "run",
            PROMPT,
            "--session",
            SESSION,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("launch product binary");
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll product binary") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop private binary");
            let _ = child.wait();
            panic!("bounded live smoke watchdog elapsed");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    assert!(
        status.success(),
        "live smoke failed; check sanitized provider diagnostics privately"
    );
    let db = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    let (stored_model, status): (String, String) = db
        .query_row(
            "SELECT json_extract(result,'$.model'), status FROM turns WHERE session_id=?1",
            [SESSION],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored_model, model);
    assert_eq!(status, "completed");
}
