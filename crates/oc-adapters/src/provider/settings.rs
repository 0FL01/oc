//! Typed, model-name-independent request options. Parsed at config admission;
//! every request lane consumes the same captured values before profile overlays.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{ProviderError, RequestOverlay, protocol::Protocol};

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct WireSettings {
    pub effort: Option<String>,
    summary: Option<String>,
    verbosity: Option<String>,
    thinking: Option<Thinking>,
    output: Option<OutputConfig>,
    pub body: Map<String, Value>,
}

impl std::fmt::Debug for WireSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WireSettings").finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum Thinking {
    Enabled {
        #[serde(alias = "budgetTokens")]
        budget_tokens: u64,
        #[serde(flatten)]
        fields: ThinkingFields,
    },
    Adaptive {
        #[serde(flatten)]
        fields: ThinkingFields,
    },
    Disabled {},
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThinkingFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    display: Option<Display>,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_binding: Option<BlockBinding>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Display {
    Summarized,
    Omitted,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockBinding {
    prefix_mismatch_behavior: Mismatch,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mismatch {
    Error,
    DropBlock,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<JsonFormat>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum JsonFormat {
    JsonSchema { schema: Map<String, Value> },
}

impl WireSettings {
    pub(crate) fn overlay(
        &self,
        protocol: Protocol,
        overlay: &BTreeMap<String, Value>,
    ) -> Result<Self, ProviderError> {
        let mut raw = BTreeMap::new();
        for (key, value) in [
            ("reasoningEffort", &self.effort),
            ("reasoningSummary", &self.summary),
            ("textVerbosity", &self.verbosity),
        ] {
            if let Some(value) = value {
                raw.insert(key.into(), Value::String(value.clone()));
            }
        }
        if let Some(value) = &self.thinking {
            raw.insert(
                "thinking".into(),
                serde_json::to_value(value).map_err(|_| ProviderError::InvalidConfig)?,
            );
        }
        if let Some(value) = &self.output {
            raw.insert(
                "outputConfig".into(),
                serde_json::to_value(value).map_err(|_| ProviderError::InvalidConfig)?,
            );
        }
        raw.extend(overlay.clone());
        Self::admit(protocol, &raw, &self.body)
    }
    pub(crate) fn admit(
        protocol: Protocol,
        raw: &BTreeMap<String, Value>,
        body: &Map<String, Value>,
    ) -> Result<Self, ProviderError> {
        let invalid = || ProviderError::InvalidConfig;
        let string = |key| {
            raw.get(key)
                .map(|value| {
                    value
                        .as_str()
                        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
                        .map(str::to_owned)
                        .ok_or_else(invalid)
                })
                .transpose()
        };
        RequestOverlay {
            headers: Default::default(),
            body: body.clone(),
        }
        .validate()
        .map_err(|_| invalid())?;
        let mut settings = Self {
            effort: string("reasoningEffort")?,
            body: body.clone(),
            ..Self::default()
        };
        if raw.contains_key("reasoningSummary") || raw.contains_key("textVerbosity") {
            if protocol != Protocol::Responses {
                return Err(invalid());
            }
            settings.summary = string("reasoningSummary")?;
            settings.verbosity = string("textVerbosity")?;
        }
        if let Some(value) = raw.get("thinking") {
            if protocol != Protocol::Messages {
                return Err(invalid());
            }
            let thinking: Thinking =
                serde_json::from_value(value.clone()).map_err(|_| invalid())?;
            if matches!(
                thinking,
                Thinking::Enabled {
                    budget_tokens: 0,
                    ..
                }
            ) {
                return Err(invalid());
            }
            settings.thinking = Some(thinking);
        }
        let output = match (raw.get("outputConfig"), raw.get("output_config")) {
            (Some(a), Some(b)) if a != b => return Err(invalid()),
            (Some(value), _) | (_, Some(value)) => Some(value),
            _ => None,
        };
        if let Some(value) = output {
            if protocol != Protocol::Messages {
                return Err(invalid());
            }
            let output: OutputConfig =
                serde_json::from_value(value.clone()).map_err(|_| invalid())?;
            if output
                .effort
                .as_deref()
                .is_some_and(|effort| effort.is_empty() || effort.chars().any(char::is_control))
            {
                return Err(invalid());
            }
            settings.output = Some(output);
        }
        Ok(settings)
    }

    pub(crate) fn apply(&self, protocol: Protocol, body: &mut Value, effort: Option<&str>) {
        match protocol {
            Protocol::Responses => {
                if let Some(summary) = &self.summary {
                    body["reasoning"]["summary"] = summary.clone().into();
                }
                if let Some(verbosity) = &self.verbosity {
                    body["text"]["verbosity"] = verbosity.clone().into();
                }
            }
            Protocol::Messages => {
                if let Some(thinking) = &self.thinking {
                    body["thinking"] = serde_json::to_value(thinking).expect("typed thinking");
                }
                if let Some(output) = &self.output {
                    body["output_config"] =
                        serde_json::to_value(output).expect("typed output config");
                }
                // A committed variant is authoritative over provider defaults.
                if let Some(effort) = effort {
                    body["output_config"]["effort"] = effort.into();
                }
            }
            Protocol::Chat => {}
        }
        merge(body, &json!(self.body));
    }
}

pub(super) fn merge(target: &mut Value, overlay: &Value) {
    if let (Some(target), Some(overlay)) = (target.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            match target.get_mut(key) {
                Some(old) => merge(old, value),
                None => {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *target = overlay.clone();
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
