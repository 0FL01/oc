//! Standard library installed into every Code Mode realm.

pub(super) mod array;
pub(super) mod collections;
pub(super) mod date;
pub(super) mod object;
pub(super) mod regexp;
pub(super) mod string;
pub(super) mod web;

use std::cell::RefCell;

use super::interp::{Cx, ErrorKind, R, arg};
use super::json;
use super::value::{Func, JsStr, Kind, Obj, PropKey, Slot, Value};

pub(super) fn install(cx: &Cx) {
    install_globals(cx);
    object::install(cx);
    array::install(cx);
    string::install(cx);
    regexp::install(cx);
    collections::install(cx);
    date::install(cx);
    web::install(cx);
    super::promise::install(cx);
    super::generator::install(cx);
    super::host::install(cx);
    install_json(cx);
    install_console(cx);
}

thread_local! {
    static JOINING: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// `ToString` for objects without calling user code.
pub(super) fn object_to_string_builtin(cx: &Cx, obj: &Obj) -> R<JsStr> {
    enum Shape {
        Array(Vec<Value>),
        Error,
        Date(f64),
        RegExp(String),
        Function(String),
        Url(String),
        Params(String),
        Boxed(Value),
        Bytes(Vec<u8>),
        Plain,
    }
    let shape = {
        let data = obj.borrow();
        match &data.kind {
            Kind::Array(items) => Shape::Array(items.clone()),
            Kind::Error => Shape::Error,
            Kind::Date(t) => Shape::Date(*t),
            Kind::RegExp(re) => Shape::RegExp(format!("/{}/{}", re.source, re.flags)),
            Kind::Function(func) => {
                let name = match data.props.get(&PropKey::from("name")).map(|p| &p.slot) {
                    Some(Slot::Data(Value::Str(s))) => s.to_string(),
                    _ => String::new(),
                };
                Shape::Function(match func {
                    Func::User(user) if user.is_class_constructor => format!("class {name} {{ }}"),
                    Func::User(_) => format!("function {name}() {{ [code] }}"),
                    _ => format!("function {name}() {{ [native code] }}"),
                })
            }
            Kind::Url(url) => Shape::Url(url.href.clone()),
            Kind::SearchParams { list, .. } => Shape::Params(web::serialize_params(list)),
            Kind::Boxed(value) => Shape::Boxed(value.clone()),
            Kind::Bytes(bytes) => Shape::Bytes(bytes.clone()),
            _ => Shape::Plain,
        }
    };
    Ok(match shape {
        Shape::Array(items) => {
            let id = obj.id();
            if JOINING.with(|j| j.borrow().contains(&id)) {
                return Ok("".into());
            }
            JOINING.with(|j| j.borrow_mut().push(id));
            let result = (|| {
                let mut out = String::new();
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    if !item.is_nullish() {
                        out.push_str(&cx.to_str(item)?);
                    }
                    cx.check_string_len(out.len())?;
                }
                Ok(out)
            })();
            JOINING.with(|j| j.borrow_mut().pop());
            result?.into()
        }
        Shape::Error => {
            let base = Value::Obj(obj.clone());
            let name = cx.get_data(&base, "name")?;
            let message = cx.get_data(&base, "message")?;
            let name = if name.is_nullish() {
                "Error".into()
            } else {
                cx.to_str(&name)?
            };
            let message = if message.is_nullish() {
                "".into()
            } else {
                cx.to_str(&message)?
            };
            if message.is_empty() {
                name
            } else if name.is_empty() {
                message
            } else {
                format!("{name}: {message}").into()
            }
        }
        Shape::Date(t) => date::to_display_string(t).into(),
        Shape::RegExp(text) | Shape::Function(text) | Shape::Url(text) | Shape::Params(text) => {
            text.into()
        }
        Shape::Boxed(value) => cx.to_str(&value)?,
        Shape::Bytes(bytes) => bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
            .into(),
        Shape::Plain => {
            let tag = cx
                .lookup(
                    &Value::Obj(obj.clone()),
                    &PropKey::Sym(cx.sym_to_string_tag.clone()),
                )?
                .and_then(|slot| match slot {
                    Slot::Data(Value::Str(s)) => Some(s),
                    _ => None,
                });
            match tag {
                Some(tag) => format!("[object {tag}]").into(),
                None => "[object Object]".into(),
            }
        }
    })
}

