//! Active wire history, DCP planning/projection and compression.

use super::*;

fn protected_message(value: &serde_json::Value) -> Result<Message, RuntimeError> {
    Ok(Message {
        id: MessageId::new(value["id"].as_str().ok_or(RuntimeError::Storage)?)
            .ok_or(RuntimeError::Storage)?,
        role: match value["role"].as_str() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => return Err(RuntimeError::Storage),
        },
        text: value["text"].as_str().ok_or(RuntimeError::Storage)?.into(),
    })
}

fn map_messages(history: &[(String, String, String)]) -> Result<Vec<Message>, RuntimeError> {
    let mut out = Vec::with_capacity(history.len());
    for (id, role, text) in history {
        let role = match role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => {
                return Err(RuntimeError::Compress(format!("unknown role for {id}")));
            }
        };
        let Some(id) = MessageId::new(id.clone()) else {
            return Err(RuntimeError::Compress("empty message id".to_string()));
        };
        out.push(Message {
            id,
            role,
            text: text.clone(),
        });
    }
    Ok(out)
}

pub(super) fn dcp_config_input(
    projected: &[(String, String, String)],
    config: &DcpConfig,
) -> Option<InputItem> {
    if !config.enabled || config.manual_mode {
        return None;
    }
    let anchors = projected
        .iter()
        .enumerate()
        .filter(|(_, (id, _, _))| !id.starts_with("session-checkpoint"))
        .map(|(index, (id, role, _))| {
            serde_json::json!({
                "id": id,
                "role": role,
                "closed": index + 1 < projected.len(),
            })
        })
        .collect::<Vec<_>>();
    Some(InputItem::message(
        InputRole::Developer,
        format!(
            "DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: {}",
            serde_json::Value::Array(anchors)
        ),
    ))
}

fn plan_dcp_strategies(
    input: &[InputItem],
    config: &DcpConfig,
    existing: &crate::storage::DcpToolProjection,
) -> crate::storage::DcpToolProjection {
    let mut projection = crate::storage::DcpToolProjection::default();
    if !config.enabled || (config.manual_mode && !config.automatic_strategies) {
        return projection;
    }
    let total_users = input
        .iter()
        .filter(|item| {
            matches!(
                item,
                InputItem::Message {
                    role: InputRole::User,
                    ..
                }
            )
        })
        .count() as u64;
    let mut outputs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in input {
        if let Some((call_id, output)) = item.call_output() {
            outputs
                .entry(call_id.to_owned())
                .or_default()
                .push(output.to_owned());
        }
    }
    let mut users_seen = 0u64;
    let mut calls = Vec::new();
    let mut occurrences: BTreeMap<String, u64> = BTreeMap::new();
    for (index, item) in input.iter().enumerate() {
        if matches!(
            item,
            InputItem::Message {
                role: InputRole::User,
                ..
            }
        ) {
            users_seen += 1;
        }
        let InputItem::ProviderOutput(value) = item else {
            continue;
        };
        if value["type"] != "function_call" {
            continue;
        }
        let Some(call_id) = value["call_id"].as_str().map(str::to_owned) else {
            continue;
        };
        let occurrence = occurrences.entry(call_id.clone()).or_default();
        let call_key = (call_id.clone(), *occurrence);
        *occurrence += 1;
        if existing.hidden.contains(&call_key) {
            continue;
        }
        let name = value["name"].as_str().unwrap_or_default().to_owned();
        let arguments = value["arguments"].as_str().unwrap_or_default().to_owned();
        let file_protected = dcp_call_has_protected_path(&name, &arguments, config);
        let generally_protected =
            crate::dcp_auto::tool_is_protected(&config.protected_tools, &name);
        let dedup_protected = file_protected
            || generally_protected
            || crate::dcp_auto::tool_is_protected(&config.dedup_protected_tools, &name);
        let purge_protected = file_protected
            || generally_protected
            || crate::dcp_auto::tool_is_protected(&config.purge_protected_tools, &name);
        let age = total_users.saturating_sub(users_seen);
        let turn_protected = config.turn_protection && age <= config.turn_protection_turns;
        let dedup_protected = dedup_protected || turn_protected;
        let purge_protected = purge_protected || turn_protected;
        if config.purge_errors
            && !purge_protected
            && arguments.len() > crate::dcp_auto::LARGE_INPUT_BYTES
            && age >= config.purge_after_turns
            && outputs
                .get(&call_id)
                .and_then(|values| values.get(call_key.1 as usize))
                .is_some_and(|output| output.starts_with("error:"))
        {
            projection.purged.insert(call_key.clone());
        }
        calls.push((
            index,
            call_key,
            name,
            canonical_json(&arguments),
            dedup_protected,
        ));
    }
    if !config.deduplication {
        return projection;
    }
    let mut last = BTreeMap::new();
    for (index, _, name, arguments, _) in &calls {
        last.insert((name.clone(), arguments.clone()), *index);
    }
    projection.hidden = calls
        .into_iter()
        .filter(|(index, _, name, arguments, protected)| {
            !protected && last.get(&(name.clone(), arguments.clone())) != Some(index)
        })
        .map(|(_, call_key, _, _, _)| call_key)
        .collect();
    projection
}

fn apply_turn_protection(
    history: &[(String, String, String)],
    config: &DcpConfig,
    spec: &mut ProtectedSpec,
) {
    if !config.turn_protection || config.turn_protection_turns == 0 {
        return;
    }
    let closed_end = if history.last().is_some_and(|(_, role, _)| role == "user") {
        history.len().saturating_sub(1)
    } else {
        history.len()
    };
    let Some(start) = history[..closed_end]
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, (_, role, _))| role == "user")
        .nth(usize::try_from(config.turn_protection_turns.saturating_sub(1)).unwrap_or(usize::MAX))
        .map(|(index, _)| index)
        .or_else(|| {
            history[..closed_end]
                .iter()
                .position(|(_, role, _)| role == "user")
        })
    else {
        return;
    };
    spec.protected_message_ids.extend(
        history[start..closed_end]
            .iter()
            .map(|(id, _, _)| id.clone()),
    );
}

/// Projection is local to one assembled request; the durable journal stays raw.
pub(super) fn dcp_continuation(
    history: &[InputItem],
    current: &[InputItem],
    projection: &crate::storage::DcpToolProjection,
) -> Vec<InputItem> {
    let mut input = history.iter().chain(current).cloned().collect();
    apply_dcp_projection(&mut input, projection);
    input
}

