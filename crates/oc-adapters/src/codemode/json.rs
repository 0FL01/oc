//! JSON: `JSON.parse`/`JSON.stringify` and the host data boundary.
//!
//! The boundary follows the donor's table: values cross as `JSON.stringify` would serialize
//! them, except that promises are awaited, Sets become arrays, URLSearchParams become their
//! query string, Errors become `{ name, message, ...own }`, byte arrays are rejected and own
//! `__proto__` keys are dropped.

use futures_util::future::LocalBoxFuture;
use serde_json::{Map, Number, Value as Json};

use super::interp::{Cx, ErrorKind, R};
use super::value::{Func, JsStr, Kind, Obj, PropKey, Value, number_to_string};

const MAX_DEPTH: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// In-program `JSON.stringify`.
    Program,
    /// Values leaving the interpreter (tool inputs and the program result).
    Boundary,
}

struct Walk<'a> {
    cx: &'a Cx,
    mode: Mode,
    replacer: Option<Value>,
    allow: Option<Vec<JsStr>>,
    stack: Vec<Obj>,
}

fn number_json(n: f64) -> Json {
    if !n.is_finite() {
        return Json::Null;
    }
    if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
        return Json::Number(Number::from(n as i64));
    }
    Number::from_f64(n).map(Json::Number).unwrap_or(Json::Null)
}

