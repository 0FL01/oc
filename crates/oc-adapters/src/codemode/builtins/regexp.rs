//! `RegExp` over the `regress` ECMAScript engine, matching on UTF-16 code units.
//!
//! As in the donor, a single pathological match cannot be interrupted mid-search.

use std::rc::Rc;

use super::super::interp::{Cx, ErrorKind, NativeImpl, R, arg};
use super::super::value::{
    IterKind, IterState, JsStr, Kind, Obj, Prop, PropKey, Slot, Value, to_integer,
};
use super::string::expand_replacement;

pub(in super::super) struct RegExpData {
    pub source: String,
    pub flags: String,
    regex: regress::Regex,
}

#[derive(Clone)]
pub(super) struct ReObj {
    obj: Obj,
    data: Rc<RegExpData>,
}

impl ReObj {
    pub(super) fn data(&self) -> &RegExpData {
        &self.data
    }
}

struct Found {
    start: usize,
    end: usize,
    groups: Vec<Option<(usize, usize)>>,
    named: Vec<(String, Option<(usize, usize)>)>,
}

fn compile(cx: &Cx, pattern: &str, flags: &str) -> R<Rc<RegExpData>> {
    let mut seen = String::new();
    for c in flags.chars() {
        if !"dgimsuyv".contains(c) || seen.contains(c) {
            return Err(cx.throw(
                ErrorKind::Syntax,
                format!("Invalid regular expression flags '{flags}'"),
            ));
        }
        seen.push(c);
    }
    let engine_flags: String = flags.chars().filter(|c| "imsuv".contains(*c)).collect();
    match regress::Regex::with_flags(pattern, engine_flags.as_str()) {
        Ok(regex) => {
            let mut ordered: Vec<char> = flags.chars().collect();
            ordered.sort_by_key(|c| "dgimsuvy".find(*c).unwrap_or(9));
            Ok(Rc::new(RegExpData {
                source: if pattern.is_empty() {
                    "(?:)".into()
                } else {
                    pattern.to_string()
                },
                flags: ordered.into_iter().collect(),
                regex,
            }))
        }
        Err(err) => Err(cx.throw(
            ErrorKind::Syntax,
            format!("Invalid regular expression: /{pattern}/{flags}: {err}"),
        )),
    }
}

fn wrap(cx: &Cx, data: Rc<RegExpData>, proto: Obj) -> Value {
    let obj = Obj::new(Some(proto), Kind::RegExp(data));
    obj.borrow_mut().props.insert(
        "lastIndex".into(),
        Prop {
            slot: Slot::Data(Value::Num(0.0)),
            enumerable: false,
            writable: true,
            configurable: false,
        },
    );
    let _ = cx;
    Value::Obj(obj)
}

pub(in super::super) fn create(cx: &Cx, pattern: &str, flags: &str) -> R<Value> {
    let data = compile(cx, pattern, flags)?;
    Ok(wrap(cx, data, cx.protos.regexp.clone()))
}

pub(super) fn as_regexp(value: &Value) -> Option<ReObj> {
    let obj = value.as_obj()?;
    let data = match &obj.borrow().kind {
        Kind::RegExp(data) => data.clone(),
        _ => return None,
    };
    Some(ReObj {
        obj: obj.clone(),
        data,
    })
}

/// The argument of `match`/`matchAll`/`search` as a RegExp.
pub(super) fn coerce(cx: &Cx, value: &Value, global: bool) -> R<ReObj> {
    if let Some(re) = as_regexp(value) {
        return Ok(re);
    }
    let pattern = match value {
        Value::Undefined => JsStr::from(""),
        other => cx.to_str(other)?,
    };
    let created = create(cx, &pattern, if global { "g" } else { "" })?;
    as_regexp(&created).ok_or_else(|| cx.type_error("RegExp creation failed"))
}