/// The existing durable journal establishes immutable call occurrence identity.
/// Local occurrence counts include omitted pending calls, so window cuts cannot
/// renumber coverage. Legacy compression outcomes are never backfilled.
pub(crate) fn dcp_call_identities(
    logs: &[String],
    current: Option<&TurnLog>,
) -> Result<BTreeMap<crate::storage::DcpCallKey, crate::storage::DcpCallIdentity>, RuntimeError> {
    let mut result = BTreeMap::new();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut add = |log: &TurnLog, include_pending: bool| {
        let calls = log
            .input
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                InputItem::ProviderOutput(v) if v["type"] == "function_call" => {
                    v["call_id"].as_str().map(|id| (index, id))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let answered = log
            .input
            .iter()
            .filter_map(|item| item.call_output().map(|(id, _)| id))
            .collect::<std::collections::BTreeSet<_>>();
        let mut local = BTreeMap::<String, u64>::new();
        for (index, id) in calls {
            let occurrence = local.entry(id.into()).or_default();
            let identity = (
                log.turn_id.clone(),
                id.into(),
                log.call_occurrences
                    .get(&index)
                    .copied()
                    .unwrap_or(*occurrence),
            );
            *occurrence += 1;
            if !include_pending && !answered.contains(id) {
                continue;
            }
            let n = counts.entry(id.into()).or_default();
            result.insert((id.into(), *n), identity);
            *n += 1;
        }
    };
    for raw in logs {
        let value = serde_json::from_str(raw).map_err(|_| RuntimeError::Storage)?;
        let log = TurnLog::from_json(&value).map_err(|_| RuntimeError::Storage)?;
        if let Some(working) = &log.working {
            add(
                &TurnLog::from_json(working).map_err(|_| RuntimeError::Storage)?,
                false,
            );
        }
        add(&log, false);
    }
    if let Some(log) = current {
        if let Some(working) = &log.working {
            add(
                &TurnLog::from_json(working).map_err(|_| RuntimeError::Storage)?,
                false,
            );
        }
        add(log, true);
    }
    Ok(result)
}

pub(crate) fn apply_dcp_projection(
    input: &mut Vec<InputItem>,
    projection: &crate::storage::DcpToolProjection,
) {
    let mut calls: BTreeMap<String, u64> = BTreeMap::new();
    let mut outputs: BTreeMap<String, u64> = BTreeMap::new();
    input.retain_mut(|item| match item {
        InputItem::ProviderOutput(value) if value["type"] == "function_call" => {
            let Some(call_id) = value["call_id"].as_str().map(str::to_owned) else {
                return true;
            };
            let occurrence = calls.entry(call_id.clone()).or_default();
            let key = (call_id, *occurrence);
            *occurrence += 1;
            if projection.purged.contains(&key) {
                value["arguments"] = serde_json::Value::String(
                    serde_json::json!({"purged": "large error input"}).to_string(),
                );
            }
            !projection.hidden.contains(&key)
        }
        InputItem::FunctionCallOutput { call_id, .. }
        | InputItem::McpFunctionCallOutput { call_id, .. } => {
            let occurrence = outputs.entry(call_id.clone()).or_default();
            let key = (call_id.clone(), *occurrence);
            *occurrence += 1;
            !projection.hidden.contains(&key)
        }
        InputItem::ReadFunctionCallOutput { call_id, .. } => {
            let occurrence = outputs.entry(call_id.clone()).or_default();
            let key = (call_id.clone(), *occurrence);
            *occurrence += 1;
            !projection.hidden.contains(&key)
        }
        _ => true,
    });
}

/// Text, function arguments and outputs are independently estimated. Provider
/// envelopes/opaque continuation/image URLs are not mislabeled as text tokens.
pub(crate) fn dcp_contents(input: &[InputItem]) -> Vec<&str> {
    let mut content = Vec::new();
    for item in input {
        match item {
            InputItem::Message { content: parts, .. } => {
                for part in parts {
                    match part {
                        crate::provider::InputContent::InputText { text }
                        | crate::provider::InputContent::OutputText { text } => {
                            content.push(text.as_str())
                        }
                        crate::provider::InputContent::InputImage { .. } => {}
                    }
                }
            }
            InputItem::FunctionCallOutput { output, .. } => content.push(output.as_str()),
            InputItem::McpFunctionCallOutput { output, .. } => {
                content.extend(output.texts().iter().map(String::as_str))
            }
            InputItem::ReadFunctionCallOutput { output, .. } => content.push(output.display()),
            InputItem::ProviderOutput(value) => {
                if value["type"] == "function_call" {
                    if let Some(arguments) = value["arguments"].as_str() {
                        content.push(arguments);
                    }
                } else if value["type"] == "message" {
                    if let Some(parts) = value["content"].as_array() {
                        for part in parts {
                            if let Some(text) = part["text"].as_str() {
                                content.push(text);
                            }
                        }
                    }
                } else if value["type"] == "reasoning"
                    && let Some(parts) = value["summary"].as_array()
                {
                    for part in parts {
                        if let Some(text) = part["text"].as_str() {
                            content.push(text);
                        }
                    }
                }
            }
        }
    }
    content
}

fn dcp_wire_estimates(before: &[InputItem], after: &[InputItem]) -> (u64, u64) {
    let estimate = oc_core::dcp_view::estimate_content;
    let before = dcp_contents(before);
    let after = dcp_contents(after);
    let before_total: u64 = before.iter().map(|text| estimate(text)).sum();
    let after_total: u64 = after.iter().map(|text| estimate(text)).sum();
    let mut remaining = BTreeMap::<&str, usize>::new();
    for text in after {
        *remaining.entry(text).or_default() += 1;
    }
    let mut removed = 0u64;
    for text in before {
        if let Some(count) = remaining.get_mut(text).filter(|n| **n > 0) {
            *count -= 1;
        } else {
            removed = removed.saturating_add(estimate(text));
        }
    }
    (removed, before_total.saturating_sub(after_total))
}

pub(crate) fn dcp_call_contents(
    input: &[InputItem],
    projection: &crate::storage::DcpToolProjection,
) -> BTreeMap<crate::storage::DcpCallKey, (String, String)> {
    let mut calls = BTreeMap::<String, u64>::new();
    let mut outputs = BTreeMap::<String, u64>::new();
    let mut values = BTreeMap::<crate::storage::DcpCallKey, (String, String)>::new();
    for item in input {
        match item {
            InputItem::ProviderOutput(value) if value["type"] == "function_call" => {
                let Some(id) = value["call_id"].as_str() else {
                    continue;
                };
                let occurrence = calls.entry(id.into()).or_default();
                let key = (id.to_owned(), *occurrence);
                *occurrence += 1;
                if !projection.hidden.contains(&key) {
                    let arguments = if projection.purged.contains(&key) {
                        serde_json::json!({"purged":"large error input"}).to_string()
                    } else {
                        value["arguments"].as_str().unwrap_or_default().into()
                    };
                    values.entry(key).or_default().0 = arguments;
                }
            }
            InputItem::FunctionCallOutput { call_id, output } => {
                let occurrence = outputs.entry(call_id.clone()).or_default();
                let key = (call_id.clone(), *occurrence);
                *occurrence += 1;
                if !projection.hidden.contains(&key) {
                    values.entry(key).or_default().1 = output.clone();
                }
            }
            InputItem::McpFunctionCallOutput { call_id, output } => {
                let occurrence = outputs.entry(call_id.clone()).or_default();
                let key = (call_id.clone(), *occurrence);
                *occurrence += 1;
                if !projection.hidden.contains(&key) {
                    values.entry(key).or_default().1 = output.texts().join("\n");
                }
            }
            InputItem::ReadFunctionCallOutput { call_id, output } => {
                let occurrence = outputs.entry(call_id.clone()).or_default();
                let key = (call_id.clone(), *occurrence);
                *occurrence += 1;
                if !projection.hidden.contains(&key) {
                    values.entry(key).or_default().1 = output.display().into();
                }
            }
            _ => {}
        }
    }
    values
}

fn canonical_json(raw: &str) -> String {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => serde_json::Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, sort(value)))
                    .collect::<std::collections::BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sort).collect())
            }
            value => value,
        }
    }
    serde_json::from_str(raw)
        .map(sort)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| raw.to_string())
}

