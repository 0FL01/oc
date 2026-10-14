//! Code Mode tool catalog: TypeScript-like signatures, the model-facing listing and `search`.
//!
//! Ported from the donor's `tool-schema.ts`, `tool-runtime.ts` and `core/src/codemode/catalog.ts`.
//! This module is plain data and is shared between the turn thread and the interpreter thread.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value as Json, json};

/// One host tool callable from Code Mode, addressed by a dotted path such as `crw.scrape`.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeModeTool {
    pub path: String,
    pub description: String,
    pub input_schema: Json,
    pub output_schema: Option<Json>,
}

#[derive(Debug, Clone)]
struct Entry {
    tool: CodeModeTool,
    segments: Vec<String>,
    path_text: String,
    expression: String,
    signature: String,
    path_words: Vec<String>,
    search_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: Vec<Entry>,
    namespaces: BTreeMap<String, String>,
}

const DESCRIPTION_LIMIT: usize = 120;
const CHARACTERS_PER_TOKEN: usize = 4;
const INLINE_BUDGET: usize = 2_000;
const MAX_RENDER_DEPTH: usize = 8;
const DEFAULT_SEARCH_LIMIT: usize = 10;

pub(crate) const SEARCH_SIGNATURE: &str = "search(input: {\n  query?: string,\n  namespace?: string,\n  limit?: number,\n  offset?: number,\n}): {\n  items: Array<{\n    path: string,\n    description: string,\n    signature: string,\n  }>,\n  remaining: number,\n  next: {\n    offset: number,\n  } | null,\n}";

fn is_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn render_key(name: &str) -> String {
    if is_identifier(name) {
        name.to_string()
    } else {
        Json::String(name.to_string()).to_string()
    }
}

/// `tools.a.b` with bracket notation for non-identifier segments.
pub fn tool_expression(path: &str) -> String {
    segments_expression(path.split('.'))
}

fn segments_expression<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::from("tools");
    for segment in segments {
        if is_identifier(segment) {
            out.push('.');
            out.push_str(segment);
        } else {
            out.push('[');
            out.push_str(&Json::String(segment.to_string()).to_string());
            out.push(']');
        }
    }
    out
}

fn tokenize(query: &str) -> Vec<String> {
    let mut spaced = String::with_capacity(query.len() + 8);
    let mut prev: Option<char> = None;
    for c in query.chars() {
        if let Some(p) = prev
            && (p.is_ascii_lowercase() || p.is_ascii_digit())
            && c.is_ascii_uppercase()
        {
            spaced.push(' ');
        }
        spaced.push(c);
        prev = Some(c);
    }
    spaced
        .to_lowercase()
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|term| !term.is_empty() && *term != "*")
        .map(str::to_string)
        .collect()
}

fn term_forms(term: &str) -> Vec<String> {
    let mut forms = vec![term.to_string()];
    if term.ends_with("es") && term.len() > 3 {
        forms.push(term[..term.len() - 2].to_string());
    }
    if term.ends_with('s') && term.len() > 2 {
        forms.push(term[..term.len() - 1].to_string());
    }
    forms
}

// ----- schema rendering ----------------------------------------------------------------

struct Render {
    definitions: BTreeMap<String, Json>,
    pretty: bool,
}

fn json_number(value: &Json) -> Option<String> {
    value.as_f64().map(|_| value.to_string())
}

