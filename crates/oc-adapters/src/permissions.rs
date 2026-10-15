//! Ordered action/resource rules, intersected at authority boundaries.
//!
//! Ordinary V2 documents and agent overrides form one ordered configuration.
//! Native/programmatic authorities and delegated ceilings remain intersected.

use crate::config::{ConfigError, Permission, legacy_key};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configured: Option<ConfiguredRules>,
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

/// Keep source phases distinct so a child selects its own agent before the
/// captured parent policy is applied as an independent ceiling.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfiguredRules {
    // None is an unselected source/profile, not a captured actor authority.
    defaults: Option<Vec<Rule>>,
    documents: Vec<Rule>,
    agent: Vec<Rule>,
}

impl ConfiguredRules {
    fn iter(&self) -> impl Iterator<Item = &Rule> + Clone {
        self.defaults
            .iter()
            .flatten()
            .chain(&self.documents)
            .chain(&self.agent)
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut Rule> {
        self.defaults
            .iter_mut()
            .flatten()
            .chain(&mut self.documents)
            .chain(&mut self.agent)
    }
}

fn rule(action: &str, resource: &str, effect: Permission) -> Rule {
    Rule {
        action: action.into(),
        resource: resource.into(),
        effect,
    }
}

fn strictest(a: Permission, b: Permission) -> Permission {
    match (a, b) {
        (Permission::Deny, _) | (_, Permission::Deny) => Permission::Deny,
        (Permission::Ask, _) | (_, Permission::Ask) => Permission::Ask,
        _ => Permission::Allow,
    }
}

impl PermissionRules {
    /// Ordinary evaluation has no implicit grant; selecting an agent supplies
    /// its pinned defaults before documents and agent-specific overrides.
    pub(crate) fn configuration() -> Self {
        Self {
            configured: Some(ConfiguredRules::default()),
            ..Default::default()
        }
    }

    pub(crate) fn from_document(value: &serde_json::Value) -> Result<Self, ConfigError> {
        let mut configured = Self::configuration();
        configured.append_configuration(Self::from_config(value)?);
        Ok(configured)
    }

    /// Config source ordering is not an authority boundary. Do not use extend
    /// here: that API deliberately intersects independently captured policies.
    pub(crate) fn append_configuration(&mut self, mut other: Self) {
        let configured = self.configured.get_or_insert_default();
        if let Some(source) = other.configured.take() {
            configured.documents.extend(source.documents);
            configured.documents.extend(source.agent);
        }
        configured
            .documents
            .extend(other.authorities.drain(..).flatten());
        self.extend(other);
    }

    pub(crate) fn configure_agent(
        &mut self,
        base: &BTreeMap<String, Permission>,
        agent: &BTreeMap<String, Permission>,
        rules: &Self,
    ) {
        let Some(configured) = &mut self.configured else {
            self.narrow(base, agent, rules);
            return;
        };
        // schema/agent.ts::Info.default at the configuration oracle revision.
        configured.defaults = Some(vec![
            rule("*", "*", Permission::Allow),
            rule("read", "*.env", Permission::Ask),
            rule("read", "*.env.*", Permission::Ask),
            rule("read", "*.env.example", Permission::Allow),
        ]);
        configured.agent.clear();
        if let Some(source) = &rules.configured {
            configured.agent.extend(source.documents.iter().cloned());
            configured.agent.extend(source.agent.iter().cloned());
        }
        // Scalar SDK policies are not canonical document overrides.
        if !base.is_empty() {
            self.constraints.push(scalar_rules(base));
        }
        if !agent.is_empty() {
            self.constraints.push(scalar_rules(agent));
        }
        self.constraints.extend(rules.authorities.clone());
        self.constraints.extend(rules.constraints.clone());
        self.builtin_defaults.extend(rules.builtin_defaults.clone());
        self.builtin_grants.extend(rules.builtin_grants.clone());
        self.plan_ceilings.extend(rules.plan_ceilings.clone());
    }

    /// Child ordinary overrides are based on the same documents, not on its
    /// parent's agent overrides. The complete parent chain remains a ceiling.
    pub(crate) fn configure_child(
        &mut self,
        base: &BTreeMap<String, Permission>,
        agent: &BTreeMap<String, Permission>,
        rules: &Self,
    ) {
        if let Some(parent) = &self.configured
            && parent.defaults.is_some()
        {
            self.constraints.push(parent.iter().cloned().collect());
        }
        self.configure_agent(base, agent, rules);
    }
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
            .chain(
                self.configured
                    .iter_mut()
                    .flat_map(|configured| [&mut configured.documents, &mut configured.agent]),
            )
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
    /// Parse canonical ordered `permissions` rules without dropping order or
    /// resources. OpenCode 1 `permission`/`tools` shapes are refused by T62.
    pub fn from_config(value: &serde_json::Value) -> Result<Self, ConfigError> {
        let mut result = Self::default();
        // Never silently drop an OpenCode 1 rule source: that would widen authority.
        for legacy in ["permission", "tools"] {
            if value.get(legacy).is_some() {
                return Err(invalid(legacy));
            }
        }
        if let Some(raw) = value.get("permissions") {
            if !raw.is_array() {
                return Err(invalid("permissions"));
            }
            let rules = parse(raw, "permissions")?;
            if !rules.is_empty() {
                result.authorities.push(rules);
            }
        }
        Ok(result)
    }