fn find(data: &RegExpData, s: &JsStr, start: usize) -> Option<Found> {
    let units = s.units();
    if start > units.len() {
        return None;
    }
    let unicode = data.flags.contains('u') || data.flags.contains('v');
    let m = if unicode {
        data.regex.find_from_utf16(units, start).next()
    } else {
        data.regex.find_from_ucs2(units, start).next()
    }?;
    if data.flags.contains('y') && m.start() != start {
        return None;
    }
    let groups = m
        .captures
        .iter()
        .map(|g| g.as_ref().map(|r| (r.start, r.end)))
        .collect();
    let named = m
        .named_groups()
        .map(|(name, range)| (name.to_string(), range.map(|r| (r.start, r.end))))
        .collect();
    Some(Found {
        start: m.start(),
        end: m.end(),
        groups,
        named,
    })
}

fn last_index(cx: &Cx, re: &ReObj) -> R<usize> {
    let value = re.obj.own_data("lastIndex").unwrap_or(Value::Num(0.0));
    Ok(to_integer(cx.to_num(&value)?).max(0.0) as usize)
}

fn set_last_index(re: &ReObj, index: usize) {
    if let Some(prop) = re
        .obj
        .borrow_mut()
        .props
        .get_mut(&PropKey::from("lastIndex"))
    {
        prop.slot = Slot::Data(Value::Num(index as f64));
    }
}

fn match_object(cx: &Cx, s: &JsStr, found: &Found) -> Value {
    let mut items = vec![Value::Str(s.slice16(found.start, found.end))];
    for group in &found.groups {
        items.push(match group {
            Some((a, b)) => Value::Str(s.slice16(*a, *b)),
            None => Value::Undefined,
        });
    }
    let array = cx.new_array(items);
    array.set_data("index", Value::Num(found.start as f64));
    array.set_data("input", Value::Str(s.clone()));
    let groups = if found.named.is_empty() {
        Value::Undefined
    } else {
        let obj = Obj::new(None, Kind::Plain);
        for (name, range) in &found.named {
            let value = match range {
                Some((a, b)) => Value::Str(s.slice16(*a, *b)),
                None => Value::Undefined,
            };
            obj.set_data(PropKey::Str(name.as_str().into()), value);
        }
        Value::Obj(obj)
    };
    array.set_data("groups", groups);
    Value::Obj(array)
}

fn exec(cx: &Cx, re: &ReObj, s: &JsStr) -> R<Value> {
    let stateful = re.data.flags.contains('g') || re.data.flags.contains('y');
    let start = if stateful { last_index(cx, re)? } else { 0 };
    if start > s.len16() {
        set_last_index(re, 0);
        return Ok(Value::Null);
    }
    match find(&re.data, s, start) {
        Some(found) => {
            if stateful {
                set_last_index(re, found.end);
            }
            Ok(match_object(cx, s, &found))
        }
        None => {
            if stateful {
                set_last_index(re, 0);
            }
            Ok(Value::Null)
        }
    }
}

fn advance(s: &JsStr, index: usize, unicode: bool) -> usize {
    if unicode
        && let Some(unit) = s.unit_at(index)
        && (0xd800..0xdc00).contains(&unit)
        && s.unit_at(index + 1)
            .is_some_and(|u| (0xdc00..0xe000).contains(&u))
    {
        return index + 2;
    }
    index + 1
}

fn all_matches(cx: &Cx, re: &ReObj, s: &JsStr) -> R<Vec<Found>> {
    let unicode = re.data.flags.contains('u') || re.data.flags.contains('v');
    let mut out = Vec::new();
    let mut index = 0;
    while index <= s.len16() {
        cx.tick()?;
        let Some(found) = find(&re.data, s, index) else {
            break;
        };
        index = if found.end == found.start {
            advance(s, found.end, unicode)
        } else {
            found.end
        };
        out.push(found);
        cx.check_array_len(out.len())?;
    }
    Ok(out)
}

pub(super) fn match_string(cx: &Cx, re: &ReObj, s: &JsStr) -> R<Value> {
    if !re.data.flags.contains('g') {
        return exec(cx, re, s);
    }
    set_last_index(re, 0);
    let matches = all_matches(cx, re, s)?;
    if matches.is_empty() {
        return Ok(Value::Null);
    }
    Ok(cx.array_value(
        matches
            .iter()
            .map(|m| Value::Str(s.slice16(m.start, m.end)))
            .collect(),
    ))
}