pub(super) fn object_to_primitive_builtin(cx: &Cx, obj: &Obj, prefer_number: bool) -> R<Value> {
    {
        let data = obj.borrow();
        match &data.kind {
            Kind::Date(t) if prefer_number => return Ok(Value::Num(*t)),
            Kind::Boxed(value) => return Ok(value.clone()),
            _ => {}
        }
    }
    Ok(Value::Str(object_to_string_builtin(cx, obj)?))
}

pub(super) fn num_arg(cx: &Cx, args: &[Value], index: usize) -> R<f64> {
    cx.to_num(&arg(args, index))
}

pub(super) fn str_arg(cx: &Cx, args: &[Value], index: usize) -> R<JsStr> {
    cx.to_str(&arg(args, index))
}

/// Optional integer argument; `undefined` yields `default`.
pub(super) fn int_arg(cx: &Cx, args: &[Value], index: usize, default: f64) -> R<f64> {
    match args.get(index) {
        None | Some(Value::Undefined) => Ok(default),
        Some(value) => Ok(super::value::to_integer(cx.to_num(value)?)),
    }
}

fn parse_int(text: &str, radix: Option<u32>) -> f64 {
    let s = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut radix = radix.unwrap_or(0);
    let mut s = s;
    if (radix == 0 || radix == 16)
        && let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
    {
        s = rest;
        radix = 16;
    }
    if radix == 0 {
        radix = 10;
    }
    if !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    let digits: Vec<u32> = s.chars().map_while(|c| c.to_digit(radix)).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let value = if radix == 10 {
        let text: String = s.chars().take(digits.len()).collect();
        text.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        digits
            .iter()
            .fold(0.0, |acc, d| acc * f64::from(radix) + f64::from(*d))
    };
    if negative { -value } else { value }
}

pub(super) fn parse_float(text: &str) -> f64 {
    let s = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    for prefix in ["Infinity", "+Infinity"] {
        if s.starts_with(prefix) {
            return f64::INFINITY;
        }
    }
    if s.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let bytes = s.as_bytes();
    let mut end = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut last_valid = 0;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_digit() {
            seen_digit = true;
            end += 1;
            last_valid = end;
        } else if c == b'.' && !seen_dot && !seen_exp {
            seen_dot = true;
            end += 1;
            if seen_digit {
                last_valid = end;
            }
        } else if (c == b'e' || c == b'E') && seen_digit && !seen_exp {
            seen_exp = true;
            end += 1;
            if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                end += 1;
            }
        } else {
            break;
        }
    }
    if !seen_digit {
        return f64::NAN;
    }
    let mut candidate = &s[..last_valid.max(1)];
    loop {
        if let Ok(value) = candidate.parse::<f64>() {
            return value;
        }
        if candidate.is_empty() {
            return f64::NAN;
        }
        candidate = &candidate[..candidate.len() - 1];
    }
}

const URI_RESERVED: &str = ";/?:@&=+$,#";
const URI_UNESCAPED_MARKS: &str = "-_.!~*'()";

fn uri_encode(cx: &Cx, text: &str, keep: &str) -> R<Value> {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || URI_UNESCAPED_MARKS.contains(c) || keep.contains(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    cx.string(out)
}

fn uri_decode(cx: &Cx, text: &str, keep: &str) -> R<Value> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            let Some(b) = hex else {
                return Err(cx.throw(ErrorKind::Uri, "URI malformed"));
            };
            if b < 0x80 && keep.contains(b as char) {
                out.extend_from_slice(&bytes[i..i + 3]);
            } else {
                out.push(b);
            }
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    match String::from_utf8(out) {
        Ok(text) => cx.string(text),
        Err(_) => Err(cx.throw(ErrorKind::Uri, "URI malformed")),
    }
}

