//! Ordered action/resource rules, intersected at authority boundaries.
//!
//! v2.0.12 uses last-match within a ruleset. Native central sources and agent
//! constraints remain independent authorities: later allow cannot erase their deny.

use crate::config::{ConfigError, Permission, legacy_key};
use std::collections::BTreeMap;

/// One upstream action/resource/effect rule.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Tool/action wildcard.
    pub action: String,
    /// Resource wildcard.
    pub resource: String,
    /// Allow, ask, or deny.
    pub effect: Permission,
}

/// Authoritative ordered rules plus independently intersected constraints.
/// The legacy scalar map is a compatibility summary, not a resource grant.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PermissionRules {
    authorities: Vec<Vec<Rule>>,
    constraints: Vec<Vec<Rule>>,
}

fn strictest(a: Permission, b: Permission) -> Permission {
    match (a, b) {
        (Permission::Deny, _) | (_, Permission::Deny) => Permission::Deny,
        (Permission::Ask, _) | (_, Permission::Ask) => Permission::Ask,
        _ => Permission::Allow,
    }
}

impl PermissionRules {
    /// Parse the legacy singular, native plural (or native-profile map), and
    /// legacy boolean tool constraints without dropping order or resources.
    pub fn from_config(value: &serde_json::Value) -> Result<Self, ConfigError> {
        let mut result = Self::default();
        let mut rules = Vec::new();
        for key in ["permission", "permissions"] {
            if let Some(raw) = value.get(key) {
                rules.extend(parse(raw, key)?);
            }
        }
        if !rules.is_empty() {
            result.authorities.push(rules);
        }
        if let Some(tools) = value.get("tools") {
            let tools = tools.as_object().ok_or_else(|| invalid("tools"))?;
            let mut denied = Vec::new();
            for (action, enabled) in tools {
                let enabled = enabled.as_bool().ok_or_else(|| invalid("tools"))?;
                // A legacy true flag does not grant authority by itself.
                if !enabled {
                    denied.push(Rule {
                        action: legacy_key(action).into(),
                        resource: "*".into(),
                        effect: Permission::Deny,
                    });
                }
            }
            result.constraints.push(denied);
        }
        Ok(result)
    }

    /// Add another central source, intersecting overlapping action authority.
    pub fn extend(&mut self, other: Self) {
        self.authorities.extend(other.authorities);
        self.constraints.extend(other.constraints);
    }

    /// Expand home only for path actions, never raw shell command resources.
    pub fn expand_home(&mut self, home: &str) {
        for rule in self
            .authorities
            .iter_mut()
            .chain(&mut self.constraints)
            .flatten()
        {
            if !matches!(
                rule.action.as_str(),
                "read" | "apply_patch" | "external_directory"
            ) {
                continue;
            }
            if matches!(rule.resource.as_str(), "~" | "$HOME") {
                rule.resource = home.to_string();
            } else if let Some(relative) = rule
                .resource
                .strip_prefix("~/")
                .or_else(|| rule.resource.strip_prefix("$HOME/"))
                .or_else(|| rule.resource.strip_prefix("$HOME\\"))
            {
                rule.resource = format!("{}/{relative}", home.trim_end_matches('/'));
            }
        }
    }

    /// Freeze programmatic scalar authority before appending an agent constraint.
    pub fn narrow(
        &mut self,
        base: &BTreeMap<String, Permission>,
        agent: &BTreeMap<String, Permission>,
        rules: &Self,
    ) {
        if self.authorities.is_empty() {
            self.authorities.push(scalar_rules(base));
        }
        if rules.authorities.is_empty() {
            self.constraints.push(scalar_rules(agent));
        } else {
            self.constraints.extend(rules.authorities.clone());
        }
        self.constraints.extend(rules.constraints.clone());
    }

    /// Add a native module's explicit grant, still bounded by existing rules.
    pub fn module_permission(&mut self, action: &str, effect: Permission) {
        self.authorities.push(vec![Rule {
            action: action.into(),
            resource: "*".into(),
            effect,
        }]);
    }

    /// Resolve one actual resource; unmatched resource maps ask, missing central
    /// action authority denies. Constraints can never introduce a grant.
    pub fn evaluate(
        &self,
        fallback: &BTreeMap<String, Permission>,
        action: &str,
        resource: &str,
    ) -> Permission {
        self.evaluate_actions(fallback, &[action], resource)
    }

