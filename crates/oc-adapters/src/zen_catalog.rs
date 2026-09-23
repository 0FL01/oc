//! Strict, replace-on-success catalog for direct Zen Free Chat Completions.
//! Eligibility is the intersection of the public models.dev metadata and the
//! live Zen inventory; neither a name suffix nor a cached list proves price.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};
use thiserror::Error;

const ZEN_API: &str = "https://opencode.ai/zen/v1";
const CHAT_SDK: &str = "@ai-sdk/openai-compatible";
const BODY_CAP: usize = 16 * 1024 * 1024;
const ROW_CAP: usize = 10_000;

/// Safe to display: no URLs, server bodies, model IDs, or transport details.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ZenCatalogError {
    #[error("invalid catalog endpoint")]
    InvalidEndpoint,
    #[error("catalog request failed")]
    Request,
    #[error("invalid catalog response")]
    InvalidResponse,
    #[error("no eligible Zen Free Chat Completions models")]
    NoEligibleModels,
}

fn endpoint(
    url: &str,
    expected_host: &str,
    expected_path: &str,
    allow_private: bool,
) -> Result<reqwest::Url, ZenCatalogError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| ZenCatalogError::InvalidEndpoint)?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path().trim_end_matches('/') != expected_path
    {
        return Err(ZenCatalogError::InvalidEndpoint);
    }
    let public = parsed.scheme() == "https"
        && parsed.host_str() == Some(expected_host)
        && parsed.port().is_none();
    let local = allow_private
        && parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("127.0.0.1" | "[::1]"));
    if !public && !local {
        return Err(ZenCatalogError::InvalidEndpoint);
    }
    Ok(parsed)
}