fn install_globals(cx: &Cx) {
    cx.define_global("undefined", Value::Undefined);
    cx.define_global("NaN", Value::Num(f64::NAN));
    cx.define_global("Infinity", Value::Num(f64::INFINITY));
    let global = |name: &str, len: usize, f: fn(&Cx, &Value, &[Value]) -> R<Value>| {
        let func = cx.native(name, len, f);
        cx.define_global(name, Value::Obj(func));
    };
    global("parseInt", 2, |cx, _, args| {
        let text = str_arg(cx, args, 0)?;
        let radix = match arg(args, 1) {
            Value::Undefined => None,
            other => Some(super::value::to_int32(cx.to_num(&other)?) as u32),
        };
        Ok(Value::Num(parse_int(&text, radix)))
    });
    global("parseFloat", 1, |cx, _, args| {
        Ok(Value::Num(parse_float(&str_arg(cx, args, 0)?)))
    });
    global("isNaN", 1, |cx, _, args| {
        Ok(Value::Bool(num_arg(cx, args, 0)?.is_nan()))
    });
    global("isFinite", 1, |cx, _, args| {
        Ok(Value::Bool(num_arg(cx, args, 0)?.is_finite()))
    });
    global("encodeURIComponent", 1, |cx, _, args| {
        uri_encode(cx, &str_arg(cx, args, 0)?, "")
    });
    global("encodeURI", 1, |cx, _, args| {
        uri_encode(cx, &str_arg(cx, args, 0)?, URI_RESERVED)
    });
    global("decodeURIComponent", 1, |cx, _, args| {
        uri_decode(cx, &str_arg(cx, args, 0)?, "")
    });
    global("decodeURI", 1, |cx, _, args| {
        uri_decode(cx, &str_arg(cx, args, 0)?, URI_RESERVED)
    });
    let clone = cx.native_async("structuredClone", 1, |cx, _, args| {
        Box::pin(async move {
            let json = json::to_host(&cx, &arg(&args, 0)).await?;
            Ok(json::from_json(&cx, &json))
        })
    });
    cx.define_global("structuredClone", Value::Obj(clone));
    for name in [
        "setTimeout",
        "setInterval",
        "setImmediate",
        "clearTimeout",
        "clearInterval",
        "require",
        "eval",
        "Function",
        "process",
        "Deno",
        "Bun",
    ] {
        let message: &'static str = match name {
            "setTimeout" | "setInterval" | "setImmediate" | "clearTimeout" | "clearInterval" => {
                "Timers are not available in Code Mode"
            }
            "require" => "Modules are not available in Code Mode",
            "eval" | "Function" => "Dynamic code evaluation is not available in Code Mode",
            _ => "Process and runtime APIs are not available in Code Mode",
        };
        let func = cx.native(name, 0, move |cx, _, _| {
            Err(cx.throw(ErrorKind::Reference, message))
        });
        cx.define_global(name, Value::Obj(func));
    }
}

fn install_json(cx: &Cx) {
    let json_obj = cx.new_object();
    cx.method_async(&json_obj, "stringify", 3, |cx, _, args| {
        Box::pin(async move {
            let text = json::stringify(&cx, arg(&args, 0), arg(&args, 1), arg(&args, 2)).await?;
            Ok(text.map(Value::str).unwrap_or(Value::Undefined))
        })
    });
    cx.method_async(&json_obj, "parse", 2, |cx, _, args| {
        Box::pin(async move {
            let text = cx.to_str(&arg(&args, 0))?;
            let value = json::parse(&cx, &text)?;
            let reviver = arg(&args, 1);
            if reviver.is_callable() {
                let root = cx.new_object();
                root.set_data("", value);
                return json::revive(&cx, Value::Obj(root), "".into(), &reviver).await;
            }
            Ok(value)
        })
    });
    json_obj.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_to_string_tag.clone()),
        super::value::Prop::hidden(Value::str("JSON")),
    );
    cx.define_global("JSON", Value::Obj(json_obj));
}