    /// Independent deny ceiling for built-in profiles; never a grant.
    pub(crate) fn deny_actions(actions: &[&str]) -> Self {
        Self {
            constraints: vec![
                actions
                    .iter()
                    .map(|action| Rule {
                        action: legacy_key(action).into(),
                        resource: "*".into(),
                        effect: Permission::Deny,
                    })
                    .collect(),
            ],
            ..Self::default()
        }
    }

    /// Intersect an independent policy, never ordinary configuration documents.
    pub fn extend(&mut self, mut other: Self) {
        if let Some(configured) = other.configured.take() {
            self.authorities.push(configured.iter().cloned().collect());
        }
        self.authorities.extend(other.authorities);
        self.constraints.extend(other.constraints);
        self.builtin_defaults.extend(other.builtin_defaults);
        self.builtin_grants.extend(other.builtin_grants);
        self.plan_ceilings.extend(other.plan_ceilings);
    }

    pub(super) fn plan_defaults() -> Self {
        let mut rules = Self::from_config(&serde_json::json!({"permissions":[
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
        let mut rules = Self::from_config(&serde_json::json!({"permissions":[
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
            .chain(
                self.configured
                    .iter_mut()
                    .flat_map(ConfiguredRules::iter_mut),
            )
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
        if self.authorities.is_empty() && self.configured.is_none() {
            self.authorities.push(scalar_rules(base));
        }
        if let Some(configured) = &rules.configured {
            self.constraints.push(configured.iter().cloned().collect());
        }
        if rules.authorities.is_empty() && rules.configured.is_none() {
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
        let mut effect = self.configured.as_ref().map(|configured| {
            evaluate_layer_resources(configured.iter(), actions, &resources)
                .unwrap_or(Permission::Ask)
        });
        if self.authorities.is_empty() && self.configured.is_none() {
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
        if self.configured.as_ref().is_some_and(|configured| {
            evaluate_layer(configured.iter(), &actions, resource) == Some(Permission::Deny)
        }) {
            return true;
        }
        let scalar = scalar_rules(fallback);
        let authorities = if self.authorities.is_empty() && self.configured.is_none() {
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
        let authorities = if self.authorities.is_empty() && self.configured.is_none() {
            std::slice::from_ref(&scalar)
        } else {
            &self.authorities
        };
        let mut effect = self.configured.as_ref().map(|configured| {
            evaluate_layer_resources(configured.iter(), &["read"], &[resolved, location])
                .unwrap_or(Permission::Ask)
        });
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
        let authorities = if self.authorities.is_empty() && self.configured.is_none() {
            std::slice::from_ref(&scalar)
        } else {
            &self.authorities
        };
        if self
            .configured
            .as_ref()
            .is_some_and(|configured| layer_denies_all(configured.iter(), &actions))
        {
            return false;
        }
        let admitted = self.configured.is_some()
            || registered_mcp
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
            if layer_denies_all(layer.iter(), &actions) {
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

fn layer_denies_all<'a>(rules: impl Iterator<Item = &'a Rule> + Clone, actions: &[&str]) -> bool {
    evaluate_layer(rules.clone(), actions, "*") == Some(Permission::Deny)
        && rules
            .clone()
            .filter(|rule| {
                actions
                    .iter()
                    .any(|action| wildcard(action, legacy_key(&rule.action)))
            })
            .all(|rule| {
                evaluate_layer(rules.clone(), actions, &rule.resource) == Some(Permission::Deny)
            })
}

fn evaluate_layer<'a>(
    rules: impl IntoIterator<Item = &'a Rule>,
    actions: &[&str],
    resource: &str,
) -> Option<Permission> {
    evaluate_layer_resources(rules, actions, &[resource])
}

fn evaluate_layer_resources<'a>(
    rules: impl IntoIterator<Item = &'a Rule>,
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
    value
        .as_array()
        .ok_or_else(|| invalid(field))?
        .iter()
        .map(|raw| {
            let mut rule: Rule = serde_json::from_value(raw.clone()).map_err(|_| invalid(field))?;
            rule.action = legacy_key(&rule.action).into();
            Ok(rule)
        })
        .collect()
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