pub(super) fn match_all(cx: &Cx, re: &ReObj, s: &JsStr) -> R<Value> {
    let matches = all_matches(cx, re, s)?;
    let items = matches.iter().map(|m| match_object(cx, s, m)).collect();
    Ok(Value::Obj(Obj::new(
        Some(cx.protos.iterator.clone()),
        Kind::Iter(IterState {
            kind: IterKind::List(items),
            index: 0,
            done: false,
        }),
    )))
}

pub(super) fn search(cx: &Cx, re: &ReObj, s: &JsStr) -> R<Value> {
    let _ = cx;
    Ok(Value::Num(
        find(&re.data, s, 0).map_or(-1.0, |m| m.start as f64),
    ))
}

pub(super) async fn replace(cx: &Cx, re: &ReObj, s: JsStr, replacement: Value) -> R<Value> {
    let global = re.data.flags.contains('g');
    let matches = if global {
        set_last_index(re, 0);
        all_matches(cx, re, &s)?
    } else {
        let start = if re.data.flags.contains('y') {
            last_index(cx, re)?
        } else {
            0
        };
        match find(&re.data, &s, start) {
            Some(found) => {
                if re.data.flags.contains('y') {
                    set_last_index(re, found.end);
                }
                vec![found]
            }
            None => {
                if re.data.flags.contains('y') {
                    set_last_index(re, 0);
                }
                Vec::new()
            }
        }
    };
    let template = if replacement.is_callable() {
        None
    } else {
        Some(cx.to_str(&replacement)?)
    };
    let mut out = String::new();
    let mut last = 0;
    for found in matches {
        cx.tick()?;
        out.push_str(&s.slice16(last, found.start));
        let matched = s.slice16(found.start, found.end);
        let groups: Vec<Option<String>> = found
            .groups
            .iter()
            .map(|g| g.map(|(a, b)| s.slice16(a, b).to_string()))
            .collect();
        let replaced = match &template {
            Some(template) => {
                let named: Vec<(String, Option<String>)> = found
                    .named
                    .iter()
                    .map(|(n, r)| (n.clone(), r.map(|(a, b)| s.slice16(a, b).to_string())))
                    .collect();
                expand_replacement(
                    template,
                    &matched,
                    &s.slice16(0, found.start),
                    &s.slice16(found.end, s.len16()),
                    &groups,
                    &named,
                )
            }
            None => {
                let mut args = vec![Value::Str(matched.clone())];
                args.extend(groups.iter().map(|g| {
                    g.as_ref()
                        .map(|t| Value::str(t.as_str()))
                        .unwrap_or(Value::Undefined)
                }));
                args.push(Value::Num(found.start as f64));
                args.push(Value::Str(s.clone()));
                if !found.named.is_empty() {
                    let obj = cx.new_object();
                    for (name, range) in &found.named {
                        let value = range
                            .map(|(a, b)| Value::Str(s.slice16(a, b)))
                            .unwrap_or(Value::Undefined);
                        obj.set_data(PropKey::Str(name.as_str().into()), value);
                    }
                    args.push(Value::Obj(obj));
                }
                let result = cx.call(&replacement, Value::Undefined, args).await?;
                cx.to_string_async(&result).await?.to_string()
            }
        };
        out.push_str(&replaced);
        cx.check_string_len(out.len())?;
        last = found.end;
    }
    out.push_str(&s.slice16(last, s.len16()));
    cx.string(out)
}

pub(super) fn split(cx: &Cx, re: &ReObj, s: &JsStr, limit: usize) -> R<Value> {
    let mut out = Vec::new();
    if limit == 0 {
        return Ok(cx.array_value(out));
    }
    let len = s.len16();
    if len == 0 {
        if find(&re.data, s, 0).is_some_and(|m| m.end == 0) {
            return Ok(cx.array_value(out));
        }
        return Ok(cx.array_value(vec![Value::Str(s.clone())]));
    }
    let mut p = 0;
    let mut q = 0;
    while q < len {
        cx.tick()?;
        let Some(found) = find(&re.data, s, q) else {
            break;
        };
        if found.start >= len {
            break;
        }
        if found.end == p {
            // Empty match at the previous split point.
            q = found.start + 1;
            continue;
        }
        out.push(Value::Str(s.slice16(p, found.start)));
        if out.len() >= limit {
            return Ok(cx.array_value(out));
        }
        for group in &found.groups {
            out.push(
                group
                    .map(|(a, b)| Value::Str(s.slice16(a, b)))
                    .unwrap_or(Value::Undefined),
            );
            if out.len() >= limit {
                return Ok(cx.array_value(out));
            }
        }
        cx.check_array_len(out.len())?;
        p = found.end;
        q = p;
    }
    out.push(Value::Str(s.slice16(p, len)));
    Ok(cx.array_value(out))
}

