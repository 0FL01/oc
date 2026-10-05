//! Read-only projection of the existing generation and selected merged catalog.
use super::ModelCatalog;
use crate::config::Generation;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Input {
    query: Option<String>,
    provider: Option<String>,
    all: bool,
    limit: usize,
    offset: usize,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            query: None,
            provider: None,
            all: false,
            limit: 20,
            offset: 0,
        }
    }
}
pub(crate) fn parse(args: &Value) -> Result<Input, String> {
    // Optional means omitted, not null; serde Option otherwise accepts null.
    if args
        .as_object()
        .is_none_or(|o| o.values().any(Value::is_null))
    {
        return Err("invalid model lookup arguments".into());
    }
    let input: Input =
        serde_json::from_value(args.clone()).map_err(|_| "invalid model lookup arguments")?;
    if !(1..=100).contains(&input.limit)
        || input.query.as_ref().is_some_and(|v| v.len() > 4096)
        || input.provider.as_ref().is_some_and(|v| v.len() > 4096)
    {
        return Err("invalid model lookup bounds".into());
    }
    Ok(input)
}

#[cfg(test)]
pub(crate) fn execute(
    generation: &Generation,
    selected: &ModelCatalog,
    args: &Value,
    secrets: &[String],
) -> Result<String, String> {
    execute_with_public(generation, selected, args, secrets, None)
}

pub(crate) fn wants_public(generation: &Generation, args: &Value) -> Result<bool, String> {
    let input = parse(args)?;
    Ok(generation.public_go_enabled
        && input.provider.as_deref().is_none_or(|provider| {
            provider.eq_ignore_ascii_case(crate::models_dev::PROVIDER)
                || provider.eq_ignore_ascii_case("OpenCode Go")
        }))
}

pub(crate) fn execute_with_public(
    generation: &Generation,
    selected: &ModelCatalog,
    args: &Value,
    secrets: &[String],
    public: Option<&crate::models_dev::CatalogRead>,
) -> Result<String, String> {
    let input = parse(args)?;
    let terms: Vec<_> = input
        .query
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let provider_filter = input.provider.as_deref().map(str::to_lowercase);
    let mut matching = Vec::new();
    // The selected catalog is supplied even for narrow Runtime fixtures whose
    // generation carries no provider config. No synthetic model is introduced.
    let providers: std::collections::BTreeSet<_> = generation
        .providers
        .keys()
        .map(String::as_str)
        .chain(std::iter::once(selected.provider.as_str()))
        .chain(public.map(|_| crate::models_dev::PROVIDER))
        .collect();
    for provider in providers {
        let configured = generation.providers.get(provider);
        let name = configured.and_then(|p| p.name.as_deref()).unwrap_or(
            if provider == crate::models_dev::PROVIDER {
                "OpenCode Go"
            } else {
                provider
            },
        );
        if provider_filter.as_ref().is_some_and(|filter| {
            provider.to_lowercase() != *filter && name.to_lowercase() != *filter
        }) {
            continue;
        }
        let models = if provider == crate::models_dev::PROVIDER && public.is_some() {
            public.map(|read| &read.models)
        } else if provider == selected.provider {
            Some(&selected.models)
        } else {
            configured.map(|p| &p.models)
        };
        for (id, entry) in models.into_iter().flatten() {
            if matching.len() >= super::CATALOG_ROWS_CAP {
                return Err("model lookup catalog capacity exceeded".into());
            }
            let model_name = entry.get("name").and_then(Value::as_str).unwrap_or(id);
            if provider.len() + id.len() + model_name.len() > super::CATALOG_LABEL_BYTES_CAP {
                return Err("model lookup metadata capacity exceeded".into());
            }
            let text = format!("{provider}/{id} {model_name}").to_lowercase();
            if terms.iter().all(|term| text.contains(term)) {
                let released = entry
                    .pointer("/time/released")
                    .or_else(|| entry.get("released"))
                    .and_then(Value::as_u64);
                let family = entry.get("family").and_then(Value::as_str);
                matching.push((provider, name, id, model_name, entry, released, family));
            }
        }
    }
    matching.sort_by(|a, b| {
        (b.0 == selected.provider)
            .cmp(&(a.0 == selected.provider))
            .then_with(|| a.0.cmp(b.0))
            .then_with(|| b.5.cmp(&a.5))
    });
    let mut families = HashSet::new();
    matching.retain(|m| input.all || m.6.is_none_or(|family| families.insert((m.0, family))));
    let total = matching.len();
    let mut groups: Vec<Value> = Vec::new();
    for (provider, name, id, model_name, entry, released, family) in
        matching.into_iter().skip(input.offset).take(input.limit)
    {
        let variants: Vec<_> = super::ordered_variants(entry)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut cost = serde_json::Map::new();
        if let Some(values) = entry.get("cost").and_then(Value::as_object) {
            for key in ["input", "output", "cache_read", "cache_write"] {
                if let Some(v) = values
                    .get(key)
                    .filter(|v| v.as_f64().is_some_and(|n| n >= 0.0))
                {
                    cost.insert(key.into(), v.clone());
                }
            }
        }
        let model = json!({"id":format!("{provider}/{id}"),"name":model_name,"released":released,"family":family,
            "variants":variants,"cost":if cost.is_empty(){Value::Null}else{Value::Object(cost)},
            "status":entry.get("status").and_then(Value::as_str)});
        if groups.last().is_none_or(|g| g["id"] != provider) {
            groups.push(json!({"id":provider,"name":name,"models":[]}));
        }
        groups.last_mut().expect("provider group")["models"]
            .as_array_mut()
            .expect("models")
            .push(model);
    }
    let next = input.offset.saturating_add(input.limit);
    let mut result =
        json!({"providers":groups,"total":total,"nextOffset":(next < total).then_some(next)});
    if let Some(public) = public {
        result["publicCatalog"] = json!({"source":crate::models_dev::SOURCE,
            "fetchedAt":public.fetched_at_ms,"status":if public.failure.is_some() {"failed"}
                else if public.fetched_at_ms.is_some() {"ready"} else {"unavailable"}});
    }
    // Only schema-known public fields above survive. Redact before returning;
    // an oversized page fails explicitly rather than corrupting pagination.
    if result.to_string().len() > 65536 {
        return Err("model lookup page capacity exceeded".into());
    }
    crate::mcp_result::redact_json(&mut result, secrets);
    let output = result.to_string();
    if output.len() > 65536 {
        return Err("model lookup page capacity exceeded".into());
    }
    Ok(output)
}
