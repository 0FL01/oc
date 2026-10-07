//! Shared public Go/OpenAI metadata, distinct from authenticated discovery.
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
pub const OPENAI: &str = "openai";
pub(crate) const CACHE_KEY: &str = "public-catalog:https://models.dev/api.json:opencode-go:v1";
const TTL_MS: u64 = 300_000;
const DEADLINE: Duration = Duration::from_secs(15);

/// Retain only the two public slices this owner can publish. Building a Value
/// tree for every unrelated provider amplifies the bounded wire body manyfold.
struct PublicDocument(BTreeMap<String, Value>);
impl<'de> Deserialize<'de> for PublicDocument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DocumentVisitor;
        impl<'de> serde::de::Visitor<'de> for DocumentVisitor {
            type Value = PublicDocument;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a public provider object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut slices = BTreeMap::new();
                while let Some(id) = map.next_key::<String>()? {
                    if matches!(id.as_str(), PROVIDER | OPENAI) {
                        // Preserve Value's last-key-wins semantics, including
                        // duplicate fields inside a recognized provider record.
                        slices.insert(id, map.next_value()?);
                    } else {
                        map.next_value::<CheckedDiscard>()?;
                    }
                }
                Ok(PublicDocument(slices))
            }
        }
        deserializer.deserialize_map(DocumentVisitor)
    }
}

/// Visit foreign JSON without retaining it. Unlike deserialize_ignored_any's
/// optimized skip, deserialize_any retains the original number/depth checks.
struct CheckedDiscard;
impl<'de> Deserialize<'de> for CheckedDiscard {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Self)
    }
}
impl<'de> serde::de::Visitor<'de> for CheckedDiscard {
    type Value = Self;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("valid JSON")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_seq<S: serde::de::SeqAccess<'de>>(self, mut seq: S) -> Result<Self, S::Error> {
        while seq.next_element::<Self>()?.is_some() {}
        Ok(self)
    }
    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self, M::Error> {
        while map.next_key::<Self>()?.is_some() {
            map.next_value::<Self>()?;
        }
        Ok(self)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ProviderRecord {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<ModelBody>,
}
#[derive(Clone, Serialize, Deserialize)]
struct ModelBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningBody>,
}
#[derive(Clone, Serialize, Deserialize)]
struct ReasoningBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
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