pub(super) fn dcp_call_has_protected_path(name: &str, arguments: &str, config: &DcpConfig) -> bool {
    fn contains_path(value: &serde_json::Value, patterns: &[String]) -> bool {
        match value {
            serde_json::Value::String(value) => crate::dcp::path_is_protected(patterns, value),
            serde_json::Value::Array(values) => {
                values.iter().any(|value| contains_path(value, patterns))
            }
            serde_json::Value::Object(values) => {
                values.values().any(|value| contains_path(value, patterns))
            }
            _ => false,
        }
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return matches!(name, "apply_patch" | "edit" | "write");
    };
    if name == "apply_patch"
        && let Some(patch) = value.get("patchText").and_then(|value| value.as_str())
    {
        return crate::patch::affected_paths(patch).map_or(true, |paths| {
            paths
                .iter()
                .any(|path| crate::dcp::path_is_protected(&config.protected_file_patterns, path))
        });
    }
    if matches!(name, "edit" | "write") {
        return value
            .get("path")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|path| {
                crate::dcp::path_is_protected(&config.protected_file_patterns, path)
            });
    }
    contains_path(&value, &config.protected_file_patterns)
}

pub(super) fn active_summary_tokens(projected: &[(String, String, String)]) -> u64 {
    projected
        .iter()
        .filter(|(id, _, _)| id.starts_with('b'))
        .map(|(_, _, text)| estimate_tokens(text))
        .sum()
}