impl Walk<'_> {
    fn walk<'b>(
        &'b mut self,
        holder: Value,
        key: JsStr,
        value: Value,
    ) -> LocalBoxFuture<'b, R<Option<Json>>> {
        Box::pin(async move {
            let cx = self.cx;
            let mut value = value;
            if self.mode == Mode::Boundary
                && let Value::Obj(obj) = &value
                && matches!(obj.borrow().kind, Kind::Promise(_))
            {
                value = cx.await_value(value).await?;
            }
            if let Value::Obj(_) = &value {
                let to_json = cx.get_str(&value, "toJSON").await?;
                if to_json.is_callable() {
                    value = cx
                        .call(&to_json, value.clone(), vec![Value::Str(key.clone())])
                        .await?;
                }
            }
            if let Some(replacer) = &self.replacer {
                value = cx
                    .call(replacer, holder, vec![Value::Str(key.clone()), value])
                    .await?;
            }
            let obj = match &value {
                Value::Undefined | Value::Sym(_) => return Ok(None),
                Value::Null => return Ok(Some(Json::Null)),
                Value::Bool(b) => return Ok(Some(Json::Bool(*b))),
                Value::Num(n) => return Ok(Some(number_json(*n))),
                Value::Str(s) => return Ok(Some(Json::String(s.to_string()))),
                Value::Obj(obj) => obj.clone(),
            };
            if obj.is_callable() {
                return Ok(None);
            }
            enum Special {
                None,
                Boxed(Value),
                Promise,
                Bytes,
                Set(Vec<Value>),
                Params(String),
                Error,
                Empty,
            }
            let special = match &obj.borrow().kind {
                Kind::Boxed(inner) => Special::Boxed(inner.clone()),
                Kind::Promise(_) => Special::Promise,
                Kind::Bytes(_) => Special::Bytes,
                Kind::Set(set) if self.mode == Mode::Boundary => {
                    Special::Set(set.values().cloned().collect())
                }
                Kind::SearchParams { list, .. } if self.mode == Mode::Boundary => {
                    Special::Params(super::builtins::web::serialize_params(list))
                }
                Kind::Error => Special::Error,
                Kind::Map(_)
                | Kind::Set(_)
                | Kind::RegExp(_)
                | Kind::Generator(_)
                | Kind::Iter(_) => Special::Empty,
                _ => Special::None,
            };
            match special {
                Special::Boxed(inner) => return self.walk(Value::Undefined, key, inner).await,
                Special::Promise => {
                    return Err(cx.type_error("Cannot serialize a Promise; await it first"));
                }
                Special::Bytes if self.mode == Mode::Boundary => {
                    return Err(cx.type_error(
                        "Uint8Array cannot cross the Code Mode boundary; encode it as text (for example base64) first",
                    ));
                }
                Special::Set(items) => {
                    let array = cx.array_value(items);
                    return self.walk(Value::Undefined, key, array).await;
                }
                Special::Params(text) => return Ok(Some(Json::String(text))),
                Special::Empty => return Ok(Some(Json::Object(Map::new()))),
                _ => {}
            }
            if self.stack.iter().any(|o| o.ptr_eq(&obj)) {
                return Err(cx.type_error("Converting circular structure to JSON"));
            }
            if self.stack.len() >= MAX_DEPTH {
                return Err(cx.range_error("JSON nesting exceeds the maximum depth"));
            }
            self.stack.push(obj.clone());
            let result = if let Some(items) = obj.array_items() {
                let mut out = Vec::with_capacity(items.len());
                for (index, item) in items.into_iter().enumerate() {
                    cx.tick()?;
                    let element = self
                        .walk(Value::Obj(obj.clone()), index.to_string().into(), item)
                        .await?;
                    out.push(element.unwrap_or(Json::Null));
                }
                Json::Array(out)
            } else if let Kind::Bytes(bytes) = &obj.borrow().kind {
                let mut map = Map::new();
                for (index, b) in bytes.iter().enumerate() {
                    map.insert(index.to_string(), Json::Number(Number::from(*b)));
                }
                Json::Object(map)
            } else {
                let mut map = Map::new();
                let is_error = matches!(special, Special::Error);
                if is_error {
                    let name = cx.get_str(&value, "name").await?;
                    let message = cx.get_str(&value, "message").await?;
                    map.insert("name".into(), Json::String(cx.to_str(&name)?.to_string()));
                    map.insert(
                        "message".into(),
                        Json::String(cx.to_str(&message)?.to_string()),
                    );
                }
                let keys: Vec<JsStr> = match &self.allow {
                    Some(allow) => allow.clone(),
                    None => cx
                        .own_keys(&obj, true, false)
                        .into_iter()
                        .filter_map(|k| match k {
                            PropKey::Str(s) => Some(s),
                            PropKey::Sym(_) => None,
                        })
                        .collect(),
                };
                for name in keys {
                    cx.tick()?;
                    if self.mode == Mode::Boundary && name.as_str() == "__proto__" {
                        continue;
                    }
                    if is_error && matches!(name.as_str(), "name" | "message" | "stack") {
                        continue;
                    }
                    let item = cx.get(&value, &PropKey::Str(name.clone())).await?;
                    if let Some(json) = self
                        .walk(Value::Obj(obj.clone()), name.clone(), item)
                        .await?
                    {
                        map.insert(name.to_string(), json);
                    }
                }
                Json::Object(map)
            };
            self.stack.pop();
            Ok(Some(result))
        })
    }
}

/// Converts a program value into host JSON. `undefined` becomes `null`.
pub(super) async fn to_host(cx: &Cx, value: &Value) -> R<Json> {
    let mut walk = Walk {
        cx,
        mode: Mode::Boundary,
        replacer: None,
        allow: None,
        stack: Vec::new(),
    };
    Ok(walk
        .walk(Value::Undefined, "".into(), value.clone())
        .await?
        .unwrap_or(Json::Null))
}

pub(super) fn from_json(cx: &Cx, json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Num(n.as_f64().unwrap_or(f64::NAN)),
        Json::String(s) => Value::str(s.as_str()),
        Json::Array(items) => {
            cx.array_value(items.iter().map(|item| from_json(cx, item)).collect())
        }
        Json::Object(map) => {
            let obj = cx.new_object();
            for (key, value) in map {
                if key == "__proto__" {
                    continue;
                }
                obj.set_data(PropKey::Str(key.as_str().into()), from_json(cx, value));
            }
            Value::Obj(obj)
        }
    }
}