impl ProviderRecord {
    fn validate(&self) -> Result<(), discovery::DiscoveryError> {
        use discovery::DiscoveryError::InvalidResponse;
        if !matches!(self.id.as_str(), PROVIDER | OPENAI)
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
            if model
                .body
                .as_ref()
                .and_then(|b| b.reasoning.as_ref())
                .and_then(|r| r.mode.as_deref())
                .is_some_and(|v| !label(v))
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
            let protocol = crate::config::package_protocol(&self.id, Some(effective_package)).ok();
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
            if let Some(body) = &model.body {
                metadata["body"] = json!(body);
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
    record: Option<ProviderRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openai: Option<ProviderRecord>,
}
impl CacheRecord {
    fn provider(&self, id: &str) -> Option<&ProviderRecord> {
        match id {
            PROVIDER => self.record.as_ref(),
            OPENAI => self.openai.as_ref(),
            _ => None,
        }
    }
    fn valid(&self) -> bool {
        (self.record.is_some() || self.openai.is_some())
            && [(PROVIDER, &self.record), (OPENAI, &self.openai)]
                .into_iter()
                .all(|(id, r)| {
                    r.as_ref()
                        .is_none_or(|r| r.id == id && r.validate().is_ok())
                })
    }
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

/// One public source/cache owner. The historical Go name preserves existing users;
/// both provider slices share its fetch, bounded cache and single flight.
pub struct GoCatalog {
    state: tokio::sync::Mutex<CacheState>,
    flight: tokio::sync::Mutex<()>,
    revision: AtomicU64,
}
impl GoCatalog {
    /// Read a bounded source-qualified cache. Corrupt/foreign cache is never used.
    pub fn open(db: &Db) -> Self {
        let raw = match db.get_pref_bounded(CACHE_KEY, discovery::DISCOVERY_BODY_CAP) {
            Ok(BoundedPref::Value(raw)) => Some(raw),
            _ => None,
        };
        Self::from_cache(raw.as_deref())
    }

    /// Catalog-only process: bounded public-pref read, no native store owner or
    /// mutation. Missing/unusable WAL cache is a cache miss, never recovery.
    pub(crate) fn read_only(root: Option<&std::path::Path>) -> Self {
        let raw = root.and_then(Db::public_cache_read_only);
        Self::from_cache(raw.as_deref())
    }

    fn from_cache(raw: Option<&str>) -> Self {
        let record = raw
            .and_then(|raw| serde_json::from_str::<CacheRecord>(raw).ok())
            .filter(|cache| cache.source == SOURCE && cache.valid());
        Self {
            state: tokio::sync::Mutex::new(CacheState {
                cache: record,
                failure: None,
            }),
            revision: AtomicU64::new(0),
            flight: tokio::sync::Mutex::new(()),
        }
    }

    /// Immediately usable public last-good data, independent of credential readiness.
    pub async fn read(&self, local: &BTreeMap<String, Value>) -> CatalogRead {
        self.read_provider(PROVIDER, local).await
    }

    pub async fn read_provider(
        &self,
        provider: &str,
        local: &BTreeMap<String, Value>,
    ) -> CatalogRead {
        let state = self.state.lock().await;
        Self::view(state.cache.as_ref(), provider, local, false, state.failure)
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
        self.refresh_inner(Some(db), client, PROVIDER, local, now_ms, force)
            .await
    }

    pub async fn refresh_provider(
        &self,
        db: &Db,
        client: &impl discovery::DiscoveryClient,
        provider: &str,
        local: &BTreeMap<String, Value>,
        now_ms: u64,
        force: bool,
    ) -> CatalogRead {
        self.refresh_inner(Some(db), client, provider, local, now_ms, force)
            .await
    }

    /// Same public fetch/read-view in a read-only process. It never persists.
    pub async fn refresh_read_only(
        &self,
        client: &impl discovery::DiscoveryClient,
        local: &BTreeMap<String, Value>,
        now_ms: u64,
    ) -> CatalogRead {
        self.refresh_inner(None, client, PROVIDER, local, now_ms, false)
            .await
    }

    pub async fn refresh_provider_read_only(
        &self,
        client: &impl discovery::DiscoveryClient,
        provider: &str,
        local: &BTreeMap<String, Value>,
        now_ms: u64,
    ) -> CatalogRead {
        self.refresh_inner(None, client, provider, local, now_ms, false)
            .await
    }

    async fn refresh_inner(
        &self,
        db: Option<&Db>,
        client: &impl discovery::DiscoveryClient,
        provider: &str,
        local: &BTreeMap<String, Value>,
        now_ms: u64,
        force: bool,
    ) -> CatalogRead {
        if !matches!(provider, PROVIDER | OPENAI) {
            return Self::view(
                None,
                provider,
                local,
                false,
                Some(discovery::DiscoveryFailure::InvalidConfig),
            );
        }
        let revision = self.revision.load(Ordering::SeqCst);
        let _flight = self.flight.lock().await;
        let state = self.state.lock().await;
        if revision != self.revision.load(Ordering::SeqCst)
            || (!force
                && state.cache.as_ref().is_some_and(|c| {
                    c.provider(provider).is_some()
                        && now_ms >= c.fetched_at_ms
                        && now_ms - c.fetched_at_ms < TTL_MS
                }))
        {
            return Self::view(state.cache.as_ref(), provider, local, false, state.failure);
        }
        drop(state); // Cached reads must not wait for the bounded network job.
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
            let mut root: PublicDocument = serde_json::from_slice(&body)
                .map_err(|_| discovery::DiscoveryError::InvalidResponse)?;
            let mut slice =
                |id: &str| -> Result<Option<ProviderRecord>, discovery::DiscoveryError> {
                    root.0
                        .remove(id)
                        .map(|value| {
                            let record: ProviderRecord = serde_json::from_value(value)
                                .map_err(|_| discovery::DiscoveryError::InvalidResponse)?;
                            if record.id != id {
                                return Err(discovery::DiscoveryError::InvalidResponse);
                            }
                            record.validate()?;
                            Ok(record)
                        })
                        .transpose()
                };
            let cache = CacheRecord {
                source: SOURCE.into(),
                fetched_at_ms: now_ms,
                record: slice(PROVIDER)?,
                openai: slice(OPENAI)?,
            };
            if cache.provider(provider).is_none() {
                return Err(discovery::DiscoveryError::InvalidResponse);
            }
            Ok(cache)
        })
        .await
        .unwrap_or(Err(discovery::DiscoveryError::Network));
        let mut state = self.state.lock().await;
        self.revision.fetch_add(1, Ordering::SeqCst);
        match fetch {
            Err(error) => {
                state.failure = Some(discovery::DiscoveryFailure::from(&error));
                Self::view(state.cache.as_ref(), provider, local, false, state.failure)
            }
            Ok(cache) => {
                let changed = state.cache.as_ref().is_none_or(|c| {
                    serde_json::to_value(c.provider(provider)).ok()
                        != serde_json::to_value(cache.provider(provider)).ok()
                });
                if let Some(db) = db
                    && let Ok(raw) = serde_json::to_string(&cache)
                    && raw.len() <= discovery::DISCOVERY_BODY_CAP
                {
                    let _ = db.set_pref(CACHE_KEY, &raw);
                }
                state.cache = Some(cache);
                state.failure = None;
                Self::view(state.cache.as_ref(), provider, local, changed, None)
            }
        }
    }
    fn view(
        cache: Option<&CacheRecord>,
        provider: &str,
        local: &BTreeMap<String, Value>,
        changed: bool,
        failure: Option<discovery::DiscoveryFailure>,
    ) -> CatalogRead {
        CatalogRead {
            models: cache
                .and_then(|c| c.provider(provider))
                .map(|r| r.models(local))
                .unwrap_or_default(),
            fetched_at_ms: cache
                .filter(|c| c.provider(provider).is_some())
                .map(|c| c.fetched_at_ms),
            changed,
            failure: failure.or_else(|| {
                cache
                    .filter(|c| c.provider(provider).is_none())
                    .map(|_| discovery::DiscoveryFailure::InvalidResponse)
            }),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod openai_tests;