async fn get_json(
    client: &reqwest::Client,
    url: reqwest::Url,
    allow_private: bool,
) -> Result<Value, ZenCatalogError> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| ZenCatalogError::Request)?;
    if !response.status().is_success() {
        return Err(ZenCatalogError::Request);
    }
    if response
        .remote_addr()
        .is_some_and(|peer| !crate::zen_chat::allowed(peer.ip(), allow_private))
    {
        return Err(ZenCatalogError::Request);
    }
    if response
        .content_length()
        .is_some_and(|len| len > BODY_CAP as u64)
    {
        return Err(ZenCatalogError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ZenCatalogError::Request)?
    {
        if chunk.len() > BODY_CAP.saturating_sub(bytes.len()) {
            return Err(ZenCatalogError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| ZenCatalogError::InvalidResponse)
}

/// Fetch two public catalogs, validate both, then return only the current
/// intersection. A failed refresh never publishes a partial/previous catalog.
/// `allow_private` is exclusively for explicit loopback HTTP test fixtures.
pub async fn fetch(
    metadata_url: &str,
    zen_base_url: &str,
    allow_private: bool,
) -> Result<BTreeMap<String, Value>, ZenCatalogError> {
    let metadata = endpoint(metadata_url, "models.dev", "/api.json", allow_private)?;
    let zen = endpoint(zen_base_url, "opencode.ai", "/zen/v1", allow_private)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .dns_resolver(Arc::new(crate::zen_chat::GuardedResolver { allow_private }))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .user_agent(crate::USER_AGENT)
        .build()
        .map_err(|_| ZenCatalogError::Request)?;
    let metadata = get_json(&client, metadata, allow_private).await?;
    let mut models = zen;
    models.set_path("/zen/v1/models");
    let availability = get_json(&client, models, allow_private).await?;
    parse(&metadata, &availability)
}

fn object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

fn positive(value: &Value) -> bool {
    value.as_u64().is_some_and(|n| n > 0)
}

fn price_zero(cost: &Value) -> bool {
    let Some(cost) = object(cost) else {
        return false;
    };
    if !["input", "output"]
        .iter()
        .all(|field| cost.get(*field).is_some_and(Value::is_number))
    {
        return false;
    }
    cost.iter().all(|(key, value)| match key.as_str() {
        "input" | "output" | "cache_read" | "cache_write" | "input_audio" | "output_audio" => {
            value.as_f64() == Some(0.0)
        }
        "context_over_200k" => price_zero(value),
        "tiers" => value.as_array().is_some_and(|tiers| {
            !tiers.is_empty()
                && tiers.iter().all(|tier| {
                    let Some(fields) = object(tier) else {
                        return false;
                    };
                    let Some(descriptor) = fields.get("tier").and_then(object) else {
                        return false;
                    };
                    if descriptor.len() != 2
                        || !descriptor.get("type").is_some_and(Value::is_string)
                        || !descriptor.get("size").is_some_and(positive)
                    {
                        return false;
                    }
                    let prices: Map<String, Value> = fields
                        .iter()
                        .filter(|(key, _)| key.as_str() != "tier")
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect();
                    price_zero(&Value::Object(prices))
                })
        }),
        // A new/unknown billing dimension cannot silently become free.
        _ => false,
    })
}

fn valid_api(api: &Value) -> bool {
    api.as_str() == Some(ZEN_API)
}

fn eligible_model(row: &Value, id: &str, provider: &Map<String, Value>) -> Option<Value> {
    let map = object(row)?;
    if map.get("id")?.as_str()? != id || id.trim().is_empty() {
        return None;
    }
    // Closed schema for billing: changes outside known descriptive fields
    // cannot silently introduce another pricing dimension.
    if map.keys().any(|key| {
        !matches!(
            key.as_str(),
            "id" | "name"
                | "description"
                | "attachment"
                | "reasoning"
                | "reasoning_options"
                | "tool_call"
                | "temperature"
                | "release_date"
                | "last_updated"
                | "modalities"
                | "open_weights"
                | "limit"
                | "cost"
                | "family"
                | "knowledge"
                | "provider"
                | "structured_output"
                | "interleaved"
                | "status"
                | "npm"
                | "api"
                | "options"
                | "headers"
                | "baseURL"
        )
    }) {
        return None;
    }
    // models.dev overrides live in `model.provider`, with npm/api as
    // provider-level defaults. Direct row overrides are also checked if added.
    let override_provider = match map.get("provider") {
        Some(value) => Some(object(value)?),
        None => None,
    };
    for source in [Some(map), override_provider].into_iter().flatten() {
        if source
            .get("npm")
            .is_some_and(|v| v.as_str() != Some(CHAT_SDK))
            || source.get("api").is_some_and(|v| !valid_api(v))
        {
            return None;
        }
    }
    if override_provider.is_some_and(|p| p.keys().any(|key| key != "npm" && key != "api")) {
        return None;
    }
    let effective = |key: &str| {
        override_provider
            .and_then(|p| p.get(key))
            .or_else(|| map.get(key))
            .or_else(|| provider.get(key))
    };
    if effective("npm")?.as_str() != Some(CHAT_SDK) || !valid_api(effective("api")?) {
        return None;
    }
    // models.dev denotes current models with no status; deprecated models have
    // an explicit status. Unknown states cannot be considered active.
    if !matches!(
        map.get("status").and_then(Value::as_str),
        None | Some("active")
    ) || map.get("status").is_some_and(|v| !v.is_string())
        || map.get("tool_call") != Some(&Value::Bool(true))
    {
        return None;
    }
    let modalities = map.get("modalities").and_then(object)?;
    for direction in ["input", "output"] {
        if !modalities
            .get(direction)?
            .as_array()?
            .iter()
            .any(|v| v.as_str() == Some("text"))
        {
            return None;
        }
    }
    if !price_zero(map.get("cost")?)
        || ["pricing", "price", "billing", "surcharge", "rate"]
            .iter()
            .any(|field| {
                map.contains_key(*field)
                    || provider.contains_key(*field)
                    || override_provider.is_some_and(|p| p.contains_key(*field))
            })
    {
        return None;
    }
    let limit = map.get("limit").and_then(object)?;
    if !["context", "output"]
        .iter()
        .all(|key| limit.get(*key).is_some_and(positive))
        || limit.get("input").is_some_and(|v| !positive(v))
    {
        return None;
    }
    let name = map.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let mut safe_limit = Map::new();
    for key in ["context", "input", "output"] {
        if let Some(value) = limit.get(key) {
            safe_limit.insert(key.into(), value.clone());
        }
    }
    let mut safe_modalities = Map::new();
    for direction in ["input", "output"] {
        let values = modalities.get(direction)?.as_array()?;
        if values.iter().any(|v| {
            !matches!(
                v.as_str(),
                Some("text" | "image" | "audio" | "video" | "pdf")
            )
        }) {
            return None;
        }
        safe_modalities.insert(direction.into(), Value::Array(values.clone()));
    }
    let mut entry = Map::new();
    entry.insert("name".into(), Value::String(name.to_owned()));
    entry.insert("limit".into(), Value::Object(safe_limit));
    entry.insert("modalities".into(), Value::Object(safe_modalities));
    entry.insert("tool_call".into(), Value::Bool(true));
    for key in ["description", "reasoning", "attachment"] {
        if let Some(value) = map.get(key) {
            if (key == "description" && !value.is_string())
                || (key != "description" && !value.is_boolean())
            {
                return None;
            }
            entry.insert(key.into(), value.clone());
        }
    }
    Some(Value::Object(entry))
}

/// Parse models.dev `opencode.models` and the live `/zen/v1/models` list.
/// All admitted rows are new snapshots; a retired ID is never carried over.
pub fn parse(
    metadata: &Value,
    availability: &Value,
) -> Result<BTreeMap<String, Value>, ZenCatalogError> {
    let provider = metadata
        .get("opencode")
        .and_then(object)
        .ok_or(ZenCatalogError::InvalidResponse)?;
    if provider.keys().any(|key| {
        !matches!(
            key.as_str(),
            "api" | "doc" | "env" | "id" | "models" | "name" | "npm"
        )
    }) {
        return Err(ZenCatalogError::InvalidResponse);
    }
    if provider.get("id").and_then(Value::as_str) != Some("opencode")
        || provider.get("npm").and_then(Value::as_str) != Some(CHAT_SDK)
        || !provider.get("api").is_some_and(valid_api)
    {
        return Err(ZenCatalogError::InvalidResponse);
    }
    let models = provider
        .get("models")
        .and_then(object)
        .ok_or(ZenCatalogError::InvalidResponse)?;
    let list = availability
        .as_object()
        .ok_or(ZenCatalogError::InvalidResponse)?;
    if list.get("object").and_then(Value::as_str) != Some("list") {
        return Err(ZenCatalogError::InvalidResponse);
    }
    let data = list
        .get("data")
        .and_then(Value::as_array)
        .ok_or(ZenCatalogError::InvalidResponse)?;
    if models.len() > ROW_CAP || data.len() > ROW_CAP {
        return Err(ZenCatalogError::InvalidResponse);
    }
    let mut live = BTreeSet::new();
    for row in data {
        let fields = object(row).ok_or(ZenCatalogError::InvalidResponse)?;
        let id = fields
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .ok_or(ZenCatalogError::InvalidResponse)?;
        if fields.get("object").and_then(Value::as_str) != Some("model") || !live.insert(id) {
            return Err(ZenCatalogError::InvalidResponse);
        }
    }
    let result: BTreeMap<_, _> = live
        .into_iter()
        .filter_map(|id| {
            models
                .get(id)
                .and_then(|row| eligible_model(row, id, provider))
                .map(|config| (id.to_owned(), config))
        })
        .collect();
    if result.is_empty() {
        Err(ZenCatalogError::NoEligibleModels)
    } else {
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    #[ignore = "opt-in public metadata GET; no generation or credentials"]
    async fn read_only_official_catalog_preflight() {
        let result = fetch("https://models.dev/api.json", ZEN_API, false)
            .await
            .expect("public Zen Free catalog must have compatible models");
        println!(
            "currently eligible public Zen Chat IDs: {:?}",
            result.keys().collect::<Vec<_>>()
        );
        assert!(!result.is_empty());
    }

    fn metadata() -> Value {
        json!({"opencode": {"id":"opencode", "api": ZEN_API, "npm": CHAT_SDK, "models": {
            "fixture/new": {"id":"fixture/new", "name":"Future", "description":"Metadata", "cost":{"input":0,"output":0,"cache_read":0,"cache_write":0}, "limit":{"context":16384,"input":12000,"output":2048}, "tool_call":true, "modalities":{"input":["text","image"],"output":["text"]}, "reasoning":false, "provider":{"npm":CHAT_SDK}, "headers":{"Authorization":"do not publish"}, "options":{"baseURL":"https://attacker.invalid"}},
            "fixture/retired": {"id":"fixture/retired", "name":"Retired", "status":"deprecated", "cost":{"input":0,"output":0}, "limit":{"context":16384,"output":2048}, "tool_call":true, "modalities":{"input":["text"],"output":["text"]}}
        }}})
    }
    fn inventory(ids: &[&str]) -> Value {
        json!({"object":"list", "data": ids.iter().map(|id| json!({"id":id,"object":"model"})).collect::<Vec<_>>()})
    }
    #[test]
    fn admits_only_intersection_and_safe_metadata() {
        let out = parse(&metadata(), &inventory(&["fixture/new", "fixture/retired"])).unwrap();
        assert_eq!(out.len(), 1);
        let row = &out["fixture/new"];
        assert_eq!(row["limit"]["input"], 12000);
        assert_eq!(row["description"], "Metadata");
        for forbidden in [
            "npm", "provider", "api", "cost", "headers", "options", "baseURL",
        ] {
            assert!(!row.to_string().contains(forbidden));
        }
        assert_eq!(
            parse(&metadata(), &inventory(&["fixture/retired"])),
            Err(ZenCatalogError::NoEligibleModels)
        );
        let mut nested = metadata();
        nested["opencode"]["models"]["fixture/new"]["limit"]["headers"] =
            json!({"Authorization":"nested-secret"});
        nested["opencode"]["models"]["fixture/new"]["cost"] = json!({
            "input":0,"output":0,"context_over_200k":{"input":0,"output":0},
            "tiers":[{"tier":{"type":"context","size":200000},"input":0,"output":0}]
        });
        let row = parse(&nested, &inventory(&["fixture/new"]))
            .unwrap()
            .remove("fixture/new")
            .unwrap();
        assert!(!row.to_string().contains("nested-secret"));
    }
    #[test]
    fn fails_closed_on_price_protocol_state_and_missing_capabilities() {
        let variants = [
            ("cost", json!({"input":0,"output":0,"cache_read":0.01})),
            (
                "cost",
                json!({"input":0,"output":0,"tiers":[{"tier":{"type":"context","size":200000},"input":0,"output":1}]}),
            ),
            (
                "cost",
                json!({"input":0,"output":0,"context_over_200k":{"input":1,"output":0}}),
            ),
            ("cost", json!({"input":0,"output":0,"unknown_fee":0})),
            ("cost", json!({"input":0})),
            ("provider", json!({"npm":"@ai-sdk/openai"})),
            (
                "provider",
                json!({"npm":"@ai-sdk/openai-compatible", "api":"https://attacker.invalid"}),
            ),
            ("status", json!("inactive")),
            ("tool_call", json!(false)),
            ("modalities", json!({"input":["image"],"output":["text"]})),
            ("modalities", json!({"input":["text"],"output":["image"]})),
            ("limit", json!({"context":16384})),
        ];
        for (field, value) in variants {
            let mut changed = metadata();
            changed["opencode"]["models"]["fixture/new"][field] = value;
            assert_eq!(
                parse(&changed, &inventory(&["fixture/new"])),
                Err(ZenCatalogError::NoEligibleModels),
                "field {field}"
            );
        }
        let mut missing = metadata();
        missing["opencode"]["models"]["fixture/new"]
            .as_object_mut()
            .unwrap()
            .remove("cost");
        assert_eq!(
            parse(&missing, &inventory(&["fixture/new"])),
            Err(ZenCatalogError::NoEligibleModels)
        );
        missing = metadata();
        missing["opencode"]["models"]["fixture/new"]["new_billing_dimension"] = json!(0);
        assert_eq!(
            parse(&missing, &inventory(&["fixture/new"])),
            Err(ZenCatalogError::NoEligibleModels)
        );
    }
    #[test]
    fn rejects_hostile_inventory_and_bad_endpoints() {
        for bad in [
            json!({"data":[]}),
            inventory(&["fixture/new", "fixture/new"]),
            json!({"object":"list","data":[{"id":123,"object":"model"}]}),
        ] {
            assert_eq!(
                parse(&metadata(), &bad),
                Err(ZenCatalogError::InvalidResponse)
            );
        }
        for url in [
            "http://169.254.169.254/api.json",
            "https://user:pass@models.dev/api.json",
            "https://models.dev/api.json?x=1",
            "file:///api.json",
            "https://evil.invalid/api.json",
        ] {
            assert_eq!(
                endpoint(url, "models.dev", "/api.json", false),
                Err(ZenCatalogError::InvalidEndpoint)
            );
        }
    }

    #[tokio::test]
    async fn fetch_uses_only_two_bounded_public_gets_and_refuses_redirects() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let meta = metadata().to_string();
        let live = inventory(&["fixture/new"]).to_string();
        let server = tokio::spawn(async move {
            for (path, body) in [("/api.json", meta), ("/zen/v1/models", live)] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut req = [0; 4096];
                let size = stream.read(&mut req).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&req[..size])
                        .starts_with(&format!("GET {path} HTTP/1.1"))
                );
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let result = fetch(
            &format!("http://{address}/api.json"),
            &format!("http://{address}/zen/v1"),
            true,
        )
        .await
        .unwrap();
        assert_eq!(result.len(), 1);
        server.await.unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut req = [0; 4096];
            assert!(stream.read(&mut req).await.unwrap() > 0);
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/api.json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        assert_eq!(
            fetch(
                &format!("http://{address}/api.json"),
                &format!("http://{address}/zen/v1"),
                true
            )
            .await,
            Err(ZenCatalogError::Request)
        );
        server.await.unwrap();
    }
}
