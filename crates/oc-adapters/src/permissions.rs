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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionRules {
    authorities: Vec<Vec<Rule>>,
    constraints: Vec<Vec<Rule>>,
    // Explore's native defaults yield only to actual registered MCP. User
    // constraints stay separate and never receive this exception.
    builtin_defaults: Vec<Vec<Rule>>,
    // Narrow Plan baseline only fills missing central authority. Explicit
    // central/profile/parent rules remain independently intersected.
    builtin_grants: Vec<Rule>,
    plan_ceilings: Vec<Vec<Rule>>,
}

fn strictest(a: Permission, b: Permission) -> Permission {
    match (a, b) {
        (Permission::Deny, _) | (_, Permission::Deny) => Permission::Deny,
        (Permission::Ask, _) | (_, Permission::Ask) => Permission::Ask,
        _ => Permission::Allow,
    }
}

impl PermissionRules {
    pub(crate) fn has_plan_ceiling(&self) -> bool {
        !self.builtin_grants.is_empty()
    }

    /// Native policy resources inside Location are relative. Preserve equivalent
    /// absolute rules in the same ordered layers, only with a native Plan ceiling.
    pub(crate) fn bind_plan_project(&mut self, project: &std::path::Path) {
        let Some(grant) = self
            .builtin_grants
            .iter()
            .find(|rule| rule.action == "apply_patch")
        else {
            return;
        };
        if grant.resource.starts_with("~/") {
            // No captured HOME expanded this native baseline. A literal tilde
            // directory beneath Location is not the user's Plan directory.
            self.builtin_grants.clear();
            for layer in &mut self.plan_ceilings {
                layer.retain(|rule| {
                    rule.effect != Permission::Allow || !rule.resource.starts_with("~/")
                });
            }
            return;
        }
        if !std::path::Path::new(&grant.resource).starts_with(project) {
            return;
        }
        let equivalent = |rule: &Rule| {
            if !matches!(
                rule.action.as_str(),
                "*" | "apply_patch" | "external_directory"
            ) {
                return None;
            }
            let relative = std::path::Path::new(&rule.resource)
                .strip_prefix(project)
                .ok()?;
            let resource = if relative.as_os_str().is_empty() {
                ".".into()
            } else {
                relative.to_string_lossy().into_owned()
            };
            Some(Rule {
                resource,
                ..rule.clone()
            })
        };
        for layer in self
            .authorities
            .iter_mut()
            .chain(&mut self.constraints)
            .chain(&mut self.plan_ceilings)
        {
            let mut original = std::mem::take(layer).into_iter().peekable();
            while let Some(rule) = original.next() {
                let alias = equivalent(&rule);
                layer.push(rule);
                if let Some(alias) = alias
                    && original.peek() != Some(&alias)
                {
                    layer.push(alias);
                }
            }
        }
        for alias in self.builtin_grants.clone().iter().filter_map(equivalent) {
            if !self.builtin_grants.contains(&alias) {
                self.builtin_grants.push(alias);
            }
        }
    }
    /// Parse the legacy singular, native plural (or native-profile map), and
    /// legacy boolean tool constraints without dropping order or resources.
    pub fn from_config(value: &serde_json::Value) -> Result<Self, ConfigError> {
        let mut result = Self::default();
        let mut rules = Vec::new();
        let has_object_alias = |action: &str| {
            ["permission", "permissions"].iter().any(|key| {
                value
                    .get(key)
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|map| map.contains_key(action))
            })
        };
        let intersect_shell_aliases = has_object_alias("bash") && has_object_alias("shell");
        for key in ["permission", "permissions"] {
            if let Some(raw) = value.get(key) {
                // Object aliases are one policy identity, not an alternate
                // route around the stricter legacy rule. Ordered rule arrays
                // retain their existing explicit last-match semantics.
                if let Some(map) = raw
                    .as_object()
                    .filter(|map| intersect_shell_aliases && map.contains_key("shell"))
                {
                    let mut rest = map.clone();
                    let shell = rest.remove("shell").expect("present alias");
                    rules.extend(parse(&serde_json::Value::Object(rest), key)?);
                    result
                        .constraints
                        .push(parse(&serde_json::json!({"shell": shell}), key)?);
                    continue;
                }
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
        self.builtin_defaults.extend(other.builtin_defaults);
        self.builtin_grants.extend(other.builtin_grants);
        self.plan_ceilings.extend(other.plan_ceilings);
    }

    pub(super) fn plan_defaults() -> Self {
        let mut rules = Self::from_config(&serde_json::json!({"permission":[
            {"action":"edit","resource":"*","effect":"deny"},
            {"action":"edit","resource":"~/.opencode/plan/*","effect":"allow"}
        ]}))
        .expect("builtin Plan ceiling");
        rules.plan_ceilings = std::mem::take(&mut rules.authorities);
        rules.builtin_grants = vec![
            Rule {
                action: "apply_patch".into(),
                resource: "~/.opencode/plan/*".into(),
                effect: Permission::Allow,
            },
            Rule {
                action: "external_directory".into(),
                resource: "~/.opencode/plan/*".into(),
                effect: Permission::Allow,
            },
        ];
        rules
    }

    pub(super) fn explore_defaults() -> Self {
        let mut rules = Self::from_config(&serde_json::json!({"permission":[
            {"action":"*","resource":"*","effect":"deny"},
            {"action":"read","resource":"*","effect":"allow"},
            {"action":"glob","resource":"*","effect":"allow"},
            {"action":"grep","resource":"*","effect":"allow"},
            {"action":"webfetch","resource":"*","effect":"allow"},
            {"action":"websearch","resource":"*","effect":"allow"},
            {"action":"external_directory","resource":"*","effect":"ask"},
            {"action":"compress","resource":"*","effect":"allow"}
        ]}))
        .expect("builtin Explore defaults");
        rules.builtin_defaults = std::mem::take(&mut rules.authorities);
        // Fill absent central authority for own-history compression only.
        // Explicit central/profile/parent Ask/Deny still intersects this grant.
        rules.builtin_grants.push(Rule {
            action: "compress".into(),
            resource: "*".into(),
            effect: Permission::Allow,
        });
        rules
    }

    /// Expand home only for path actions, never raw shell command resources.
    pub fn expand_home(&mut self, home: &str) {
        for rule in self
            .authorities
            .iter_mut()
            .chain(&mut self.constraints)
            .chain(&mut self.builtin_defaults)
            .chain(&mut self.plan_ceilings)
            .flatten()
            .chain(&mut self.builtin_grants)
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
        self.builtin_defaults.extend(rules.builtin_defaults.clone());
        self.builtin_grants.extend(rules.builtin_grants.clone());
        self.plan_ceilings.extend(rules.plan_ceilings.clone());
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
        self.evaluate_registered(fallback, actions, resource, false, None)
    }

    /// Only an entry in the captured, connected registry supplies this grant.
    pub(crate) fn evaluate_registered(
        &self,
        fallback: &BTreeMap<String, Permission>,
        actions: &[&str],
        resource: &str,
        registered_mcp: bool,
        project: Option<&std::path::Path>,
    ) -> Permission {
        let actions: Vec<_> = actions.iter().map(|action| legacy_key(action)).collect();
        let actions = actions.as_slice();
        // One native path has both Location-relative and absolute spellings.
        // Match both within each ordered authority, rather than intersecting an
        // invented missing-match Ask or dropping an explicit rooted glob.
        let project = project.filter(|_| {
            self.has_plan_ceiling()
                && !registered_mcp
                && actions
                    .iter()
                    .any(|action| matches!(*action, "apply_patch" | "external_directory"))
        });
        let absolute = project.map(|root| root.join(resource).to_string_lossy().into_owned());
        let relative = project
            .and_then(|root| std::path::Path::new(resource).strip_prefix(root).ok())
            .map(|path| path.to_string_lossy().into_owned());
        let resources = [
            resource,
            absolute.as_deref().unwrap_or(resource),
            relative.as_deref().unwrap_or(resource),
        ];
        let mut effect = None;
        if self.authorities.is_empty() {
            effect = evaluate_layer_resources(&scalar_rules(fallback), actions, &resources);
        } else {
            for layer in &self.authorities {
                if let Some(next) = evaluate_layer_resources(layer, actions, &resources) {
                    effect = Some(effect.map_or(next, |old| strictest(old, next)));
                }
            }
        }
        let mut effect = effect
            .or_else(|| {
                self.builtin_grants
                    .iter()
                    .rev()
                    .find(|rule| {
                        actions.iter().any(|action| wildcard(action, &rule.action))
                            && resources
                                .iter()
                                .any(|resource| wildcard(resource, &rule.resource))
                    })
                    .map(|rule| rule.effect)
            })
            .unwrap_or(if registered_mcp {
                Permission::Allow
            } else {
                Permission::Deny
            });
        for layer in self
            .constraints
            .iter()
            .chain(&self.plan_ceilings)
            .chain(self.builtin_defaults.iter().filter(|_| !registered_mcp))
        {
            if let Some(next) = evaluate_layer_resources(layer, actions, &resources) {
                effect = strictest(effect, next);
            }
        }
        effect
    }

    /// Explicit cross-action ceiling for search. Missing read authority neither
    /// grants read nor denies independently authorized search.
    pub(crate) fn denied_by_rule(
        &self,
        fallback: &BTreeMap<String, Permission>,
        action: &str,
        resource: &str,
    ) -> bool {
        let actions = [legacy_key(action)];
        let scalar = scalar_rules(fallback);
        let authorities = if self.authorities.is_empty() {
            std::slice::from_ref(&scalar)
        } else {
            &self.authorities
        };
        authorities
            .iter()
            .chain(&self.constraints)
            .chain(&self.plan_ceilings)
            .any(|layer| evaluate_layer(layer, &actions, resource) == Some(Permission::Deny))
    }

    /// Automatic instruction sources have no approval consumer. Match the two
    /// names of this one source within each ordered authority, then intersect
    /// authorities/constraints as usual. An approval for another file is absent.
    pub(crate) fn automatic_source_allowed(
        &self,
        fallback: &BTreeMap<String, Permission>,
        resolved: &str,
        location: &str,
    ) -> bool {
        let layer_effect = |layer: &[Rule]| {
            let mut effect = None;
            for rule in layer {
                if wildcard("read", legacy_key(&rule.action)) {
                    effect.get_or_insert(Permission::Ask);
                    if wildcard(resolved, &rule.resource) || wildcard(location, &rule.resource) {
                        effect = Some(rule.effect);
                    }
                }
            }
            effect
        };
        let scalar = scalar_rules(fallback);
        let authorities = if self.authorities.is_empty() {
            std::slice::from_ref(&scalar)
        } else {
            &self.authorities
        };
        let mut effect = None;
        for layer in authorities {
            if let Some(next) = layer_effect(layer) {
                effect = Some(effect.map_or(next, |old| strictest(old, next)));
            }
        }
        let mut effect = effect.unwrap_or(Permission::Deny);
        for layer in self.constraints.iter().chain(&self.plan_ceilings) {
            if let Some(next) = layer_effect(layer) {
                effect = strictest(effect, next);
            }
        }
        effect == Permission::Allow
    }

    /// Catalog visibility is conservative for resource rules, but a whole-action
    /// denial at any authority boundary removes the definition. Execution still
    /// evaluates every actual resource; visibility never grants a call.
    pub(crate) fn actions_visible(
        &self,
        fallback: &BTreeMap<String, Permission>,
        actions: &[&str],
        registered_mcp: bool,
    ) -> bool {
        let actions: Vec<_> = actions.iter().map(|action| legacy_key(action)).collect();
        let scalar = scalar_rules(fallback);
        let authorities = if self.authorities.is_empty() {
            std::slice::from_ref(&scalar)
        } else {
            &self.authorities
        };
        let admitted = registered_mcp
            || self
                .builtin_grants
                .iter()
                .any(|rule| actions.iter().any(|action| wildcard(action, &rule.action)))
            || authorities.iter().flatten().any(|rule| {
                actions
                    .iter()
                    .any(|action| wildcard(action, legacy_key(&rule.action)))
            });
        for layer in authorities
            .iter()
            .chain(&self.constraints)
            .chain(&self.plan_ceilings)
            .chain(self.builtin_defaults.iter().filter(|_| !registered_mcp))
        {
            let matching: Vec<_> = layer
                .iter()
                .filter(|rule| {
                    actions
                        .iter()
                        .any(|action| wildcard(action, legacy_key(&rule.action)))
                })
                .collect();
            if matching.is_empty() {
                continue;
            }
            if evaluate_layer(layer, &actions, "*") == Some(Permission::Deny)
                && matching.iter().all(|rule| {
                    evaluate_layer(layer, &actions, &rule.resource) == Some(Permission::Deny)
                })
            {
                return false;
            }
        }
        admitted
    }
}

fn scalar_rules(map: &BTreeMap<String, Permission>) -> Vec<Rule> {
    let mut normalized = BTreeMap::new();
    for (action, effect) in map {
        normalized
            .entry(legacy_key(action))
            .and_modify(|old| *old = strictest(*old, *effect))
            .or_insert(*effect);
    }
    normalized
        .into_iter()
        .map(|(action, effect)| Rule {
            action: action.into(),
            resource: "*".into(),
            effect,
        })
        .collect()
}

fn evaluate_layer(rules: &[Rule], actions: &[&str], resource: &str) -> Option<Permission> {
    evaluate_layer_resources(rules, actions, &[resource])
}

fn evaluate_layer_resources(
    rules: &[Rule],
    actions: &[&str],
    resources: &[&str],
) -> Option<Permission> {
    let mut effect = None;
    for rule in rules {
        if actions
            .iter()
            .any(|action| wildcard(action, legacy_key(&rule.action)))
        {
            effect.get_or_insert(Permission::Ask);
            if resources
                .iter()
                .any(|resource| wildcard(resource, &rule.resource))
            {
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
    wildcard_preserving_identity(&input, &pattern)
}

/// Saved owner patterns operate on actual Unix resource identities, not config
/// spelling normalization. Literal backslashes must never become separators.
pub(crate) fn wildcard_preserving_identity(input: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(" *")
        && wildcard_preserving_identity(input, prefix)
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
