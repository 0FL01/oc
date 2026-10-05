//! Public Go metadata, distinct from authenticated OpenProxy discovery.
//! No remote connection inputs survive this owner, including in persisted cache.
use crate::{
    discovery,
    storage::{BoundedPref, Db},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// The sole production source; not the donor mirror or a configured provider URL.
pub const SOURCE: &str = "https://models.dev/api.json";
/// Public provider ID, independent of selection and credentials.
pub const PROVIDER: &str = "opencode-go";
const CACHE_KEY: &str = "public-catalog:https://models.dev/api.json:opencode-go:v1";
const TTL_MS: u64 = 300_000;
const DEADLINE: Duration = Duration::from_secs(15);

#[derive(Clone, Serialize, Deserialize)]
struct GoRecord {
    id: String,
    npm: String,
    models: BTreeMap<String, SourceModel>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SourceModel {
    id: String,
    name: String,
    limit: Limits,
    tool_call: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<ModelProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modalities: Option<Modalities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cost: Option<Cost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interleaved: Option<Interleaved>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_options: Option<Vec<ReasoningOption>>,
}
#[derive(Clone, Serialize, Deserialize)]
struct ModelProvider {
    npm: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Limits {
    context: u64,
    output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Modalities {
    input: Vec<String>,
    output: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Cost {
    input: f64,
    output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_write: Option<f64>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Status {
    Active,
    Alpha,
    Beta,
    Deprecated,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Interleaved {
    Bool(bool),
    Field(String),
    Object { field: String },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ReasoningOption {
    Effort {
        values: Vec<Option<String>>,
    },
    Toggle {},
    BudgetTokens {
        #[serde(default)]
        min: Option<u64>,
        #[serde(default)]
        max: Option<u64>,
    },
}

fn label(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= crate::models::CATALOG_LABEL_BYTES_CAP
        && !value.chars().any(char::is_control)
}

impl GoRecord {
    fn validate(&self) -> Result<(), discovery::DiscoveryError> {
        use discovery::DiscoveryError::InvalidResponse;
        if self.id != PROVIDER
            || !label(&self.npm)
            || self.models.len() > discovery::DISCOVERY_ROWS_CAP
        {
            return Err(InvalidResponse);
        }
        for (id, model) in &self.models {
            if !label(id)
                || model.id != *id
                || !label(&model.name)
                || model.limit.context == 0
                || model.limit.output == 0
                || model.limit.input == Some(0)
                || model
                    .provider
                    .as_ref()
                    .and_then(|p| p.npm.as_deref())
                    .is_some_and(|v| !label(v))
            {
                return Err(InvalidResponse);
            }
            if let Some(cost) = &model.cost
                && [
                    Some(cost.input),
                    Some(cost.output),
                    cost.cache_read,
                    cost.cache_write,
                ]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v < 0.0)
            {
                return Err(InvalidResponse);
            }
            if let Some(modalities) = &model.modalities
                && modalities
                    .input
                    .iter()
                    .chain(&modalities.output)
                    .any(|v| !label(v))
            {
                return Err(InvalidResponse);
            }
            let field = match &model.interleaved {
                Some(Interleaved::Field(s) | Interleaved::Object { field: s }) => Some(s),
                _ => None,
            };
            if field.is_some_and(|s| !label(s)) {
                return Err(InvalidResponse);
            }
            for option in model.reasoning_options.iter().flatten() {
                match option {
                    ReasoningOption::Effort { values }
                        if values.len() > 64 || values.iter().flatten().any(|v| !label(v)) =>
                    {
                        return Err(InvalidResponse);
                    }
                    ReasoningOption::BudgetTokens { min, max }
                        if *min == Some(0)
                            || *max == Some(0)
                            || min.zip(*max).is_some_and(|(a, b)| a > b) =>
                    {
                        return Err(InvalidResponse);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn models(&self, local: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        use crate::provider::protocol::Protocol;
        let mut models = BTreeMap::new();
        for (id, model) in &self.models {
            if model.status == Some(Status::Deprecated) {
                continue;
            }
            let package = model
                .provider
                .as_ref()
                .and_then(|p| p.npm.as_deref())
                .unwrap_or(&self.npm);
            let effective_package = local
                .get(id)
                .and_then(|m| m.get("package"))
                .and_then(Value::as_str)
                .unwrap_or(package);
            let protocol = crate::config::package_protocol(PROVIDER, Some(effective_package)).ok();
            let mut metadata = json!({"modelID": id, "name": model.name, "package": package,
                "limit": model.limit, "tool_call": model.tool_call,
                "capabilities": {"tools": model.tool_call}, "variants": []});
            if let Some(modalities) = &model.modalities {
                metadata["modalities"] = json!(modalities);
                metadata["capabilities"]["input"] = json!(modalities.input);
                metadata["capabilities"]["output"] = json!(modalities.output);
            }
            if let Some(cost) = &model.cost {
                metadata["cost"] = json!(cost);
            }
            if let Some(status) = &model.status {
                metadata["status"] = json!(status);
            }
            if let Some(options) = &model.reasoning_options {
                metadata["reasoning_options"] = json!(options);
            }
            if let Some(Interleaved::Field(s) | Interleaved::Object { field: s }) =
                &model.interleaved
            {
                metadata["compatibility"] = json!({"reasoningField": s});
            }
            let mut variants: Vec<Value> = Vec::new();
            // Only declared controls with an implemented generic lowering. No model-name defaults.
            let options = model.reasoning_options.as_deref().unwrap_or_default();
            let effort = options
                .iter()
                .find(|o| matches!(o, ReasoningOption::Effort { .. }));
            let budget = options
                .iter()
                .find(|o| matches!(o, ReasoningOption::BudgetTokens { .. }));
            let toggle = options
                .iter()
                .find(|o| matches!(o, ReasoningOption::Toggle {}));
            if protocol == Some(Protocol::Messages) && toggle.is_some() {
                variants.push(json!({"id":"none","settings":{"thinking":{"type":"disabled"}}}));
            }
            if let Some(option) = effort.or(budget).or(toggle) {
                match (protocol, option) {
                    (Some(_), ReasoningOption::Effort { values }) => {
                        for effort in values.iter().flatten().filter(|v| v.as_str() != "null") {
                            let mut settings = json!({"reasoningEffort": effort});
                            if protocol == Some(Protocol::Messages) {
                                settings["thinking"] = json!({"type":"adaptive"});
                            }
                            if !variants.iter().any(|v| v["id"] == *effort) {
                                variants.push(json!({"id": effort, "settings": settings}));
                            }
                        }
                    }
                    (Some(Protocol::Messages), ReasoningOption::Toggle {}) => {
                        variants.push(
                            json!({"id":"thinking","settings":{"thinking":{"type":"adaptive"}}}),
                        );
                    }
                    (Some(Protocol::Messages), ReasoningOption::BudgetTokens { min, max }) => {
                        let ceiling = model.limit.output.min(32_000);
                        let maximum = max
                            .unwrap_or(ceiling.saturating_sub(1))
                            .min(ceiling.saturating_sub(1));
                        if maximum > 0 && min.unwrap_or(1) <= maximum {
                            let high = min.unwrap_or(1).max(maximum.div_ceil(2)).min(maximum);
                            for (id, budget) in [("high", high), ("max", maximum)] {
                                if !variants.iter().any(|v| v["id"] == id) {
                                    variants.push(json!({"id":id,"settings":{"thinking":{"type":"enabled","budget_tokens":budget}}}));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            metadata["variants"] = json!(variants);
            if let Some(local) = local.get(id).and_then(Value::as_object) {
                let map = metadata.as_object_mut().expect("metadata object");
                for (key, value) in local {
                    if key != "limit" && key != "variants" {
                        map.insert(key.clone(), value.clone());
                    }
                }
                if let Some(limit) = local.get("limit").and_then(Value::as_object) {
                    map["limit"]
                        .as_object_mut()
                        .expect("typed limits")
                        .extend(limit.clone());
                }
                if let Some(local_variants) = local.get("variants") {
                    let local_variants: Vec<Value> = if let Some(array) = local_variants.as_array()
                    {
                        array.clone()
                    } else {
                        local_variants
                            .as_object()
                            .into_iter()
                            .flatten()
                            .map(|(id, value)| {
                                let mut value = value.clone();
                                value["id"] = json!(id);
                                value
                            })
                            .collect()
                    };
                    for variant in local_variants {
                        if let Some(existing) =
                            variants.iter_mut().find(|v| v["id"] == variant["id"])
                        {
                            *existing = variant;
                        } else {
                            variants.push(variant);
                        }
                    }
                    map.insert("variants".into(), json!(variants));
                }
            }
            if metadata.get("disabled").and_then(Value::as_bool) != Some(true) {
                models.insert(id.clone(), metadata);
            }
        }
        models
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct CacheRecord {
    source: String,
    fetched_at_ms: u64,
    record: GoRecord,
}
struct CacheState {
    cache: Option<CacheRecord>,
    failure: Option<discovery::DiscoveryFailure>,
}

/// Safe result: metadata only, never account or remote connection material.
#[derive(Clone)]
pub struct CatalogRead {
    /// Current remote base plus only current surviving local overrides.
    pub models: BTreeMap<String, Value>,
    /// Last validated fetch timestamp, not the last failed attempt.
    pub fetched_at_ms: Option<u64>,
    /// True when the validated public base changed (including valid retirement).
    pub changed: bool,
    /// Bounded refresh status; last-good data remains on failure.
    pub failure: Option<discovery::DiscoveryFailure>,
}
impl std::fmt::Debug for CatalogRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogRead")
            .field("model_count", &self.models.len())
            .field("fetched_at_ms", &self.fetched_at_ms)
            .field("changed", &self.changed)
            .field("failure", &self.failure)
            .finish()
    }
}

/// One public source/cache owner. Concurrent refreshes coalesce, including force.
pub struct GoCatalog {
    state: tokio::sync::Mutex<CacheState>,
    revision: AtomicU64,
}
impl GoCatalog {
    /// Read a bounded source-qualified cache. Corrupt/foreign cache is never used.
    pub fn open(db: &Db) -> Self {
        let record = match db.get_pref_bounded(CACHE_KEY, discovery::DISCOVERY_BODY_CAP) {
            Ok(BoundedPref::Value(raw)) => serde_json::from_str::<CacheRecord>(&raw)
                .ok()
                .filter(|cache| cache.source == SOURCE && cache.record.validate().is_ok()),
            _ => None,
        };
        Self {
            state: tokio::sync::Mutex::new(CacheState {
                cache: record,
                failure: None,
            }),
            revision: AtomicU64::new(0),
        }
    }

    /// Immediately usable public last-good data, independent of credential readiness.
    pub async fn read(&self, local: &BTreeMap<String, Value>) -> CatalogRead {
        let state = self.state.lock().await;
        Self::view(state.cache.as_ref(), local, false, state.failure)
    }

    /// One bounded credential-free GET. The caller owns cancellation by dropping
    /// this future; publication occurs only after complete validation, without awaits.
    pub async fn refresh(
        &self,
        db: &Db,
        client: &impl discovery::DiscoveryClient,
        local: &BTreeMap<String, Value>,
        now_ms: u64,
        force: bool,
    ) -> CatalogRead {
        let revision = self.revision.load(Ordering::SeqCst);
        let mut state = self.state.lock().await;
        if revision != self.revision.load(Ordering::SeqCst)
            || (!force
                && state.cache.as_ref().is_some_and(|c| {
                    now_ms >= c.fetched_at_ms && now_ms - c.fetched_at_ms < TTL_MS
                }))
        {
            return Self::view(state.cache.as_ref(), local, false, state.failure);
        }
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let fetch = tokio::time::timeout(DEADLINE, async {
            let (status, body) = client.get(SOURCE, &headers, DEADLINE).await?;
            if !(200..300).contains(&status) {
                return Err(discovery::DiscoveryError::Http { status });
            }
            if body.len() > discovery::DISCOVERY_BODY_CAP {
                return Err(discovery::DiscoveryError::InvalidResponse);
            }
            let root: Value = serde_json::from_slice(&body)
                .map_err(|_| discovery::DiscoveryError::InvalidResponse)?;
            let record: GoRecord = serde_json::from_value(
                root.get(PROVIDER)
                    .cloned()
                    .ok_or(discovery::DiscoveryError::InvalidResponse)?,
            )
            .map_err(|_| discovery::DiscoveryError::InvalidResponse)?;
            record.validate()?;
            Ok(record)
        })
        .await
        .unwrap_or(Err(discovery::DiscoveryError::Network));
        self.revision.fetch_add(1, Ordering::SeqCst);
        match fetch {
            Err(error) => {
                state.failure = Some(discovery::DiscoveryFailure::from(&error));
                Self::view(state.cache.as_ref(), local, false, state.failure)
            }
            Ok(record) => {
                let changed = state.cache.as_ref().is_none_or(|c| {
                    serde_json::to_value(&c.record).ok() != serde_json::to_value(&record).ok()
                });
                let cache = CacheRecord {
                    source: SOURCE.into(),
                    fetched_at_ms: now_ms,
                    record,
                };
                if let Ok(raw) = serde_json::to_string(&cache)
                    && raw.len() <= discovery::DISCOVERY_BODY_CAP
                {
                    let _ = db.set_pref(CACHE_KEY, &raw);
                }
                state.cache = Some(cache);
                state.failure = None;
                Self::view(state.cache.as_ref(), local, changed, None)
            }
        }
    }
    fn view(
        cache: Option<&CacheRecord>,
        local: &BTreeMap<String, Value>,
        changed: bool,
        failure: Option<discovery::DiscoveryFailure>,
    ) -> CatalogRead {
        CatalogRead {
            models: cache.map(|c| c.record.models(local)).unwrap_or_default(),
            fetched_at_ms: cache.map(|c| c.fetched_at_ms),
            changed,
            failure,
        }
    }
}

#[cfg(test)]
mod tests;