impl<'a> Runtime<'a> {
    pub(super) fn renew_compaction_selection(
        &self,
        session: &str,
        context: &mut ActiveContext,
    ) -> Result<(), RuntimeError> {
        let spec = self.history_protection_spec(session)?;
        let config = self.dcp_config.read().expect("dcp config").clone();
        for block in &mut context.blocks {
            let legacy = crate::dcp::select_legacy_protection(
                &crate::dcp::legacy_protection(block).map_err(|_| RuntimeError::Storage)?,
                &spec,
            )
            .map_err(|_| RuntimeError::Storage)?;
            if let Some(hot) = &mut block.hot {
                let logs = hot["logs"]
                    .as_array()
                    .ok_or(RuntimeError::Storage)?
                    .iter()
                    .map(serde_json::Value::to_string)
                    .collect::<Vec<_>>();
                hot["logs"] = self.select_history_facts(&logs, &config)?.into();
                let facts = hot["protected"].as_array().ok_or(RuntimeError::Storage)?;
                let mut selected = Vec::new();
                let mut summary = crate::dcp::authored_summary(&block.summary).to_string();
                for fact in facts {
                    let message = protected_message(fact)?;
                    if oc_core::context_plan::message_protected(&spec, &message) {
                        summary.push_str(if message.role == oc_core::session::Role::User {
                            crate::dcp::PROTECTED_USER_HEADING
                        } else {
                            crate::dcp::PROTECTED_CONTENT_HEADING
                        });
                        summary.push_str(&message.text);
                        selected.push(fact.clone());
                    }
                }
                hot["protected"] = selected.into();
                crate::dcp::append_legacy_protection(&mut summary, &legacy)
                    .map_err(|_| RuntimeError::Storage)?;
                hot["legacy_protected"] = legacy.into();
                block.summary = summary;
            }
        }
        let by_id = context
            .blocks
            .iter()
            .cloned()
            .map(|b| (b.id.clone(), b))
            .collect();
        for row in &mut context.projected {
            if context.blocks.iter().any(|b| b.id == row.0) {
                row.2 = crate::dcp::expand_block(&by_id, &row.0, 0, &mut Vec::new())
                    .map_err(|_| RuntimeError::Storage)?;
            }
        }
        Ok(())
    }
    pub(super) fn history_protection_spec(
        &self,
        session: &str,
    ) -> Result<ProtectedSpec, RuntimeError> {
        let config = self.dcp_config.read().expect("dcp config");
        Ok(ProtectedSpec {
            protect_user_messages: config.protect_user_messages,
            protect_tags: config.protect_tags,
            file_globs: config.protected_file_patterns.clone(),
            protected_message_ids: if config.turn_protection {
                self.db
                    .recent_completed_user_ids(session, config.turn_protection_turns)?
                    .into_iter()
                    .collect()
            } else {
                Default::default()
            },
        })
    }
    pub(super) fn selected_message_facts(
        &self,
        session: &str,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
    ) -> Result<Vec<serde_json::Value>, RuntimeError> {
        let spec = self.history_protection_spec(session)?;
        let mut legacy = Vec::new();
        for block in blocks {
            legacy.extend(
                crate::dcp::select_legacy_protection(
                    &crate::dcp::legacy_protection(block).map_err(|_| RuntimeError::Storage)?,
                    &spec,
                )
                .map_err(|_| RuntimeError::Storage)?,
            );
        }
        let mut candidates = projected
            .iter()
            .filter(|(_, role, _)| role == "user" || role == "assistant")
            .map(|(id, role, text)| serde_json::json!({"id":id,"role":role,"text":text}))
            .collect::<Vec<_>>();
        for block in blocks {
            if let Some(facts) = block.hot.as_ref().and_then(|h| h["protected"].as_array()) {
                candidates.extend(facts.iter().cloned());
            }
        }
        if projected.iter().any(|r| r.0 == "session-checkpoint")
            && let Some(selection) = self.db.checkpoint_selection(session)?
        {
            for value in selection {
                if let Some(fact) = value.get("protected_message") {
                    candidates.push(fact.clone());
                }
                if let Some(facts) = value.get("legacy_protected") {
                    legacy.extend(
                        crate::dcp::select_legacy_protection(
                            facts.as_array().ok_or(RuntimeError::Storage)?,
                            &spec,
                        )
                        .map_err(|_| RuntimeError::Storage)?,
                    );
                }
            }
        }
        let mut selected = Vec::new();
        let mut bytes = 0usize;
        for fact in legacy {
            let value = serde_json::json!({"legacy_protected":[fact]});
            if !selected.contains(&value) {
                bytes =
                    bytes.saturating_add(fact["text"].as_str().ok_or(RuntimeError::Storage)?.len());
                selected.push(value);
            }
        }
        if bytes > ACTIVE_CONTEXT_BYTES_CAP {
            return Err(RuntimeError::ContextOverflow {
                bytes: bytes as u64,
                cap: ACTIVE_CONTEXT_BYTES_CAP,
            });
        }
        for fact in candidates {
            let message = protected_message(&fact)?;
            if oc_core::context_plan::message_protected(&spec, &message)
                && !selected
                    .iter()
                    .any(|v: &serde_json::Value| v["protected_message"]["id"] == fact["id"])
            {
                bytes = bytes.saturating_add(message.text.len());
                if bytes > ACTIVE_CONTEXT_BYTES_CAP {
                    return Err(RuntimeError::ContextOverflow {
                        bytes: bytes as u64,
                        cap: ACTIVE_CONTEXT_BYTES_CAP,
                    });
                }
                selected.push(serde_json::json!({"protected_message":fact}));
            }
        }
        Ok(selected)
    }
    pub(super) fn select_history_facts(
        &self,
        logs: &[String],
        config: &DcpConfig,
    ) -> Result<Vec<serde_json::Value>, RuntimeError> {
        self.select_history_facts_with_producers(logs, config, false)
    }
    fn select_history_facts_with_producers(
        &self,
        logs: &[String],
        config: &DcpConfig,
        fresh_producers: bool,
    ) -> Result<Vec<serde_json::Value>, RuntimeError> {
        let mut result = Vec::new();
        let mut bytes = 0usize;
        for raw in logs {
            let value: serde_json::Value =
                serde_json::from_str(raw).map_err(|_| RuntimeError::Storage)?;
            let fresh = fresh_producers && value["_dcp_block"].is_null();
            let log = TurnLog::from_json(&value).map_err(|_| RuntimeError::Storage)?;
            let live = self.db.live_shell_call_ids(&log.turn_id)?;
            let selected = log
                .selected_closed_groups(|group| {
                    let projection_only = group.iter().any(|item| matches!(item, InputItem::ProviderOutput(v) if v["type"] == "function_call" && v["name"] == "compress"))
                        && !group.iter().any(|item| matches!(item, InputItem::ProviderOutput(v) if v["type"] == "function_call" && v["name"] != "compress"));
                    (fresh
                        && !projection_only
                        && group
                            .iter()
                            .any(|item| matches!(item, InputItem::ProviderOutput(v) if v["type"] != "message")))
                        || group.iter().any(|item| match item {
                            InputItem::ProviderOutput(v) if v["type"] == "function_call" => {
                                let name = v["name"].as_str().unwrap_or_default();
                                live.iter()
                                    .any(|id| v["call_id"].as_str() == Some(id.as_str()))
                                    || crate::dcp_auto::tool_is_protected(
                                        &config.protected_tools,
                                        name,
                                    )
                                    || crate::dcp_auto::tool_is_protected(
                                        &config.dedup_protected_tools,
                                        name,
                                    )
                                    || crate::dcp_auto::tool_is_protected(
                                        &config.purge_protected_tools,
                                        name,
                                    )
                                    || dcp_call_has_protected_path(
                                        name,
                                        v["arguments"].as_str().unwrap_or_default(),
                                        config,
                                    )
                            }
                            _ => {
                                config.protect_tags
                                    && oc_core::context_plan::extract_protect_tags(
                                        &serde_json::to_string(item).unwrap_or_default(),
                                    )
                                    .next()
                                    .is_some()
                            }
                        })
                })
                .map_err(|_| RuntimeError::Storage)?;
            if !selected.input.is_empty() {
                let value = selected.to_json();
                bytes = bytes.saturating_add(value.to_string().len());
                if bytes > ACTIVE_CONTEXT_BYTES_CAP {
                    return Err(RuntimeError::ContextOverflow {
                        bytes: bytes as u64,
                        cap: ACTIVE_CONTEXT_BYTES_CAP,
                    });
                }
                result.push(value);
            }
        }
        Ok(result)
    }
    pub(super) fn projected_wire_logs(
        &self,
        session: &str,
        after: i64,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
    ) -> Result<Vec<String>, RuntimeError> {
        let mut logs = self
            .db
            .presentation_wire_logs(session, after, projected, blocks)?
            .ok_or(RuntimeError::Storage)?;
        for block in blocks
            .iter()
            .filter(|b| projected.iter().any(|r| r.0 == b.id))
        {
            if let Some(hot) = &block.hot {
                for value in hot["logs"].as_array().ok_or(RuntimeError::Storage)? {
                    let mut value = value.clone();
                    value["_dcp_block"] = block.id.clone().into();
                    logs.push(value.to_string());
                }
            }
        }
        if projected.iter().any(|r| r.0 == "session-checkpoint")
            && let Some(selection) = self.db.checkpoint_selection(session)?
        {
            for mut value in selection {
                if value.get("protected_message").is_some()
                    || value.get("legacy_protected").is_some()
                {
                    continue;
                }
                value["_dcp_block"] = "session-checkpoint".into();
                logs.push(value.to_string());
            }
        }
        let mut keyed = Vec::new();
        let mut bytes = 0usize;
        for raw in logs {
            bytes = bytes.saturating_add(raw.len());
            if bytes > ACTIVE_CONTEXT_BYTES_CAP {
                return Err(RuntimeError::ContextOverflow {
                    bytes: bytes as u64,
                    cap: ACTIVE_CONTEXT_BYTES_CAP,
                });
            }
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?;
            let key = value["_dcp_block"]
                .as_str()
                .or(value["user_message"].as_str())
                .ok_or(RuntimeError::Storage)?;
            let position = projected
                .iter()
                .position(|r| r.0 == key)
                .ok_or(RuntimeError::Storage)?;
            let seq = if let Some(anchor) = value["user_message"].as_str() {
                self.db.message_seq(session, anchor)?.unwrap_or(0)
            } else {
                0
            };
            keyed.push((position, seq, raw));
        }
        keyed.sort_by_key(|(position, seq, _)| (*position, *seq));
        Ok(keyed.into_iter().map(|(_, _, raw)| raw).collect())
    }
    /// Execute a manual compress over validated ranges (same permission path).
    pub fn run_compress(
        &self,
        session: &str,
        args: &serde_json::Value,
        spec: &ProtectedSpec,
    ) -> Result<CompressReport, RuntimeError> {
        let _lease = self.begin_active()?;
        self.run_compress_inner(session, args, spec)
    }