fn doc_tags(schema: &Map<String, Json>) -> Vec<String> {
    let mut tags = Vec::new();
    if schema.get("deprecated") == Some(&Json::Bool(true)) {
        tags.push("@deprecated".to_string());
    }
    if let Some(default) = schema.get("default") {
        tags.push(format!("@default {default}"));
    }
    if let Some(Json::String(format)) = schema.get("format") {
        tags.push(format!("@format {format}"));
    }
    if schema.get("type") == Some(&Json::String("integer".into())) {
        tags.push("@integer".into());
    }
    for key in [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "minLength",
        "maxLength",
    ] {
        if let Some(n) = schema.get(key).and_then(json_number) {
            tags.push(format!("@{key} {n}"));
        }
    }
    if let Some(Json::String(pattern)) = schema.get("pattern") {
        tags.push(format!("@pattern {pattern}"));
    }
    for key in ["minItems", "maxItems"] {
        if let Some(n) = schema.get(key).and_then(json_number) {
            tags.push(format!("@{key} {n}"));
        }
    }
    if schema.get("uniqueItems") == Some(&Json::Bool(true)) {
        tags.push("@uniqueItems true".into());
    }
    tags
}

fn doc_lines(schema: &Map<String, Json>, width: usize) -> Vec<String> {
    let summary = doc_tags(schema).join(" ");
    let description = schema
        .get("description")
        .and_then(Json::as_str)
        .unwrap_or("");
    let mut lines: Vec<String> = description
        .split('\n')
        .map(|l| l.trim_end().to_string())
        .collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if !summary.is_empty() && lines.len() == 1 {
        let first = &lines[0];
        let inline = format!(
            "{first}{} {summary}",
            if first.ends_with('.') { "" } else { "." }
        );
        if width + inline.len() + 7 <= 120 {
            return vec![inline];
        }
    }
    if lines.len() == 1 && summary.is_empty() {
        return lines;
    }
    let mut out = lines;
    if !summary.is_empty() {
        out.push(summary);
    }
    out
}