    /// Match native wire and upstream MCP action aliases as one authority.
    pub fn evaluate_actions(
        &self,
        fallback: &BTreeMap<String, Permission>,
        actions: &[&str],
        resource: &str,
    ) -> Permission {
        let mut effect = None;
        if self.authorities.is_empty() {
            effect = evaluate_layer(&scalar_rules(fallback), actions, resource);
        } else {
            for layer in &self.authorities {
                if let Some(next) = evaluate_layer(layer, actions, resource) {
                    effect = Some(effect.map_or(next, |old| strictest(old, next)));
                }
            }
        }
        let mut effect = effect.unwrap_or(Permission::Deny);
        for layer in &self.constraints {
            if let Some(next) = evaluate_layer(layer, actions, resource) {
                effect = strictest(effect, next);
            }
        }
        effect
    }
}

fn scalar_rules(map: &BTreeMap<String, Permission>) -> Vec<Rule> {
    map.iter()
        .map(|(action, effect)| Rule {
            action: action.clone(),
            resource: "*".into(),
            effect: *effect,
        })
        .collect()
}

fn evaluate_layer(rules: &[Rule], actions: &[&str], resource: &str) -> Option<Permission> {
    let mut effect = None;
    for rule in rules {
        if actions.iter().any(|action| wildcard(action, &rule.action)) {
            effect.get_or_insert(Permission::Ask);
            if wildcard(resource, &rule.resource) {
                effect = Some(rule.effect);
            }
        }
    }
    effect
}

fn invalid(field: &str) -> ConfigError {
    ConfigError::Invalid {
        field: field.into(),
        reason: "expected permission action, resource map, or action/resource/effect rules".into(),
    }
}

fn parse(value: &serde_json::Value, field: &str) -> Result<Vec<Rule>, ConfigError> {
    let effect = |raw: &serde_json::Value| {
        serde_json::from_value::<Permission>(raw.clone()).map_err(|_| invalid(field))
    };
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .map(|raw| {
                let mut rule: Rule =
                    serde_json::from_value(raw.clone()).map_err(|_| invalid(field))?;
                rule.action = legacy_key(&rule.action).into();
                Ok(rule)
            })
            .collect();
    }
    if value.is_string() {
        return Ok(vec![Rule {
            action: "*".into(),
            resource: "*".into(),
            effect: effect(value)?,
        }]);
    }
    let mut rules = Vec::new();
    for (action, raw) in value.as_object().ok_or_else(|| invalid(field))? {
        let action = legacy_key(action);
        if let Some(map) = raw.as_object() {
            // Empty resource maps still constrain the action to ask.
            if map.is_empty() {
                rules.push(Rule {
                    action: action.into(),
                    resource: "*".into(),
                    effect: Permission::Ask,
                });
            }
            for (resource, raw) in map {
                rules.push(Rule {
                    action: action.into(),
                    resource: resource.clone(),
                    effect: effect(raw)?,
                });
            }
        } else {
            rules.push(Rule {
                action: action.into(),
                resource: "*".into(),
                effect: effect(raw)?,
            });
        }
    }
    Ok(rules)
}

/// Upstream wildcard semantics: `*` spans separators, `?` one character, and
/// a final ` *` also matches the command without arguments. No glob classes.
pub fn wildcard(input: &str, pattern: &str) -> bool {
    let input = input.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");
    if let Some(prefix) = pattern.strip_suffix(" *")
        && wildcard(&input, prefix)
    {
        return true;
    }
    let text: Vec<char> = input.chars().collect();
    let pat: Vec<char> = pattern.chars().collect();
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while i < text.len() {
        if j < pat.len() && pat[j] == '*' {
            star = Some(j);
            j += 1;
            retry = i;
        } else if j < pat.len() && (pat[j] == '?' || pat[j] == text[i]) {
            i += 1;
            j += 1;
        } else if let Some(at) = star {
            retry += 1;
            i = retry;
            j = at + 1;
        } else {
            return false;
        }
    }
    while j < pat.len() && pat[j] == '*' {
        j += 1;
    }
    j == pat.len()
}