    fn run_compress_inner(
        &self,
        session: &str,
        args: &serde_json::Value,
        spec: &ProtectedSpec,
    ) -> Result<CompressReport, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.primary_lane(&published);
        {
            let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules);
            if policy.check(COMPRESS_TOOL).is_err() {
                return Err(denied(COMPRESS_TOOL));
            }
        }
        self.open_session(session)?;
        let (_topic, ranges) = crate::dcp::validate_range_args(args)
            .map_err(|e| RuntimeError::Compress(e.to_string()))?;
        let op = format!(
            "compress-{session}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        self.db
            .record_tool_intent(&op, session, None, COMPRESS_TOOL, &args.to_string())?;
        let config = self.dcp_config.read().expect("dcp lock").clone();
        let prepared = (|| {
            let mut plan = self.prepare_dcp_plan(session, &ranges, spec, None)?;
            let (delta, projection) =
                self.measure_dcp_plan(session, &mut plan, None, &lane, &config)?;
            Ok::<_, RuntimeError>((plan, delta, projection))
        })();
        let (plan, delta, _projection) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.db
                    .record_tool_outcome(&op, "failed", Some(&error.to_string()))?;
                return Err(error);
            }
        };
        if plan.after_bytes >= plan.before_bytes {
            let error = crate::dcp::DcpError::NoGain {
                before_bytes: plan.before_bytes,
                after_bytes: plan.after_bytes,
            };
            let output=serde_json::json!({"status":"no_gain","beforeBytes":plan.before_bytes,"afterBytes":plan.after_bytes}).to_string();
            self.db.record_tool_outcome(&op, "no_gain", Some(&output))?;
            return Err(RuntimeError::Compress(error.to_string()));
        }
        let blocks = plan
            .blocks
            .iter()
            .map(|block| block.id.clone())
            .collect::<Vec<_>>();
        let output = serde_json::json!({
            "status": "compressed",
            "blocks": blocks,
            "savedTokens": plan.saved_tokens,
            "beforeBytes": plan.before_bytes,
            "afterBytes": plan.after_bytes,
        })
        .to_string();
        let mut next_states = self.nudge_state.lock().expect("nudge lock").clone();
        for (key, state) in &mut next_states {
            if key.starts_with(&format!("dcp.nudge.{session}\0")) {
                state.on_compress_success();
            }
        }
        let preference_updates = next_states
            .iter()
            .filter(|(key, _)| key.starts_with(&format!("dcp.nudge.{session}\0")))
            .map(|(key, state)| {
                serde_json::to_string(state)
                    .map(|value| (key.clone(), value))
                    .map_err(|_| RuntimeError::Storage)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let metadata = crate::dcp::CompressionCommitMetadata {
            operation_id: &op,
            operation_state: "completed",
            operation_output: &output,
            turn_id: None,
            turn_log: None,
            preference_updates: &preference_updates,
        };
        let hidden = delta.hidden.into_iter().collect::<Vec<_>>();
        let purged = delta.purged.into_iter().collect::<Vec<_>>();
        let report = crate::dcp::commit_compression_with_projection(
            self.db,
            session,
            plan,
            Some(&metadata),
            &hidden,
            &purged,
        )
        .map_err(|error| RuntimeError::Compress(error.to_string()))?;
        *self.nudge_state.lock().expect("nudge lock") = next_states;
        self.stats.lock().expect("stats lock").compressions += 1;
        Ok(CompressReport {
            blocks: report.blocks.iter().map(|block| block.id.clone()).collect(),
            saved_tokens: report.saved_tokens,
            shrank: true,
        })
    }

    /// Bounded active projection: prune-bounded rows plus block summaries.
    ///
    /// Never materialises the covered archive; an active window above
    /// [`ACTIVE_CONTEXT_BYTES_CAP`] refuses the turn with an explicit
    /// diagnostic instead of silently dropping facts.
    pub(super) fn active_projection(&self, session: &str) -> Result<ActiveContext, RuntimeError> {
        self.active_projection_inner(session, true)
    }
    pub(super) fn active_projection_inner(
        &self,
        session: &str,
        checkpoint: bool,
    ) -> Result<ActiveContext, RuntimeError> {
        let after_seq = self
            .db
            .prune_bound(session)?
            .map(|(_, seq)| seq)
            .unwrap_or(0)
            .max(if checkpoint {
                self.db
                    .session_checkpoint(session)?
                    .map(|(seq, _)| seq)
                    .unwrap_or(0)
            } else {
                0
            });
        self.active_projection_range(
            session,
            after_seq,
            i64::MAX,
            checkpoint,
            ACTIVE_CONTEXT_BYTES_CAP,
        )
    }
    pub(super) fn active_projection_range(
        &self,
        session: &str,
        after_seq: i64,
        until: i64,
        checkpoint: bool,
        budget: usize,
    ) -> Result<ActiveContext, RuntimeError> {
        self.active_projection_selected(session, after_seq, until, checkpoint, budget, true)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn active_projection_selected(
        &self,
        session: &str,
        after_seq: i64,
        until: i64,
        checkpoint: bool,
        budget: usize,
        retain_selection: bool,
    ) -> Result<ActiveContext, RuntimeError> {
        let active = self
            .db
            .active_history_range(session, after_seq, until, budget)?;
        if active.overflow {
            return Err(RuntimeError::ContextOverflow {
                bytes: active.bytes,
                cap: ACTIVE_CONTEXT_BYTES_CAP,
            });
        }
        let mut blocks = self
            .db
            .compaction_graph(session, after_seq, retain_selection)?;
        let mut bounded = Vec::new();
        for block in blocks {
            if self
                .db
                .message_seq(session, &block.end_msg)?
                .is_some_and(|seq| seq <= until)
            {
                bounded.push(block);
            }
        }
        blocks = bounded;
        if !retain_selection {
            let by_id = blocks
                .iter()
                .cloned()
                .map(|mut b| {
                    b.summary = crate::dcp::authored_summary(&b.summary).to_owned();
                    (b.id.clone(), b)
                })
                .collect();
            for block in &mut blocks {
                block.summary = crate::dcp::expand_block(&by_id, &block.id, 0, &mut Vec::new())
                    .map_err(|_| RuntimeError::Storage)?;
                block.hot = Some(
                    serde_json::json!({"version":1,"active":true,"standalone":true,"protected":[],"logs":[]}),
                );
            }
        }
        for (id, first, last) in self.db.block_window_endpoints(session, after_seq)? {
            if let Some(block) = blocks.iter_mut().find(|b| b.id == id) {
                block.members.push(first.clone());
                if last != first {
                    block.members.push(last);
                }
            }
        }
        let mut positions = self.db.block_positions(session, after_seq)?;
        positions.retain(|(id, _)| blocks.iter().any(|b| &b.id == id));
        let mut projected = crate::dcp::project_active_rows(&active.rows, &blocks, &positions)
            .map_err(|error| RuntimeError::InvalidArgs(error.to_string()))?;
        if checkpoint && let Some((_, summary)) = self.db.session_checkpoint(session)? {
            projected.insert(
                0,
                (
                    if retain_selection {
                        "session-checkpoint"
                    } else {
                        "session-checkpoint-summary"
                    }
                    .into(),
                    "developer".into(),
                    format!("<conversation-checkpoint>\n{summary}\n</conversation-checkpoint>"),
                ),
            );
        }
        Ok(ActiveContext {
            after_seq,
            projected,
            blocks,
        })
    }

    /// Active rows only (prune-bounded, no block placement).
    #[allow(clippy::type_complexity)]
    fn active_rows(
        &self,
        session: &str,
    ) -> Result<(i64, Vec<(String, String, String)>), RuntimeError> {
        let after_seq = self
            .db
            .prune_bound(session)?
            .map(|(_, seq)| seq)
            .unwrap_or(0)
            .max(
                self.db
                    .session_checkpoint(session)?
                    .map(|(seq, _)| seq)
                    .unwrap_or(0),
            );
        let active = self
            .db
            .active_history(session, after_seq, ACTIVE_CONTEXT_BYTES_CAP)?;
        if active.overflow {
            return Err(RuntimeError::ContextOverflow {
                bytes: active.bytes,
                cap: ACTIVE_CONTEXT_BYTES_CAP,
            });
        }
        Ok((after_seq, active.rows))
    }

    pub(super) fn prepare_dcp_plan(
        &self,
        session: &str,
        ranges: &[crate::dcp::ValidatedRange],
        spec: &ProtectedSpec,
        config: Option<&DcpConfig>,
    ) -> Result<crate::dcp::CompressionPlan, RuntimeError> {
        let after_seq = self.db.prune_bound(session)?.map_or(0, |(_, seq)| seq).max(
            self.db
                .session_checkpoint(session)?
                .map_or(0, |(seq, _)| seq),
        );
        let (rows, graph, prune, next, revision) = self
            .db
            .compression_addressed_snapshot(session, after_seq, ranges)?;
        let messages = map_messages(&rows)?;
        let mut spec = spec.clone();
        if let Some(config) = config {
            apply_turn_protection(&rows, config, &mut spec);
        }
        let mut plan = crate::dcp::plan_active_compression(
            session,
            &messages,
            ranges,
            &spec,
            &graph,
            prune.as_deref(),
            next,
        )
        .map_err(|e| RuntimeError::Compress(e.to_string()))?;
        plan.projection_revision = Some(revision);
        Ok(plan)
    }

    /// The gain gate and accounting use the same request-local continuation as
    /// the next provider request. The raw turn journal is never projected in place.
    pub(super) fn measure_dcp_plan(
        &self,
        session: &str,
        plan: &mut crate::dcp::CompressionPlan,
        current: Option<&TurnLog>,
        lane: &TurnLane,
        config: &DcpConfig,
    ) -> Result<
        (
            crate::storage::DcpToolProjection,
            crate::storage::DcpToolProjection,
        ),
        RuntimeError,
    > {
        let context = self.active_projection(session)?;
        let (model, provider) = current
            .map(|log| {
                log.requests
                    .last()
                    .map(|request| (request.model.id.clone(), request.model.provider.clone()))
                    .unwrap_or_else(|| (log.model.clone(), log.provider.clone()))
            })
            .unwrap_or(self.db.dcp_wire_route(session)?.unwrap_or_default());
        let digest = current
            .map(|log| log.agent_digest.as_deref())
            .unwrap_or(lane.agent_digest.as_deref());
        let prior = |rows: &[(String, String, String)]| {
            rows.iter()
                .filter(|r| {
                    current.and_then(|log| log.user_message.as_deref()) != Some(r.0.as_str())
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let before_rows = &context.projected;
        let before_history = self.wire_history(
            session,
            &prior(before_rows),
            &context.blocks,
            &model,
            &provider,
            digest,
            context.after_seq,
        )?;
        let instruction_facts = self.db.instruction_view(session)?.1;
        let current_instruction_input = current
            .map(|log| log.instruction_input_for(&model, &provider, &instruction_facts))
            .unwrap_or_default();
        let current_input = current_instruction_input.as_slice();
        let raw_before = before_history
            .iter()
            .chain(current_input)
            .cloned()
            .collect::<Vec<_>>();
        let existing_projection = self
            .db
            .dcp_tool_projection_for_input(session, &raw_before)?;
        let delta = plan_dcp_strategies(&raw_before, config, &existing_projection);
        let mut candidate_projection = existing_projection.clone();
        candidate_projection
            .hidden
            .extend(delta.hidden.iter().cloned());
        candidate_projection
            .purged
            .extend(delta.purged.iter().cloned());
        let mut candidate = plan.context_blocks.clone();
        for block in &mut candidate {
            if plan.consumed_blocks.contains(&block.id) {
                block.members.clear();
            }
        }
        let before_logs = self.projected_wire_logs(
            session,
            context.after_seq,
            &prior(before_rows),
            &context.blocks,
        )?;
        for block in &mut plan.blocks {
            let start = self
                .db
                .message_seq(session, &block.start_msg)?
                .ok_or(RuntimeError::Storage)?;
            let end = self
                .db
                .message_seq(session, &block.end_msg)?
                .ok_or(RuntimeError::Storage)?;
            let mut covered = Vec::new();
            for raw in &before_logs {
                let value: serde_json::Value =
                    serde_json::from_str(raw).map_err(|_| RuntimeError::Storage)?;
                let anchor = value["user_message"]
                    .as_str()
                    .ok_or(RuntimeError::Storage)?;
                if self
                    .db
                    .message_seq(session, anchor)?
                    .is_some_and(|seq| start <= seq && seq <= end)
                {
                    covered.push(raw.clone());
                }
            }
            if let Some(hot) = &mut block.hot {
                hot["logs"] = self
                    .select_history_facts_with_producers(&covered, config, true)?
                    .into();
            }
        }
        candidate.extend(plan.blocks.iter().cloned());
        let mut positions = self.db.block_positions(session, context.after_seq)?;
        positions.retain(|(id, _)| !plan.consumed_blocks.contains(id));
        let members = plan
            .blocks
            .iter()
            .flat_map(|b| b.members.iter().cloned())
            .collect::<Vec<_>>();
        let seqs = self
            .db
            .message_seqs(session, &members)?
            .into_iter()
            .collect();
        let new_positions = crate::dcp::member_positions(&plan.blocks, &seqs)
            .into_iter()
            .filter(|(_, seq)| *seq > context.after_seq);
        positions.extend(new_positions);
        let rows = self.active_rows(session)?.1;
        let after_rows = crate::dcp::project_active_rows(&rows, &candidate, &positions)
            .map_err(|e| RuntimeError::Compress(e.to_string()))?;
        let after_history = self.wire_history(
            session,
            &prior(&after_rows),
            &candidate,
            &model,
            &provider,
            digest,
            context.after_seq,
        )?;
        let raw_after = after_history
            .iter()
            .chain(current_input)
            .cloned()
            .collect::<Vec<_>>();
        let identities = dcp_call_identities(&before_logs, current)?;
        let after_logs =
            self.projected_wire_logs(session, context.after_seq, &prior(&after_rows), &candidate)?;
        let after_identities = dcp_call_identities(&after_logs, current)?;
        let after_keys = after_identities
            .into_iter()
            .map(|(key, id)| (id, key))
            .collect::<BTreeMap<_, _>>();
        let remap = |projection: &crate::storage::DcpToolProjection| {
            let keys = |set: &std::collections::BTreeSet<crate::storage::DcpCallKey>| {
                set.iter()
                    .filter_map(|key| {
                        identities
                            .get(key)
                            .and_then(|id| after_keys.get(id))
                            .cloned()
                    })
                    .collect()
            };
            crate::storage::DcpToolProjection {
                hidden: keys(&projection.hidden),
                purged: keys(&projection.purged),
            }
        };
        let candidate_projection = remap(&candidate_projection);
        let delta = remap(&delta);
        plan.measurement.retire_keys = identities.keys().cloned().collect();
        plan.measurement.replacement_projection = Some(candidate_projection.clone());
        let before_calls = dcp_call_contents(&raw_before, &existing_projection);
        let after_calls = dcp_call_contents(&raw_after, &candidate_projection);
        let mut before_wire =
            dcp_continuation(&before_history, current_input, &existing_projection);
        let mut after_wire = dcp_continuation(&after_history, current_input, &candidate_projection);
        let with_reconciled = |wire: &mut Vec<InputItem>| {
            let mut fixed = Vec::new();
            crate::instructions::reconcile(&mut fixed, wire, &instruction_facts);
            fixed.append(wire);
            *wire = fixed;
        };
        with_reconciled(&mut before_wire);
        with_reconciled(&mut after_wire);
        let bytes = |rows: &[(String, String, String)], wire: &[InputItem]| {
            serde_json::to_vec(&(dcp_config_input(rows, config), wire))
                .map(|raw| raw.len())
                .map_err(|_| RuntimeError::Storage)
        };
        plan.before_bytes = bytes(before_rows, &before_wire)?;
        plan.after_bytes = bytes(&after_rows, &after_wire)?;
        let (removed, net_saved) = dcp_wire_estimates(&before_wire, &after_wire);
        let estimate = oc_core::dcp_view::estimate_content;
        let inherited: u64 = before_rows
            .iter()
            .filter(|r| plan.consumed_blocks.contains(&r.0))
            .map(|r| estimate(&r.2))
            .sum();
        plan.measurement.removed = removed.saturating_sub(inherited);
        plan.measurement.net_saved = net_saved;
        plan.saved_tokens = net_saved;
        let changed = before_calls
            .iter()
            .filter(|(key, value)| {
                (!value.0.is_empty() || !value.1.is_empty())
                    && after_calls.get(*key) != Some(*value)
            })
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        plan.measurement.prunes = changed.len() as u64;
        plan.measurement.unavailable_coverage =
            changed.iter().any(|key| !identities.contains_key(*key));
        plan.measurement.calls = changed
            .into_iter()
            .filter_map(|key| identities.get(key).cloned())
            .collect();
        let summary_tokens = |ids: &[crate::dcp::CompressionBlock]| {
            after_rows
                .iter()
                .filter(|r| ids.iter().any(|b| b.id == r.0))
                .map(|r| {
                    estimate(
                        r.2.strip_prefix(&format!("[compressed {}] ", r.0))
                            .unwrap_or(&r.2),
                    )
                })
                .sum()
        };
        plan.measurement.summary = summary_tokens(&plan.blocks);
        plan.measurement.active_summary = summary_tokens(&candidate);
        Ok((delta, candidate_projection))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn wire_history(
        &self,
        session: &str,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
        model: &str,
        provider: &str,
        agent_digest: Option<&str>,
        after_seq: i64,
    ) -> Result<Vec<InputItem>, RuntimeError> {
        let mut turns = BTreeMap::new();
        let mut changed_lane_prompts = BTreeMap::new();
        let mut represented = std::collections::BTreeSet::new();
        let move_epoch = self.db.session_move_epoch(session)?;
        let moved = move_epoch != 0;
        let instruction_facts = self.db.instruction_view(session)?.1;
        let logs = self.projected_wire_logs(session, after_seq, projected, blocks)?;
        if projected.iter().any(|r| r.0 == "session-checkpoint")
            && let Some(selection) = self.db.checkpoint_selection(session)?
        {
            let spec = self.history_protection_spec(session)?;
            for value in selection {
                if let Some(facts) = value.get("legacy_protected") {
                    let chosen = crate::dcp::select_legacy_protection(
                        facts.as_array().ok_or(RuntimeError::Storage)?,
                        &spec,
                    )
                    .map_err(|_| RuntimeError::Storage)?;
                    if !chosen.is_empty() {
                        let mut text = String::new();
                        crate::dcp::append_legacy_protection(&mut text, &chosen)
                            .map_err(|_| RuntimeError::Storage)?;
                        turns
                            .entry("session-checkpoint".to_string())
                            .or_insert_with(Vec::new)
                            .push(InputItem::message(InputRole::Developer, text));
                    }
                }
                if let Some(fact) = value.get("protected_message") {
                    let message = protected_message(fact)?;
                    if oc_core::context_plan::message_protected(&spec, &message) {
                        turns
                            .entry("session-checkpoint".to_string())
                            .or_insert_with(Vec::new)
                            .push(InputItem::message(
                                if message.role == oc_core::session::Role::User {
                                    InputRole::User
                                } else {
                                    InputRole::Assistant
                                },
                                message.text,
                            ));
                    }
                }
            }
        }
        for raw in logs {
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?;
            let log = TurnLog::from_json(&value).map_err(|_| RuntimeError::Storage)?;
            let prompt = value["_dcp_prompt"].as_str().map(str::to_owned);
            let block_id = value["_dcp_block"].as_str().map(str::to_owned);
            let Some(anchor) = log.user_message.clone() else {
                continue;
            };
            let changed_location = moved
                && (log.display["move_epoch"].as_i64().unwrap_or(0) != move_epoch
                    || log.display["location"].as_str() != Some(self.location.as_str()));
            if log.provider != provider
                && agent_digest != Some("__compaction__")
                && !changed_location
            {
                return Err(RuntimeError::InvalidArgs(
                    "session wire history belongs to a different provider/model".to_string(),
                ));
            }
            if changed_location
                || (agent_digest != Some("__compaction__")
                    && (log.agent_digest.as_deref() != agent_digest
                    // Pre-Build native sessions used the same empty default
                    // agent lane without a digest. Adopt only the unchanged
                    // builtin profile, preserving genuine tool/opaque pairs.
                    && !(log.agent_digest.is_none() && agent_digest == Some(crate::defs::agent_digest(&crate::defs::builtin_build()).as_str()))))
            {
                // Location or agent behavior changed: start a fresh causality lane
                // from public messages, but retain the originally expanded user
                // prompt for an uncompressed anchor. Never re-expand an invocation
                // using the current workspace's potentially changed commands.
                if let Some(prompt) = prompt.filter(|prompt| !prompt.is_empty()) {
                    changed_lane_prompts.insert(anchor, prompt);
                }
                continue;
            }
            if let Some(id) = value["assistant_message"].as_str() {
                represented.insert(id.to_string());
            }
            represented.extend(log.shell_notice_messages.iter().cloned());
            // Unknown operations cannot be replayed or assigned invented results.
            // Retain every committed pair; omit only unanswered function calls.
            let answered: std::collections::BTreeSet<String> = log
                .input
                .iter()
                .filter_map(|item| item.call_output().map(|(id, _)| id.to_owned()))
                .collect();
            let instruction_input = if agent_digest == Some("__compaction__") {
                log.input_for(model, provider)
            } else {
                log.instruction_input_for(model, provider, &instruction_facts)
            };
            let mut input = instruction_input
                .into_iter()
                .filter(|item| match item {
                    InputItem::ProviderOutput(value) if value["type"] == "function_call" => {
                        value["call_id"]
                            .as_str()
                            .is_some_and(|id| answered.contains(id))
                    }
                    _ => true,
                })
                .collect::<Vec<_>>();
            // Legacy command records without a usable prepared prompt retain
            // their immutable public invocation, never today's expansion.
            if block_id.is_none()
                && prompt.as_deref().is_none_or(str::is_empty)
                && let Some((_, _, public)) = projected.iter().find(|row| row.0 == anchor)
                && let Some(user) = input.iter_mut().find(|item| {
                    matches!(
                        item,
                        InputItem::Message {
                            role: InputRole::User,
                            ..
                        }
                    )
                })
            {
                *user = InputItem::message(InputRole::User, public);
            }
            turns
                .entry(block_id.unwrap_or(anchor))
                .or_insert_with(Vec::new)
                .extend(input);
        }
        let mut input = Vec::new();
        for (id, role, text) in projected {
            if !moved
                && id == "session-checkpoint"
                && let Some((_, _, _, Some(raw))) = self.db.checkpoint_record(session)?
                && self
                    .db
                    .checkpoint_model(session)?
                    .is_none_or(|origin| origin.id == model && origin.provider == provider)
            {
                input.push(InputItem::ProviderOutput(
                    serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?,
                ));
                if let Some(items) = turns.remove(id) {
                    input.extend(items);
                }
                continue;
            }
            if let Some(items) = turns.remove(id) {
                if id == "session-checkpoint" || blocks.iter().any(|block| &block.id == id) {
                    input.push(InputItem::message(InputRole::System, text));
                }
                input.extend(items);
            } else if !represented.contains(id) {
                let role = match role.as_str() {
                    "system" => InputRole::System,
                    "developer" => InputRole::Developer,
                    "user" => InputRole::User,
                    "assistant" => InputRole::Assistant,
                    _ => {
                        return Err(RuntimeError::InvalidArgs(
                            "history contains an unpaired tool message".to_string(),
                        ));
                    }
                };
                let text = if role == InputRole::User {
                    changed_lane_prompts
                        .get(id)
                        .map_or(text.as_str(), String::as_str)
                } else {
                    text
                };
                input.push(InputItem::message(role, text));
            }
        }
        Ok(input)
    }

    /// Bounded public-content estimate used by `/dcp`. Includes independently
    /// counted text, function arguments and outputs from durable active logs.
    /// This presentation estimate does not change provider admission heuristics.
    pub(crate) fn dcp_projection_estimate(
        &self,
        session: &str,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
        after_seq: i64,
    ) -> Result<Option<u64>, RuntimeError> {
        let logs = self.projected_wire_logs(session, after_seq, projected, blocks)?;
        let mut wire = Vec::new();
        let mut represented = std::collections::BTreeSet::new();
        for raw in logs {
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?;
            let log = TurnLog::from_json(&value).map_err(|_| RuntimeError::Storage)?;
            if let Some(anchor) = &log.user_message {
                represented.insert(anchor.clone());
            }
            if let Some(assistant) = value["assistant_message"].as_str() {
                represented.insert(assistant.into());
            }
            let input = log.input_for(&log.model, &log.provider);
            let answered = input
                .iter()
                .filter_map(|i| i.call_output().map(|(id, _)| id.to_owned()))
                .collect::<std::collections::BTreeSet<_>>();
            wire.extend(input.into_iter().filter(|i| {
                match i {
                    InputItem::ProviderOutput(v) if v["type"] == "function_call" => v["call_id"]
                        .as_str()
                        .is_some_and(|id| answered.contains(id)),
                    _ => true,
                }
            }));
        }
        for (id, _, text) in projected {
            if !represented.contains(id) {
                wire.push(InputItem::message(InputRole::User, text));
            }
        }
        let projection = self.db.dcp_tool_projection_for_input(session, &wire)?;
        apply_dcp_projection(&mut wire, &projection);
        Ok(Some(
            dcp_contents(&wire)
                .iter()
                .map(|text| oc_core::dcp_view::estimate_content(text))
                .sum(),
        ))
    }
}
#[cfg(test)]
#[path = "file_tool_tests.rs"]
mod file_tool_tests;