fn type_list(schema: &Map<String, Json>) -> Vec<String> {
    match schema.get("type") {
        Some(Json::String(t)) => vec![t.clone()],
        Some(Json::Array(items)) => items
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn jsdoc(schema: &Map<String, Json>, pad: &str) -> String {
    let mut content = doc_lines(schema, pad.len());
    let types = type_list(schema);
    let mut append = |label: &str, child: &Json| {
        if let Some(child) = child.as_object() {
            for (index, line) in doc_lines(child, pad.len() + label.len() + 2)
                .into_iter()
                .enumerate()
            {
                if index == 0 {
                    content.push(format!("{label}: {line}"));
                } else if line.is_empty() {
                    content.push(String::new());
                } else {
                    content.push(format!("  {line}"));
                }
            }
        }
    };
    if types.iter().any(|t| t == "array")
        && let Some(items) = schema.get("items")
    {
        append("Each item", items);
    }
    if (types.iter().any(|t| t == "object") || schema.contains_key("properties"))
        && let Some(additional @ Json::Object(_)) = schema.get("additionalProperties")
    {
        let has_props = schema
            .get("properties")
            .and_then(Json::as_object)
            .is_some_and(|p| !p.is_empty());
        append(
            if has_props {
                "Each additional value"
            } else {
                "Each value"
            },
            additional,
        );
    }
    if content.is_empty() {
        return String::new();
    }
    let escaped: Vec<String> = content.iter().map(|l| l.replace("*/", "* /")).collect();
    if escaped.len() == 1 && pad.len() + escaped[0].len() + 7 <= 120 {
        return format!("{pad}/** {} */\n", escaped[0]);
    }
    let body: Vec<String> = escaped
        .iter()
        .map(|l| {
            if l.is_empty() {
                format!("{pad} *")
            } else {
                format!("{pad} * {l}")
            }
        })
        .collect();
    format!("{pad}/**\n{}\n{pad} */\n", body.join("\n"))
}

fn intersection(members: &[String]) -> String {
    let concrete: Vec<&String> = members.iter().filter(|m| m.as_str() != "unknown").collect();
    match concrete.len() {
        0 => "unknown".into(),
        1 => concrete[0].clone(),
        _ => concrete
            .iter()
            .map(|m| {
                if m.contains(" | ") {
                    format!("({m})")
                } else {
                    (*m).clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" & "),
    }
}

fn ref_name(reference: &str) -> Option<String> {
    let rest = reference
        .strip_prefix("#/$defs/")
        .or_else(|| reference.strip_prefix("#/definitions/"))?;
    if rest.contains('/') {
        return None;
    }
    Some(rest.replace("~1", "/").replace("~0", "~"))
}

impl Render {
    fn render(&self, schema: &Json, depth: usize, seen: &BTreeSet<String>) -> String {
        if depth > MAX_RENDER_DEPTH {
            return "unknown".into();
        }
        let Some(map) = schema.as_object() else {
            return if schema == &Json::Bool(false) {
                "never".into()
            } else {
                "unknown".into()
            };
        };
        if map.contains_key("definitions") || map.contains_key("$defs") {
            let mut definitions = self.definitions.clone();
            for key in ["definitions", "$defs"] {
                if let Some(Json::Object(defs)) = map.get(key) {
                    for (name, def) in defs {
                        definitions.insert(name.clone(), def.clone());
                    }
                }
            }
            let nested = Render {
                definitions,
                pretty: self.pretty,
            };
            return nested.render_map_local(map, depth, seen);
        }
        self.render_map_local(map, depth, seen)
    }

    fn without(map: &Map<String, Json>, keys: &[&str]) -> Json {
        let mut copy = map.clone();
        for key in keys {
            copy.remove(*key);
        }
        Json::Object(copy)
    }

    fn render_owned(&self, schema: Json, depth: usize, seen: &BTreeSet<String>) -> String {
        self.render_local(&schema, depth, seen)
    }

    fn render_local(&self, schema: &Json, depth: usize, seen: &BTreeSet<String>) -> String {
        self.render(schema, depth, seen)
    }

    fn render_map_local(
        &self,
        map: &Map<String, Json>,
        depth: usize,
        seen: &BTreeSet<String>,
    ) -> String {
        if let Some(Json::String(reference)) = map.get("$ref") {
            let Some(name) = ref_name(reference) else {
                return "unknown".into();
            };
            let Some(def) = self.definitions.get(&name) else {
                return "unknown".into();
            };
            if seen.contains(&name) {
                return "unknown".into();
            }
            let mut next = seen.clone();
            next.insert(name);
            return intersection(&[
                self.render_local(def, depth, &next),
                self.render_owned(Self::without(map, &["$ref"]), depth + 1, seen),
            ]);
        }
        if let Some(constant) = map.get("const") {
            return constant.to_string();
        }
        if let Some(Json::Array(values)) = map.get("enum") {
            return values
                .iter()
                .map(Json::to_string)
                .collect::<Vec<_>>()
                .join(" | ");
        }
        let alternatives = map.get("anyOf").or_else(|| map.get("oneOf"));
        if let Some(Json::Array(alternatives)) = alternatives {
            let is_number = |item: &Json| item.get("type") == Some(&Json::String("number".into()));
            if alternatives.iter().any(is_number)
                && alternatives.iter().all(|item| {
                    is_number(item)
                        || (item.get("type") == Some(&Json::String("string".into()))
                            && item.get("enum").and_then(Json::as_array).is_some_and(|e| {
                                e.len() == 1
                                    && matches!(
                                        e[0].as_str(),
                                        Some("NaN" | "Infinity" | "-Infinity")
                                    )
                            }))
                })
            {
                return "number".into();
            }
            let members: Vec<String> = alternatives
                .iter()
                .map(|item| self.render_local(item, depth + 1, seen))
                .collect();
            if members.iter().any(|m| m == "unknown") {
                return "unknown".into();
            }
            return intersection(&[
                members.join(" | "),
                self.render_owned(Self::without(map, &["anyOf", "oneOf"]), depth + 1, seen),
            ]);
        }
        if let Some(Json::Array(all)) = map.get("allOf") {
            let mut members =
                vec![self.render_owned(Self::without(map, &["allOf"]), depth + 1, seen)];
            members.extend(
                all.iter()
                    .map(|item| self.render_local(item, depth + 1, seen)),
            );
            return intersection(&members);
        }
        if let Some(Json::Array(types)) = map.get("type") {
            return types
                .iter()
                .map(|t| {
                    let mut copy = map.clone();
                    copy.insert("type".into(), t.clone());
                    self.render_owned(Json::Object(copy), depth + 1, seen)
                })
                .collect::<Vec<_>>()
                .join(" | ");
        }
        match map.get("type").and_then(Json::as_str) {
            Some("string") => return "string".into(),
            Some("number" | "integer") => return "number".into(),
            Some("boolean") => return "boolean".into(),
            Some("null") => return "null".into(),
            Some("array") => {
                let items = map
                    .get("items")
                    .map(|i| self.render_local(i, depth + 1, seen))
                    .unwrap_or_else(|| "unknown".into());
                return format!("Array<{items}>");
            }
            _ => {}
        }
        if map.get("type").and_then(Json::as_str) == Some("object")
            || map.contains_key("properties")
        {
            let required: BTreeSet<&str> = map
                .get("required")
                .and_then(Json::as_array)
                .map(|r| r.iter().filter_map(Json::as_str).collect())
                .unwrap_or_default();
            let empty = Map::new();
            let properties = map
                .get("properties")
                .and_then(Json::as_object)
                .unwrap_or(&empty);
            let index_type = match map.get("additionalProperties") {
                Some(additional @ Json::Object(_)) => {
                    Some(self.render_local(additional, depth + 1, seen))
                }
                _ => None,
            };
            let field = |name: &str, value: &Json| {
                format!(
                    "{}{}: {}",
                    render_key(name),
                    if required.contains(name) { "" } else { "?" },
                    self.render_local(value, depth + 1, seen)
                )
            };
            if !self.pretty {
                let mut fields: Vec<String> = properties.iter().map(|(n, v)| field(n, v)).collect();
                if let Some(index) = &index_type {
                    fields.push(format!("[key: string]: {index}"));
                }
                return if fields.is_empty() {
                    "{}".into()
                } else {
                    format!("{{ {} }}", fields.join("; "))
                };
            }
            if properties.is_empty() && index_type.is_none() {
                return "{}".into();
            }
            let pad = "  ".repeat(depth + 1);
            let mut lines: Vec<String> = properties
                .iter()
                .map(|(name, value)| {
                    let doc = value
                        .as_object()
                        .map(|m| jsdoc(m, &pad))
                        .unwrap_or_default();
                    format!("{doc}{pad}{},", field(name, value))
                })
                .collect();
            if let Some(index) = &index_type {
                lines.push(format!("{pad}[key: string]: {index},"));
            }
            return format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(depth));
        }
        "unknown".into()
    }
}

/// Renders a JSON Schema as a TypeScript-like type.
pub fn schema_to_typescript(schema: &Json, pretty: bool) -> String {
    let render = Render {
        definitions: BTreeMap::new(),
        pretty,
    };
    render.render(schema, 0, &BTreeSet::new())
}

fn signature(tool: &CodeModeTool, expression: &str) -> String {
    let compact = schema_to_typescript(&tool.input_schema, false);
    let input = if compact == "{}" {
        "()".to_string()
    } else {
        format!(
            "(input: {})",
            schema_to_typescript(&tool.input_schema, true)
        )
    };
    let output = match &tool.output_schema {
        Some(schema) => schema_to_typescript(schema, true),
        None => "unknown".to_string(),
    };
    format!("{expression}{input}: Promise<{output}>")
}

fn input_property_text(schema: &Json) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(Json::Object(props)) = schema.get("properties") {
        for (name, value) in props {
            out.push(name.clone());
            if let Some(description) = value.get("description").and_then(Json::as_str) {
                out.push(description.to_string());
            }
        }
    }
    out
}

/// Namespace summary for the model-facing listing.
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceSummary {
    pub name: String,
    pub description: Option<String>,
    pub count: usize,
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub total: usize,
    pub shown: usize,
    pub namespaces: Vec<NamespaceSummary>,
}

fn cost(text: &str) -> usize {
    (text.chars().count() as f64 / CHARACTERS_PER_TOKEN as f64).round() as usize
}

fn namespace_line(namespace: &NamespaceSummary) -> String {
    let count = if namespace.count == 1 {
        "1 tool".to_string()
    } else {
        format!("{} tools", namespace.count)
    };
    let label = if namespace.entries.len() == namespace.count {
        count
    } else if namespace.entries.is_empty() {
        format!("{count}, none shown")
    } else {
        format!("{count}, {} shown", namespace.entries.len())
    };
    let description = namespace
        .description
        .as_ref()
        .map(|d| format!(" // {d}"))
        .unwrap_or_default();
    format!("- {} ({label}){description}", namespace.name)
}

impl Catalog {
    /// Builds a catalog. The last tool for a duplicate path wins; entries are sorted by path.
    pub fn new(tools: Vec<CodeModeTool>, namespace_descriptions: Vec<(String, String)>) -> Catalog {
        Self::with_paths(
            tools
                .into_iter()
                .map(|tool| {
                    let segments = tool.path.split('.').map(str::to_string).collect();
                    (tool, segments)
                })
                .collect(),
            namespace_descriptions,
        )
    }

    /// The runtime retains literal MCP segments and uses `tool.path` only as its host key.
    pub(crate) fn with_paths(
        tools: Vec<(CodeModeTool, Vec<String>)>,
        namespace_descriptions: Vec<(String, String)>,
    ) -> Catalog {
        let mut by_path: BTreeMap<Vec<String>, CodeModeTool> = BTreeMap::new();
        for (tool, segments) in tools {
            if segments.is_empty() || segments.iter().any(String::is_empty) {
                continue;
            }
            by_path.insert(segments, tool);
        }
        let namespaces: BTreeMap<String, String> = namespace_descriptions.into_iter().collect();
        let mut entries: Vec<Entry> = by_path
            .into_iter()
            .map(|(segments, tool)| {
                let path_text = segments.join(".");
                let expression = segments_expression(segments.iter().map(String::as_str));
                let mut parts = vec![path_text.clone(), tool.description.clone()];
                let mut prefix = String::new();
                for segment in &segments {
                    if !prefix.is_empty() {
                        prefix.push('.');
                    }
                    prefix.push_str(segment);
                    if let Some(description) = namespaces.get(&prefix) {
                        parts.push(description.clone());
                    }
                }
                parts.extend(input_property_text(&tool.input_schema));
                Entry {
                    signature: signature(&tool, &expression),
                    path_words: tokenize(&path_text),
                    search_text: parts.join("\n").to_lowercase(),
                    segments,
                    path_text,
                    expression,
                    tool,
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            a.path_text
                .cmp(&b.path_text)
                .then_with(|| a.segments.cmp(&b.segments))
        });
        Catalog {
            entries,
            namespaces,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn tools(&self) -> impl Iterator<Item = &CodeModeTool> {
        self.entries.iter().map(|e| &e.tool)
    }

    pub(crate) fn get(&self, path: &[String]) -> Option<&CodeModeTool> {
        self.entries
            .iter()
            .find(|e| e.segments == path)
            .map(|e| &e.tool)
    }

    /// Whether `prefix` names a namespace that contains tools.
    pub(crate) fn is_namespace(&self, prefix: &[String]) -> bool {
        prefix.is_empty()
            || self
                .entries
                .iter()
                .any(|e| e.segments.len() > prefix.len() && e.segments.starts_with(prefix))
    }

    /// Child names below a namespace path, as `Object.keys(tools.ns)` shows them.
    pub(crate) fn children(&self, prefix: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for entry in &self.entries {
            if entry.segments.starts_with(prefix)
                && let Some(child) = entry.segments.get(prefix.len())
                && !out.contains(child)
            {
                out.push(child.clone());
            }
        }
        out
    }

    pub fn summary(&self) -> Summary {
        self.summary_with_budget(INLINE_BUDGET)
    }

    pub fn summary_with_budget(&self, budget: usize) -> Summary {
        struct Ns {
            name: String,
            description: Option<String>,
            listings: Vec<(String, String)>,
            order: Vec<(usize, usize)>,
            selected: BTreeSet<usize>,
            next: usize,
        }
        let mut groups: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
        for entry in &self.entries {
            let head = entry.segments[0].clone();
            groups.entry(head).or_default().push(entry);
        }
        let mut namespaces: Vec<Ns> = groups
            .into_iter()
            .map(|(name, entries)| {
                let listings: Vec<(String, String)> = entries
                    .iter()
                    .map(|entry| {
                        let first = entry
                            .tool
                            .description
                            .split('\n')
                            .next()
                            .unwrap_or("")
                            .trim();
                        let description = if first.chars().count() > DESCRIPTION_LIMIT {
                            let cut: String = first.chars().take(DESCRIPTION_LIMIT - 3).collect();
                            format!("{cut}...")
                        } else {
                            first.to_string()
                        };
                        let suffix = if description.is_empty() {
                            String::new()
                        } else {
                            format!(" // {description}")
                        };
                        (
                            entry.path_text.clone(),
                            format!("  - {}{suffix}", entry.signature),
                        )
                    })
                    .collect();
                let mut order: Vec<(usize, usize)> = listings
                    .iter()
                    .enumerate()
                    .map(|(i, (_, line))| (i, cost(line)))
                    .collect();
                order.sort_by(|a, b| {
                    a.1.cmp(&b.1)
                        .then_with(|| listings[a.0].0.cmp(&listings[b.0].0))
                });
                Ns {
                    description: self.namespaces.get(&name).cloned(),
                    name,
                    listings,
                    order,
                    selected: BTreeSet::new(),
                    next: 0,
                }
            })
            .collect();
        let mut remaining = budget as i64;
        for ns in &namespaces {
            let line = namespace_line(&NamespaceSummary {
                name: ns.name.clone(),
                description: ns.description.clone(),
                count: ns.listings.len(),
                entries: Vec::new(),
            });
            remaining -= cost(&line) as i64;
        }
        let mut active: Vec<usize> = (0..namespaces.len()).collect();
        while !active.is_empty() {
            let mut still = Vec::new();
            for index in active {
                let ns = &mut namespaces[index];
                let Some(&(listing, c)) = ns.order.get(ns.next) else {
                    continue;
                };
                if c as i64 > remaining {
                    continue;
                }
                ns.selected.insert(listing);
                ns.next += 1;
                remaining -= c as i64;
                if ns.next < ns.order.len() {
                    still.push(index);
                }
            }
            active = still;
        }
        let summaries: Vec<NamespaceSummary> = namespaces
            .into_iter()
            .map(|ns| NamespaceSummary {
                count: ns.listings.len(),
                entries: ns
                    .listings
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| ns.selected.contains(i))
                    .map(|(_, l)| l.clone())
                    .collect(),
                name: ns.name,
                description: ns.description,
            })
            .collect();
        Summary {
            total: self.entries.len(),
            shown: summaries.iter().map(|n| n.entries.len()).sum(),
            namespaces: summaries,
        }
    }

    /// Model-facing catalog text, as donor `CodeModeInstructions.render`.
    pub fn render(&self) -> String {
        let summary = self.summary();
        if summary.total == 0 {
            return "No Code Mode tools are currently available. Later Code Mode catalog updates may add or remove tools. Do not call `execute` unless there is at least one available Code Mode tool.".into();
        }
        let more = summary.shown < summary.total;
        let mut out = format!(
            "The Code Mode tool catalog below is {}.\n\n{} the complete set of tools callable inside `execute`. It does not affect tools exposed directly outside Code Mode.",
            if more { "partial" } else { "complete" },
            if more {
                "The Code Mode catalog and `search` results are"
            } else {
                "This catalog is"
            }
        );
        if more {
            out.push_str("\n\n## Search\n\nCall `search(...)` to discover exact paths and signatures for additional tools:\n\n- ");
            out.push_str(SEARCH_SIGNATURE);
        }
        out.push_str("\n\n## Available tools\n\n");
        let mut lines = Vec::new();
        for ns in &summary.namespaces {
            lines.push(namespace_line(ns));
            lines.extend(ns.entries.iter().map(|(_, line)| line.clone()));
        }
        out.push_str(&lines.join("\n"));
        out
    }

    /// Donor-compatible `search(...)` result.
    pub fn search(
        &self,
        query: Option<&str>,
        namespace: Option<&str>,
        limit: Option<usize>,
        offset: usize,
    ) -> Json {
        let query = query.unwrap_or("");
        let scoped: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| match namespace {
                None => true,
                Some(ns) => {
                    e.tool.path == ns
                        || (1..e.segments.len()).any(|n| e.segments[..n].join(".") == ns)
                }
            })
            .collect();
        let trimmed = query.trim();
        let path_query = trimmed.strip_prefix("tools.").unwrap_or(trimmed);
        let exact = if path_query.is_empty() {
            None
        } else {
            scoped
                .iter()
                .find(|e| {
                    e.tool.path == path_query
                        || e.expression == trimmed
                        || e.expression.strip_prefix("tools.") == Some(path_query)
                })
                .copied()
        };
        let terms: Vec<Vec<String>> = tokenize(query).iter().map(|t| term_forms(t)).collect();
        let ranked: Vec<&Entry> = match exact {
            Some(entry) => vec![entry],
            None => {
                let mut scored: Vec<(usize, &Entry)> = scoped
                    .iter()
                    .map(|entry| {
                        let path = entry.path_text.to_lowercase();
                        let description = entry.tool.description.to_lowercase();
                        let score = terms.iter().fold(0, |total, forms| {
                            let any = |f: &dyn Fn(&str) -> bool| forms.iter().any(|form| f(form));
                            total
                                + if any(&|f| path == f || path.ends_with(&format!(".{f}"))) {
                                    20
                                } else {
                                    0
                                }
                                + if any(&|f| entry.path_words.iter().any(|w| w == f)) {
                                    12
                                } else {
                                    0
                                }
                                + if any(&|f| path.contains(f)) { 8 } else { 0 }
                                + if any(&|f| description.contains(f)) {
                                    4
                                } else {
                                    0
                                }
                                + if any(&|f| entry.search_text.contains(f)) {
                                    2
                                } else {
                                    0
                                }
                        });
                        (score, *entry)
                    })
                    .filter(|(score, _)| terms.is_empty() || *score > 0)
                    .collect();
                scored.sort_by(|a, b| {
                    b.0.cmp(&a.0)
                        .then_with(|| a.1.path_text.cmp(&b.1.path_text))
                });
                scored.into_iter().map(|(_, e)| e).collect()
            }
        };
        let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        let items: Vec<Json> = ranked
            .iter()
            .skip(offset)
            .take(limit)
            .map(|e| {
                json!({
                    "path": e.expression,
                    "description": e.tool.description,
                    "signature": e.signature,
                })
            })
            .collect();
        let remaining = ranked.len().saturating_sub(offset + items.len());
        let next = if remaining > 0 {
            json!({ "offset": offset + items.len() })
        } else {
            Json::Null
        };
        json!({ "items": items, "remaining": remaining, "next": next })
    }
}