pub(super) fn parse(cx: &Cx, text: &str) -> R<Value> {
    match serde_json::from_str::<Json>(text) {
        Ok(json) => Ok(from_json(cx, &json)),
        Err(err) => Err(cx.throw(
            ErrorKind::Syntax,
            format!("Unexpected token in JSON: {err}"),
        )),
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Serializes JSON with JavaScript number formatting and the given indentation gap.
pub(super) fn write_json(json: &Json, gap: &str) -> String {
    fn go(out: &mut String, json: &Json, gap: &str, indent: &str) {
        match json {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Number(n) => out.push_str(&number_to_string(n.as_f64().unwrap_or(f64::NAN))),
            Json::String(s) => write_string(out, s),
            Json::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                let inner = format!("{indent}{gap}");
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    if !gap.is_empty() {
                        out.push('\n');
                        out.push_str(&inner);
                    }
                    go(out, item, gap, &inner);
                }
                if !gap.is_empty() {
                    out.push('\n');
                    out.push_str(indent);
                }
                out.push(']');
            }
            Json::Object(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                let inner = format!("{indent}{gap}");
                for (index, (key, value)) in map.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    if !gap.is_empty() {
                        out.push('\n');
                        out.push_str(&inner);
                    }
                    write_string(out, key);
                    out.push(':');
                    if !gap.is_empty() {
                        out.push(' ');
                    }
                    go(out, value, gap, &inner);
                }
                if !gap.is_empty() {
                    out.push('\n');
                    out.push_str(indent);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    go(&mut out, json, gap, "");
    out
}

/// `JSON.stringify(value, replacer, space)`; `None` means `undefined`.
pub(super) async fn stringify(
    cx: &Cx,
    value: Value,
    replacer: Value,
    space: Value,
) -> R<Option<String>> {
    let mut allow = None;
    let mut replacer_fn = None;
    if replacer.is_callable() {
        replacer_fn = Some(replacer);
    } else if let Some(items) = replacer.as_obj().and_then(Obj::array_items) {
        let mut names = Vec::new();
        for item in items {
            match item {
                Value::Str(s) => names.push(s),
                Value::Num(n) => names.push(number_to_string(n).into()),
                _ => {}
            }
        }
        allow = Some(names);
    }
    let gap = match &space {
        Value::Num(n) => " ".repeat(n.clamp(0.0, 10.0) as usize),
        Value::Str(s) => s.chars().take(10).collect(),
        _ => String::new(),
    };
    let root = cx.new_object();
    root.set_data("", value.clone());
    let mut walk = Walk {
        cx,
        mode: Mode::Program,
        replacer: replacer_fn,
        allow,
        stack: Vec::new(),
    };
    let json = walk.walk(Value::Obj(root), "".into(), value).await?;
    let Some(json) = json else {
        return Ok(None);
    };
    let text = write_json(&json, &gap);
    cx.check_string_len(text.len())?;
    Ok(Some(text))
}

/// `JSON.parse` reviver walk.
pub(super) fn revive<'a>(
    cx: &'a Cx,
    holder: Value,
    key: JsStr,
    reviver: &'a Value,
) -> LocalBoxFuture<'a, R<Value>> {
    Box::pin(async move {
        let value = cx.get(&holder, &PropKey::Str(key.clone())).await?;
        if let Value::Obj(obj) = &value {
            let keys = cx.own_keys(obj, true, false);
            for k in keys {
                if let PropKey::Str(name) = k {
                    let revived = revive(cx, value.clone(), name.clone(), reviver).await?;
                    if matches!(revived, Value::Undefined) {
                        cx.delete(obj, &PropKey::Str(name))?;
                    } else {
                        cx.define(obj, PropKey::Str(name), revived)?;
                    }
                }
            }
        }
        cx.call(reviver, holder, vec![Value::Str(key), value]).await
    })
}

/// Human-readable rendering used by `console.*`, following the donor's console format.
pub(super) fn console_format(cx: &Cx, value: &Value) -> String {
    match value {
        Value::Undefined => "undefined".into(),
        Value::Str(s) => s.to_string(),
        other => console_value(cx, other, &mut Vec::new(), 0),
    }
}

fn console_value(cx: &Cx, value: &Value, seen: &mut Vec<Obj>, depth: usize) -> String {
    let obj = match value {
        Value::Undefined | Value::Null => return "null".into(),
        Value::Str(s) => return Json::String(s.to_string()).to_string(),
        Value::Num(n) => return number_to_string(*n),
        Value::Bool(b) => return b.to_string(),
        Value::Sym(s) => {
            return format!(
                "Symbol({})",
                s.0.description.as_ref().map(|d| d.as_str()).unwrap_or("")
            );
        }
        Value::Obj(obj) => obj.clone(),
    };
    {
        let data = obj.borrow();
        match &data.kind {
            Kind::Promise(_) => return "[Promise (await it to get its value)]".into(),
            Kind::Date(_) | Kind::RegExp(_) | Kind::Url(_) | Kind::SearchParams { .. } => {
                drop(data);
                return cx.to_str(value).map(|s| s.to_string()).unwrap_or_default();
            }
            Kind::Headers(list) => {
                let mut map = Map::new();
                for (k, v) in list {
                    map.insert(k.clone(), Json::String(v.clone()));
                }
                return format!("Headers {}", Json::Object(map));
            }
            Kind::Bytes(bytes) => {
                let joined: Vec<String> = bytes.iter().map(u8::to_string).collect();
                return format!("Uint8Array({}) [{}]", bytes.len(), joined.join(","));
            }
            Kind::Function(Func::Tool(_)) => return "[opaque reference]".into(),
            Kind::Function(_) => {
                let name = data
                    .props
                    .get(&PropKey::from("name"))
                    .and_then(|p| match &p.slot {
                        super::value::Slot::Data(Value::Str(s)) => Some(s.to_string()),
                        _ => None,
                    });
                return match name {
                    Some(name) if !name.is_empty() => format!("[Function: {name}]"),
                    _ => "[Function (anonymous)]".into(),
                };
            }
            _ => {}
        }
    }
    if depth > 32 {
        return "...".into();
    }
    if seen.iter().any(|o| o.ptr_eq(&obj)) {
        return "[Circular]".into();
    }
    seen.push(obj.clone());
    let out = {
        let kind_snapshot = match &obj.borrow().kind {
            Kind::Map(map) => Some(Ok(map.values().cloned().collect::<Vec<_>>())),
            Kind::Set(set) => Some(Err(set.values().cloned().collect::<Vec<_>>())),
            _ => None,
        };
        match kind_snapshot {
            Some(Ok(entries)) => {
                let items: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "[{},{}]",
                            console_value(cx, k, seen, depth + 1),
                            console_value(cx, v, seen, depth + 1)
                        )
                    })
                    .collect();
                format!("Map({}) [{}]", entries.len(), items.join(","))
            }
            Some(Err(values)) => {
                let items: Vec<String> = values
                    .iter()
                    .map(|v| console_value(cx, v, seen, depth + 1))
                    .collect();
                format!("Set({}) [{}]", values.len(), items.join(","))
            }
            None => {
                if let Some(items) = obj.array_items() {
                    let parts: Vec<String> = items
                        .iter()
                        .map(|v| console_value(cx, v, seen, depth + 1))
                        .collect();
                    format!("[{}]", parts.join(","))
                } else if obj.is_error() {
                    cx.to_str(value).map(|s| s.to_string()).unwrap_or_default()
                } else {
                    let mut parts = Vec::new();
                    for key in cx.own_keys(&obj, true, false) {
                        if let PropKey::Str(name) = &key {
                            let item = cx.get_data(value, name).unwrap_or(Value::Undefined);
                            parts.push(format!(
                                "{}:{}",
                                Json::String(name.to_string()),
                                console_value(cx, &item, seen, depth + 1)
                            ));
                        }
                    }
                    format!("{{{}}}", parts.join(","))
                }
            }
        }
    };
    seen.pop();
    out
}