fn push_log(cx: &Cx, line: String) {
    let mut logs = cx.logs.borrow_mut();
    if logs.bytes + line.len() > cx.limits.max_log_bytes {
        logs.dropped += 1;
        return;
    }
    logs.bytes += line.len() + 1;
    logs.lines.push(line);
}

fn table(cx: &Cx, data: &Value, columns: &Value) -> String {
    let column_filter: Option<Vec<String>> =
        columns.as_obj().and_then(Obj::array_items).map(|items| {
            items
                .iter()
                .map(|v| cx.to_str(v).map(|s| s.to_string()).unwrap_or_default())
                .collect()
        });
    let rows: Vec<(String, Value)> = match data {
        Value::Obj(obj) => {
            if let Some(items) = obj.array_items() {
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v))
                    .collect()
            } else {
                cx.own_keys(obj, true, false)
                    .into_iter()
                    .filter_map(|k| {
                        let name = k.as_str()?.clone();
                        let value = cx.get_data(data, &name).ok()?;
                        Some((name.to_string(), value))
                    })
                    .collect()
            }
        }
        Value::Undefined => return "undefined".into(),
        other => vec![("0".to_string(), other.clone())],
    };
    let row_values = |value: &Value| -> Vec<(String, Value)> {
        match value {
            Value::Obj(obj) if !obj.is_array() => cx
                .own_keys(obj, true, false)
                .into_iter()
                .filter_map(|k| {
                    let name = k.as_str()?.clone();
                    Some((name.to_string(), cx.get_data(value, &name).ok()?))
                })
                .collect(),
            other => vec![("Value".to_string(), other.clone())],
        }
    };
    let mut keys: Vec<String> = Vec::new();
    let expanded: Vec<(String, Vec<(String, Value)>)> = rows
        .iter()
        .map(|(i, v)| (i.clone(), row_values(v)))
        .collect();
    match &column_filter {
        Some(columns) => keys = columns.clone(),
        None => {
            for (_, values) in &expanded {
                for (k, _) in values {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        }
    }
    let mut lines = vec![
        std::iter::once("(index)".to_string())
            .chain(keys.iter().cloned())
            .collect::<Vec<_>>()
            .join("\t"),
    ];
    for (index, values) in expanded {
        let mut cells = vec![index];
        for key in &keys {
            let cell = values.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            cells.push(match cell {
                None | Some(Value::Undefined) => String::new(),
                Some(Value::Str(s)) => s.to_string(),
                Some(other) => json::console_format(cx, other),
            });
        }
        lines.push(cells.join("\t"));
    }
    lines.join("\n")
}

fn install_console(cx: &Cx) {
    let console = cx.new_object();
    for name in ["log", "info", "debug", "warn", "error", "trace"] {
        let prefix = match name {
            "warn" => "[warn] ",
            "error" => "[error] ",
            "debug" => "[debug] ",
            _ => "",
        };
        cx.method(&console, name, 0, move |cx, _, args| {
            let parts: Vec<String> = args.iter().map(|a| json::console_format(cx, a)).collect();
            push_log(cx, format!("{prefix}{}", parts.join(" ")));
            Ok(Value::Undefined)
        });
    }
    cx.method(&console, "dir", 0, |cx, _, args| {
        let line = match args.first() {
            None => "undefined".to_string(),
            Some(value) => json::console_format(cx, value),
        };
        push_log(cx, line);
        Ok(Value::Undefined)
    });
    cx.method(&console, "table", 1, |cx, _, args| {
        let line = table(cx, &arg(args, 0), &arg(args, 1));
        push_log(cx, line);
        Ok(Value::Undefined)
    });
    cx.define_global("console", Value::Obj(console));
}