fn this_re(cx: &Cx, this: &Value) -> R<ReObj> {
    as_regexp(this).ok_or_else(|| cx.type_error("RegExp method called on incompatible receiver"))
}

pub(super) fn install(cx: &Cx) {
    let proto = cx.protos.regexp.clone();
    let make = |cx: &Cx, nt: &Value, args: &[Value]| -> R<Value> {
        let pattern = arg(args, 0);
        let flags = arg(args, 1);
        let (source, inherited) = match as_regexp(&pattern) {
            Some(re) => (re.data.source.clone(), re.data.flags.clone()),
            None => (
                match pattern {
                    Value::Undefined => String::new(),
                    other => cx.to_str(&other)?.to_string(),
                },
                String::new(),
            ),
        };
        let flags = match flags {
            Value::Undefined => inherited,
            other => cx.to_str(&other)?.to_string(),
        };
        let source = if source == "(?:)" {
            String::new()
        } else {
            source
        };
        let data = compile(cx, &source, &flags)?;
        let proto = match nt {
            Value::Undefined => cx.protos.regexp.clone(),
            other => cx.proto_from(other, &cx.protos.regexp),
        };
        Ok(wrap(cx, data, proto))
    };
    let call = NativeImpl::Sync(Box::new(move |cx: &Cx, _: &Value, args: &[Value]| {
        if as_regexp(&arg(args, 0)).is_some() && matches!(arg(args, 1), Value::Undefined) {
            return Ok(arg(args, 0));
        }
        make(cx, &Value::Undefined, args)
    }));
    let construct = NativeImpl::Sync(Box::new(make));
    let ctor = cx.constructor("RegExp", 2, &proto, Some(call), Some(construct));
    cx.method(&ctor, "escape", 1, |cx, _, args| {
        let text = cx.to_str(&arg(args, 0))?;
        let mut out = String::new();
        for c in text.chars() {
            if "^$\\.*+?()[]{}|/-".contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        Ok(Value::str(out))
    });
    cx.method(&proto, "exec", 1, |cx, this, args| {
        let re = this_re(cx, this)?;
        let s = cx.to_str(&arg(args, 0))?;
        exec(cx, &re, &s)
    });
    cx.method(&proto, "test", 1, |cx, this, args| {
        let re = this_re(cx, this)?;
        let s = cx.to_str(&arg(args, 0))?;
        Ok(Value::Bool(!matches!(exec(cx, &re, &s)?, Value::Null)))
    });
    cx.method(&proto, "toString", 0, |cx, this, _| {
        let re = this_re(cx, this)?;
        Ok(Value::str(format!("/{}/{}", re.data.source, re.data.flags)))
    });
    cx.getter(&proto, "source", |cx, this, _| {
        Ok(Value::str(this_re(cx, this)?.data.source.as_str()))
    });
    cx.getter(&proto, "flags", |cx, this, _| {
        Ok(Value::str(this_re(cx, this)?.data.flags.as_str()))
    });
    for (name, flag) in [
        ("global", 'g'),
        ("ignoreCase", 'i'),
        ("multiline", 'm'),
        ("sticky", 'y'),
        ("unicode", 'u'),
        ("unicodeSets", 'v'),
        ("dotAll", 's'),
        ("hasIndices", 'd'),
    ] {
        cx.getter(&proto, name, move |cx, this, _| {
            Ok(Value::Bool(this_re(cx, this)?.data.flags.contains(flag)))
        });
    }
    cx.define_global("RegExp", Value::Obj(ctor));
}
