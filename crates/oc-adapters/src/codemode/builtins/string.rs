//! `String`, `Number` and `Math`.

use std::cell::Cell;

use super::super::interp::{Cx, NativeImpl, R, arg};
use super::super::value::{
    IterKind, IterState, JsStr, Kind, Obj, Value, number_to_radix, number_to_string,
    relative_index, to_int32, to_integer, to_uint32,
};
use super::{int_arg, num_arg, regexp, str_arg};

pub(super) fn this_str(cx: &Cx, this: &Value) -> R<JsStr> {
    match this {
        Value::Str(s) => Ok(s.clone()),
        Value::Undefined | Value::Null => {
            Err(cx.type_error("String.prototype method called on null or undefined"))
        }
        Value::Obj(obj) => {
            if let Kind::Boxed(Value::Str(s)) = &obj.borrow().kind {
                return Ok(s.clone());
            }
            cx.to_str(this)
        }
        other => cx.to_str(other),
    }
}

/// Finds `needle` in `hay` at or after UTF-16 index `from`, returning a UTF-16 index.
pub(super) fn find16(hay: &JsStr, needle: &JsStr, from: usize) -> Option<usize> {
    if hay.is_ascii() && needle.is_ascii() {
        if from > hay.len() {
            return None;
        }
        return hay[from..].find(needle.as_str()).map(|i| i + from);
    }
    let h = hay.units();
    let n = needle.units();
    if n.is_empty() {
        return (from <= h.len()).then_some(from);
    }
    if n.len() > h.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| &h[i..i + n.len()] == n)
}

fn rfind16(hay: &JsStr, needle: &JsStr, from: usize) -> Option<usize> {
    let h = hay.units();
    let n = needle.units();
    if n.len() > h.len() {
        return None;
    }
    let start = from.min(h.len() - n.len());
    (0..=start).rev().find(|&i| &h[i..i + n.len()] == n)
}

fn is_js_whitespace(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// Expands `$&`, `` $` ``, `$'`, `$$` and `$n` in a replacement template.
pub(super) fn expand_replacement(
    template: &str,
    matched: &str,
    before: &str,
    after: &str,
    groups: &[Option<String>],
    named: &[(String, Option<String>)],
) -> String {
    let mut out = String::new();
    let mut chars = template.char_indices().peekable();
    let bytes = template.as_bytes();
    while let Some((i, c)) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek().map(|(_, n)| *n) {
            Some('$') => {
                chars.next();
                out.push('$');
            }
            Some('&') => {
                chars.next();
                out.push_str(matched);
            }
            Some('`') => {
                chars.next();
                out.push_str(before);
            }
            Some('\'') => {
                chars.next();
                out.push_str(after);
            }
            Some('<') if !named.is_empty() => {
                let rest = &template[i + 2..];
                if let Some(end) = rest.find('>') {
                    let name = &rest[..end];
                    if let Some((_, value)) = named.iter().find(|(n, _)| n == name) {
                        out.push_str(value.as_deref().unwrap_or(""));
                    }
                    for _ in 0..=end {
                        chars.next();
                    }
                } else {
                    out.push('$');
                }
            }
            Some(d) if d.is_ascii_digit() => {
                let first = d.to_digit(10).unwrap_or(0) as usize;
                let second = bytes
                    .get(i + 2)
                    .filter(|b| b.is_ascii_digit())
                    .map(|b| (b - b'0') as usize);
                let (index, consumed) = match second {
                    Some(s) if first * 10 + s >= 1 && first * 10 + s <= groups.len() => {
                        (first * 10 + s, 2)
                    }
                    _ => (first, 1),
                };
                if index >= 1 && index <= groups.len() {
                    for _ in 0..consumed {
                        chars.next();
                    }
                    out.push_str(groups[index - 1].as_deref().unwrap_or(""));
                } else {
                    out.push('$');
                }
            }
            _ => out.push('$'),
        }
    }
    out
}

async fn replace_string(
    cx: &Cx,
    s: JsStr,
    pattern: JsStr,
    replacement: Value,
    all: bool,
) -> R<Value> {
    let mut positions = Vec::new();
    let mut from = 0;
    while let Some(pos) = find16(&s, &pattern, from) {
        positions.push(pos);
        if !all {
            break;
        }
        from = pos + pattern.len16().max(1);
        if from > s.len16() {
            break;
        }
    }
    if positions.is_empty() {
        return Ok(Value::Str(s));
    }
    let mut out = String::new();
    let mut last = 0;
    for pos in positions {
        out.push_str(&s.slice16(last, pos));
        let replaced = if replacement.is_callable() {
            let r = cx
                .call(
                    &replacement,
                    Value::Undefined,
                    vec![
                        Value::Str(pattern.clone()),
                        Value::Num(pos as f64),
                        Value::Str(s.clone()),
                    ],
                )
                .await?;
            cx.to_string_async(&r).await?.to_string()
        } else {
            let template = cx.to_str(&replacement)?;
            let before = s.slice16(0, pos);
            let after = s.slice16(pos + pattern.len16(), s.len16());
            expand_replacement(&template, &pattern, &before, &after, &[], &[])
        };
        out.push_str(&replaced);
        cx.check_string_len(out.len())?;
        last = pos + pattern.len16();
    }
    out.push_str(&s.slice16(last, s.len16()));
    cx.string(out)
}

fn split_string(cx: &Cx, s: &JsStr, sep: &Value, limit: usize) -> R<Value> {
    let mut out = Vec::new();
    match sep {
        Value::Undefined => out.push(Value::Str(s.clone())),
        _ => {
            let sep = cx.to_str(sep)?;
            if sep.is_empty() {
                for i in 0..s.len16() {
                    if out.len() >= limit {
                        break;
                    }
                    out.push(Value::Str(s.slice16(i, i + 1)));
                }
            } else {
                let mut from = 0;
                while out.len() < limit {
                    match find16(s, &sep, from) {
                        Some(pos) => {
                            out.push(Value::Str(s.slice16(from, pos)));
                            from = pos + sep.len16();
                        }
                        None => {
                            out.push(Value::Str(s.slice16(from, s.len16())));
                            break;
                        }
                    }
                    cx.check_array_len(out.len())?;
                }
            }
        }
    }
    out.truncate(limit);
    Ok(cx.array_value(out))
}

fn pad(cx: &Cx, s: &JsStr, args: &[Value], at_start: bool) -> R<Value> {
    let target = int_arg(cx, args, 0, 0.0)?;
    let filler = match arg(args, 1) {
        Value::Undefined => " ".into(),
        other => cx.to_str(&other)?,
    };
    let len = s.len16();
    if target <= len as f64 || filler.is_empty() {
        return Ok(Value::Str(s.clone()));
    }
    let needed = target as usize - len;
    cx.check_string_len(s.len() + needed * 4)?;
    let units = filler.units().to_vec();
    let fill: Vec<u16> = units.iter().cycle().take(needed).copied().collect();
    let fill = String::from_utf16_lossy(&fill);
    Ok(Value::str(if at_start {
        format!("{fill}{s}")
    } else {
        format!("{s}{fill}")
    }))
}

fn case_map(s: &str, upper: bool) -> String {
    if upper {
        s.to_uppercase()
    } else {
        s.to_lowercase()
    }
}

fn install_string(cx: &Cx) {
    let proto = cx.protos.string.clone();
    proto.borrow_mut().kind = Kind::Boxed(Value::str(""));
    let call = NativeImpl::Async(Box::new(|cx, _, args| {
        Box::pin(async move {
            match args.first() {
                None => Ok(Value::str("")),
                Some(Value::Sym(sym)) => Ok(Value::str(format!(
                    "Symbol({})",
                    sym.0.description.as_ref().map(|d| d.as_str()).unwrap_or("")
                ))),
                Some(value) => Ok(Value::Str(cx.to_string_async(value).await?)),
            }
        })
    }));
    let construct = NativeImpl::Async(Box::new(|cx, nt, args| {
        Box::pin(async move {
            let s = match args.first() {
                None => JsStr::from(""),
                Some(value) => cx.to_string_async(value).await?,
            };
            Ok(Value::Obj(Obj::new(
                Some(cx.proto_from(&nt, &cx.protos.string)),
                Kind::Boxed(Value::Str(s)),
            )))
        })
    }));
    let ctor = cx.constructor("String", 1, &proto, Some(call), Some(construct));
    cx.method(&ctor, "fromCharCode", 1, |cx, _, args| {
        let mut units = Vec::with_capacity(args.len());
        for value in args {
            units.push(to_uint32(cx.to_num(value)?) as u16);
        }
        Ok(Value::str(String::from_utf16_lossy(&units)))
    });
    cx.method(&ctor, "fromCodePoint", 1, |cx, _, args| {
        let mut out = String::new();
        for value in args {
            let n = cx.to_num(value)?;
            let c = (n.fract() == 0.0 && n >= 0.0)
                .then(|| char::from_u32(n as u32))
                .flatten()
                .ok_or_else(|| {
                    cx.range_error(format!("Invalid code point {}", number_to_string(n)))
                })?;
            out.push(c);
        }
        Ok(Value::str(out))
    });
    cx.method_async(&ctor, "raw", 1, |cx, _, args| {
        Box::pin(async move {
            let strings = arg(&args, 0);
            let raw = cx.get_str(&strings, "raw").await?;
            let parts = cx.iterate_to_vec(&raw).await?;
            let mut out = String::new();
            for (index, part) in parts.iter().enumerate() {
                out.push_str(&cx.to_str(part)?);
                if index + 1 < parts.len()
                    && let Some(sub) = args.get(index + 1)
                {
                    out.push_str(&cx.to_string_async(sub).await?);
                }
            }
            cx.string(out)
        })
    });

    cx.method(&proto, "toString", 0, |cx, this, _| {
        Ok(Value::Str(this_str(cx, this)?))
    });
    cx.method(&proto, "valueOf", 0, |cx, this, _| {
        Ok(Value::Str(this_str(cx, this)?))
    });
    cx.method(&proto, "at", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let n = to_integer(num_arg(cx, args, 0)?);
        let len = s.len16() as f64;
        let index = if n < 0.0 { len + n } else { n };
        if index < 0.0 || index >= len {
            return Ok(Value::Undefined);
        }
        Ok(Value::Str(s.slice16(index as usize, index as usize + 1)))
    });
    cx.method(&proto, "charAt", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let index = int_arg(cx, args, 0, 0.0)?;
        if index < 0.0 || index >= s.len16() as f64 {
            return Ok(Value::str(""));
        }
        Ok(Value::Str(s.slice16(index as usize, index as usize + 1)))
    });
    cx.method(&proto, "charCodeAt", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let index = int_arg(cx, args, 0, 0.0)?;
        if index < 0.0 {
            return Ok(Value::Num(f64::NAN));
        }
        Ok(Value::Num(
            s.unit_at(index as usize).map_or(f64::NAN, f64::from),
        ))
    });
    cx.method(&proto, "codePointAt", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let index = int_arg(cx, args, 0, 0.0)?;
        if index < 0.0 {
            return Ok(Value::Undefined);
        }
        let index = index as usize;
        let Some(first) = s.unit_at(index) else {
            return Ok(Value::Undefined);
        };
        if (0xd800..0xdc00).contains(&first)
            && let Some(second) = s.unit_at(index + 1)
            && (0xdc00..0xe000).contains(&second)
        {
            let cp = 0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00);
            return Ok(Value::Num(f64::from(cp)));
        }
        Ok(Value::Num(f64::from(first)))
    });
    cx.method(&proto, "concat", 1, |cx, this, args| {
        let mut out = this_str(cx, this)?.to_string();
        for value in args {
            out.push_str(&cx.to_str(value)?);
            cx.check_string_len(out.len())?;
        }
        Ok(Value::str(out))
    });
    cx.method(&proto, "includes", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        if let Value::Obj(o) = arg(args, 0)
            && matches!(o.borrow().kind, Kind::RegExp(_))
        {
            return Err(cx.type_error(
                "First argument to String.prototype.includes must not be a regular expression",
            ));
        }
        let needle = str_arg(cx, args, 0)?;
        let from = int_arg(cx, args, 1, 0.0)?.max(0.0) as usize;
        Ok(Value::Bool(
            find16(&s, &needle, from.min(s.len16())).is_some(),
        ))
    });
    cx.method(&proto, "startsWith", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let needle = str_arg(cx, args, 0)?;
        let pos = (int_arg(cx, args, 1, 0.0)?.max(0.0) as usize).min(s.len16());
        let end = pos + needle.len16();
        Ok(Value::Bool(
            end <= s.len16() && s.slice16(pos, end) == needle,
        ))
    });
    cx.method(&proto, "endsWith", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let needle = str_arg(cx, args, 0)?;
        let end = (int_arg(cx, args, 1, s.len16() as f64)?.max(0.0) as usize).min(s.len16());
        Ok(Value::Bool(
            needle.len16() <= end && s.slice16(end - needle.len16(), end) == needle,
        ))
    });
    cx.method(&proto, "indexOf", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let needle = str_arg(cx, args, 0)?;
        let from = (int_arg(cx, args, 1, 0.0)?.max(0.0) as usize).min(s.len16());
        Ok(Value::Num(
            find16(&s, &needle, from).map_or(-1.0, |i| i as f64),
        ))
    });
    cx.method(&proto, "lastIndexOf", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let needle = str_arg(cx, args, 0)?;
        let from = match arg(args, 1) {
            Value::Undefined => s.len16(),
            other => {
                let n = cx.to_num(&other)?;
                if n.is_nan() {
                    s.len16()
                } else {
                    to_integer(n).max(0.0) as usize
                }
            }
        };
        Ok(Value::Num(
            rfind16(&s, &needle, from).map_or(-1.0, |i| i as f64),
        ))
    });
    cx.method(&proto, "slice", 2, |cx, this, args| {
        let s = this_str(cx, this)?;
        let len = s.len16();
        let start = relative_index(int_arg(cx, args, 0, 0.0)?, len);
        let end = relative_index(int_arg(cx, args, 1, len as f64)?, len);
        Ok(Value::Str(if start < end {
            s.slice16(start, end)
        } else {
            "".into()
        }))
    });
    cx.method(&proto, "substring", 2, |cx, this, args| {
        let s = this_str(cx, this)?;
        let len = s.len16() as f64;
        let a = int_arg(cx, args, 0, 0.0)?.clamp(0.0, len) as usize;
        let b = int_arg(cx, args, 1, len)?.clamp(0.0, len) as usize;
        Ok(Value::Str(s.slice16(a.min(b), a.max(b))))
    });
    cx.method(&proto, "substr", 2, |cx, this, args| {
        let s = this_str(cx, this)?;
        let len = s.len16();
        let start = relative_index(int_arg(cx, args, 0, 0.0)?, len);
        let count = int_arg(cx, args, 1, len as f64)?.clamp(0.0, (len - start) as f64) as usize;
        Ok(Value::Str(s.slice16(start, start + count)))
    });
    for (name, upper) in [
        ("toLowerCase", false),
        ("toUpperCase", true),
        ("toLocaleLowerCase", false),
        ("toLocaleUpperCase", true),
    ] {
        cx.method(&proto, name, 0, move |cx, this, _| {
            Ok(Value::str(case_map(&this_str(cx, this)?, upper)))
        });
    }
    cx.method(&proto, "trim", 0, |cx, this, _| {
        Ok(Value::str(
            this_str(cx, this)?.trim_matches(is_js_whitespace),
        ))
    });
    for name in ["trimStart", "trimLeft"] {
        cx.method(&proto, name, 0, |cx, this, _| {
            Ok(Value::str(
                this_str(cx, this)?.trim_start_matches(is_js_whitespace),
            ))
        });
    }
    for name in ["trimEnd", "trimRight"] {
        cx.method(&proto, name, 0, |cx, this, _| {
            Ok(Value::str(
                this_str(cx, this)?.trim_end_matches(is_js_whitespace),
            ))
        });
    }
    cx.method(&proto, "padStart", 2, |cx, this, args| {
        pad(cx, &this_str(cx, this)?, args, true)
    });
    cx.method(&proto, "padEnd", 2, |cx, this, args| {
        pad(cx, &this_str(cx, this)?, args, false)
    });
    cx.method(&proto, "repeat", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let count = int_arg(cx, args, 0, 0.0)?;
        if count < 0.0 || count.is_infinite() {
            return Err(cx.range_error(format!("Invalid count value: {}", number_to_string(count))));
        }
        cx.check_string_len(s.len().saturating_mul(count as usize))?;
        Ok(Value::str(s.repeat(count as usize)))
    });
    cx.method(&proto, "localeCompare", 1, |cx, this, args| {
        let a = this_str(cx, this)?;
        let b = str_arg(cx, args, 0)?;
        let key = |s: &str| s.to_lowercase();
        let ordering = key(&a).cmp(&key(&b)).then_with(|| a.cmp(&b));
        Ok(Value::Num(match ordering {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        }))
    });
    cx.method(&proto, "normalize", 0, |cx, this, args| {
        let form = match arg(args, 0) {
            Value::Undefined => "NFC".into(),
            other => cx.to_str(&other)?,
        };
        if !matches!(form.as_str(), "NFC" | "NFD" | "NFKC" | "NFKD") {
            return Err(
                cx.range_error("The normalization form should be one of NFC, NFD, NFKC, NFKD.")
            );
        }
        // Unicode normalization tables are not bundled; text is returned unchanged.
        Ok(Value::Str(this_str(cx, this)?))
    });
    cx.method(&proto, "isWellFormed", 0, |cx, this, _| {
        let _ = this_str(cx, this)?;
        Ok(Value::Bool(true))
    });
    cx.method(&proto, "toWellFormed", 0, |cx, this, _| {
        Ok(Value::Str(this_str(cx, this)?))
    });
    cx.method_async(&proto, "split", 2, |cx, this, args| {
        Box::pin(async move {
            let s = this_str(&cx, &this)?;
            let limit = match arg(&args, 1) {
                Value::Undefined => usize::MAX,
                other => to_uint32(cx.to_num(&other)?) as usize,
            };
            let sep = arg(&args, 0);
            if let Some(re) = regexp::as_regexp(&sep) {
                return regexp::split(&cx, &re, &s, limit);
            }
            split_string(&cx, &s, &sep, limit)
        })
    });
    for (name, all) in [("replace", false), ("replaceAll", true)] {
        cx.method_async(&proto, name, 2, move |cx, this, args| {
            Box::pin(async move {
                let s = this_str(&cx, &this)?;
                let pattern = arg(&args, 0);
                let replacement = arg(&args, 1);
                if let Some(re) = regexp::as_regexp(&pattern) {
                    if all && !re.data().flags.contains('g') {
                        return Err(cx.type_error("replaceAll must be called with a global RegExp"));
                    }
                    return regexp::replace(&cx, &re, s, replacement).await;
                }
                let pattern = cx.to_str(&pattern)?;
                replace_string(&cx, s, pattern, replacement, all).await
            })
        });
    }
    cx.method(&proto, "match", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let re = regexp::coerce(cx, &arg(args, 0), false)?;
        regexp::match_string(cx, &re, &s)
    });
    cx.method(&proto, "matchAll", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let pattern = arg(args, 0);
        if let Some(re) = regexp::as_regexp(&pattern)
            && !re.data().flags.contains('g')
        {
            return Err(
                cx.type_error("String.prototype.matchAll called with a non-global RegExp argument")
            );
        }
        let re = regexp::coerce(cx, &pattern, true)?;
        regexp::match_all(cx, &re, &s)
    });
    cx.method(&proto, "search", 1, |cx, this, args| {
        let s = this_str(cx, this)?;
        let re = regexp::coerce(cx, &arg(args, 0), false)?;
        regexp::search(cx, &re, &s)
    });
    cx.method_sym(
        &proto,
        &cx.sym_iterator.clone(),
        "[Symbol.iterator]",
        |cx, this, _| {
            let s = this_str(cx, this)?;
            let chars: Vec<Value> = s.chars().map(|c| Value::str(c.to_string())).collect();
            Ok(Value::Obj(Obj::new(
                Some(cx.protos.iterator.clone()),
                Kind::Iter(IterState {
                    kind: IterKind::List(chars),
                    index: 0,
                    done: false,
                }),
            )))
        },
    );
    cx.define_global("String", Value::Obj(ctor));
}

fn this_num(cx: &Cx, this: &Value) -> R<f64> {
    match this {
        Value::Num(n) => Ok(*n),
        Value::Obj(obj) => match &obj.borrow().kind {
            Kind::Boxed(Value::Num(n)) => Ok(*n),
            _ => Err(cx.type_error("Number.prototype method called on incompatible receiver")),
        },
        _ => Err(cx.type_error("Number.prototype method called on incompatible receiver")),
    }
}

fn to_fixed(x: f64, digits: usize) -> String {
    let scale = 10f64.powi(digits as i32);
    let scaled = x * scale;
    if (scaled.fract().abs() - 0.5).abs() < f64::EPSILON * scaled.abs().max(1.0)
        && scaled.abs() < 1e15
    {
        let rounded = scaled.trunc() + scaled.signum();
        return format!("{:.*}", digits, rounded / scale);
    }
    format!("{x:.digits$}")
}

fn exponential(x: f64, fraction: usize) -> String {
    let text = format!("{x:.fraction$e}");
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    format!(
        "{mantissa}e{}{}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

fn group_thousands(int: &str) -> String {
    let (sign, digits) = match int.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", int),
    };
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

fn install_number(cx: &Cx) {
    let proto = cx.protos.number.clone();
    proto.borrow_mut().kind = Kind::Boxed(Value::Num(0.0));
    let call = NativeImpl::Async(Box::new(|cx, _, args| {
        Box::pin(async move {
            match args.first() {
                None => Ok(Value::Num(0.0)),
                Some(value) => Ok(Value::Num(cx.to_number_async(value).await?)),
            }
        })
    }));
    let construct = NativeImpl::Async(Box::new(|cx, nt, args| {
        Box::pin(async move {
            let n = match args.first() {
                None => 0.0,
                Some(value) => cx.to_number_async(value).await?,
            };
            Ok(Value::Obj(Obj::new(
                Some(cx.proto_from(&nt, &cx.protos.number)),
                Kind::Boxed(Value::Num(n)),
            )))
        })
    }));
    let ctor = cx.constructor("Number", 1, &proto, Some(call), Some(construct));
    for (name, value) in [
        ("EPSILON", f64::EPSILON),
        ("MAX_SAFE_INTEGER", 9_007_199_254_740_991.0),
        ("MIN_SAFE_INTEGER", -9_007_199_254_740_991.0),
        ("MAX_VALUE", f64::MAX),
        ("MIN_VALUE", 5e-324),
        ("POSITIVE_INFINITY", f64::INFINITY),
        ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
        ("NaN", f64::NAN),
    ] {
        ctor.set_hidden(name, Value::Num(value));
    }
    let is_num = |v: &Value| match v {
        Value::Num(n) => Some(*n),
        _ => None,
    };
    cx.method(&ctor, "isFinite", 1, move |_, _, args| {
        Ok(Value::Bool(
            is_num(&arg(args, 0)).is_some_and(f64::is_finite),
        ))
    });
    cx.method(&ctor, "isNaN", 1, move |_, _, args| {
        Ok(Value::Bool(is_num(&arg(args, 0)).is_some_and(f64::is_nan)))
    });
    cx.method(&ctor, "isInteger", 1, move |_, _, args| {
        Ok(Value::Bool(
            is_num(&arg(args, 0)).is_some_and(|n| n.is_finite() && n.fract() == 0.0),
        ))
    });
    cx.method(&ctor, "isSafeInteger", 1, move |_, _, args| {
        Ok(Value::Bool(is_num(&arg(args, 0)).is_some_and(|n| {
            n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0
        })))
    });
    for name in ["parseFloat", "parseInt"] {
        if let Some(func) = cx.builtins_lookup(name) {
            ctor.set_hidden(name, func);
        }
    }
    cx.method(&proto, "toString", 1, |cx, this, args| {
        let n = this_num(cx, this)?;
        let radix = int_arg(cx, args, 0, 10.0)?;
        if !(2.0..=36.0).contains(&radix) {
            return Err(cx.range_error("toString() radix must be between 2 and 36"));
        }
        Ok(Value::str(number_to_radix(n, radix as u32)))
    });
    cx.method(&proto, "toLocaleString", 0, |cx, this, _| {
        let n = this_num(cx, this)?;
        if !n.is_finite() {
            return Ok(Value::str(number_to_string(n)));
        }
        let fixed = to_fixed(n, 3);
        let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
        let frac = frac.trim_end_matches('0');
        let int = group_thousands(int);
        Ok(Value::str(if frac.is_empty() {
            int
        } else {
            format!("{int}.{frac}")
        }))
    });
    cx.method(&proto, "valueOf", 0, |cx, this, _| {
        Ok(Value::Num(this_num(cx, this)?))
    });
    cx.method(&proto, "toFixed", 1, |cx, this, args| {
        let n = this_num(cx, this)?;
        let digits = int_arg(cx, args, 0, 0.0)?;
        if !(0.0..=100.0).contains(&digits) {
            return Err(cx.range_error("toFixed() digits argument must be between 0 and 100"));
        }
        if !n.is_finite() || n.abs() >= 1e21 {
            return Ok(Value::str(number_to_string(n)));
        }
        Ok(Value::str(to_fixed(n, digits as usize)))
    });
    cx.method(&proto, "toExponential", 1, |cx, this, args| {
        let n = this_num(cx, this)?;
        if !n.is_finite() {
            return Ok(Value::str(number_to_string(n)));
        }
        let text = match arg(args, 0) {
            Value::Undefined => {
                let shortest = format!("{n:e}");
                let (mantissa, exp) = shortest.split_once('e').unwrap_or((&shortest, "0"));
                let exp: i32 = exp.parse().unwrap_or(0);
                format!(
                    "{mantissa}e{}{}",
                    if exp < 0 { '-' } else { '+' },
                    exp.abs()
                )
            }
            other => {
                let digits = to_integer(cx.to_num(&other)?);
                if !(0.0..=100.0).contains(&digits) {
                    return Err(
                        cx.range_error("toExponential() argument must be between 0 and 100")
                    );
                }
                exponential(n, digits as usize)
            }
        };
        Ok(Value::str(text))
    });
    cx.method(&proto, "toPrecision", 1, |cx, this, args| {
        let n = this_num(cx, this)?;
        let precision = match arg(args, 0) {
            Value::Undefined => return Ok(Value::str(number_to_string(n))),
            other => to_integer(cx.to_num(&other)?),
        };
        if !n.is_finite() {
            return Ok(Value::str(number_to_string(n)));
        }
        if !(1.0..=100.0).contains(&precision) {
            return Err(cx.range_error("toPrecision() argument must be between 1 and 100"));
        }
        let p = precision as usize;
        if n == 0.0 {
            return Ok(Value::str(if p == 1 {
                "0".to_string()
            } else {
                format!("0.{}", "0".repeat(p - 1))
            }));
        }
        let sci = format!("{:.*e}", p - 1, n);
        let exp: i32 = sci
            .split_once('e')
            .map(|(_, e)| e.parse().unwrap_or(0))
            .unwrap_or(0);
        if exp < -6 || exp >= p as i32 {
            return Ok(Value::str(exponential(n, p - 1)));
        }
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        Ok(Value::str(format!("{n:.decimals$}")))
    });
    cx.define_global("Number", Value::Obj(ctor));
}

thread_local! {
    static RNG: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn random_bytes(buf: &mut [u8]) {
    use std::io::Read;
    if let Ok(mut file) = std::fs::File::open("/dev/urandom")
        && file.read_exact(buf).is_ok()
    {
        return;
    }
    for b in buf.iter_mut() {
        *b = (next_random() >> 24) as u8;
    }
}

fn next_random() -> u64 {
    RNG.with(|state| {
        let mut x = state.get();
        if x == 0 {
            let mut seed = [0u8; 8];
            if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
                use std::io::Read;
                let _ = file.read_exact(&mut seed);
            }
            x = u64::from_le_bytes(seed) | 1;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        x
    })
}

fn install_math(cx: &Cx) {
    let math = cx.new_object();
    for (name, value) in [
        ("E", std::f64::consts::E),
        ("LN10", std::f64::consts::LN_10),
        ("LN2", std::f64::consts::LN_2),
        ("LOG10E", std::f64::consts::LOG10_E),
        ("LOG2E", std::f64::consts::LOG2_E),
        ("PI", std::f64::consts::PI),
        ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
        ("SQRT2", std::f64::consts::SQRT_2),
    ] {
        math.set_hidden(name, Value::Num(value));
    }
    type UnaryMath = fn(f64) -> f64;
    let unary: &[(&str, UnaryMath)] = &[
        ("abs", f64::abs),
        ("acos", f64::acos),
        ("acosh", f64::acosh),
        ("asin", f64::asin),
        ("asinh", f64::asinh),
        ("atan", f64::atan),
        ("atanh", f64::atanh),
        ("cbrt", f64::cbrt),
        ("ceil", f64::ceil),
        ("cos", f64::cos),
        ("cosh", f64::cosh),
        ("exp", f64::exp),
        ("expm1", f64::exp_m1),
        ("floor", f64::floor),
        ("fround", |x| f64::from(x as f32)),
        ("log", f64::ln),
        ("log1p", f64::ln_1p),
        ("log10", f64::log10),
        ("log2", f64::log2),
        ("sin", f64::sin),
        ("sinh", f64::sinh),
        ("sqrt", f64::sqrt),
        ("tan", f64::tan),
        ("tanh", f64::tanh),
        ("trunc", f64::trunc),
        ("sign", |x| {
            if x.is_nan() || x == 0.0 {
                x
            } else {
                x.signum()
            }
        }),
        ("round", |x| {
            if x.is_nan() || x.is_infinite() || x.fract() == 0.0 {
                x
            } else {
                (x + 0.5).floor()
            }
        }),
        ("clz32", |x| f64::from(to_uint32(x).leading_zeros())),
    ];
    for (name, f) in unary {
        let f = *f;
        cx.method(&math, name, 1, move |cx, _, args| {
            Ok(Value::Num(f(num_arg(cx, args, 0)?)))
        });
    }
    cx.method(&math, "atan2", 2, |cx, _, args| {
        Ok(Value::Num(
            num_arg(cx, args, 0)?.atan2(num_arg(cx, args, 1)?),
        ))
    });
    cx.method(&math, "pow", 2, |cx, _, args| {
        let (x, y) = (num_arg(cx, args, 0)?, num_arg(cx, args, 1)?);
        if y.is_nan() || ((x == 1.0 || x == -1.0) && y.is_infinite()) {
            return Ok(Value::Num(f64::NAN));
        }
        Ok(Value::Num(x.powf(y)))
    });
    cx.method(&math, "imul", 2, |cx, _, args| {
        Ok(Value::Num(f64::from(
            to_int32(num_arg(cx, args, 0)?).wrapping_mul(to_int32(num_arg(cx, args, 1)?)),
        )))
    });
    cx.method(&math, "max", 2, |cx, _, args| {
        let mut out = f64::NEG_INFINITY;
        for value in args {
            let n = cx.to_num(value)?;
            if n.is_nan() {
                return Ok(Value::Num(f64::NAN));
            }
            if n > out || (n == 0.0 && out == 0.0 && out.is_sign_negative()) {
                out = n;
            }
        }
        Ok(Value::Num(out))
    });
    cx.method(&math, "min", 2, |cx, _, args| {
        let mut out = f64::INFINITY;
        for value in args {
            let n = cx.to_num(value)?;
            if n.is_nan() {
                return Ok(Value::Num(f64::NAN));
            }
            if n < out || (n == 0.0 && out == 0.0 && n.is_sign_negative()) {
                out = n;
            }
        }
        Ok(Value::Num(out))
    });
    cx.method(&math, "hypot", 2, |cx, _, args| {
        let mut sum = 0.0;
        for value in args {
            let n = cx.to_num(value)?;
            if n.is_infinite() {
                return Ok(Value::Num(f64::INFINITY));
            }
            sum += n * n;
        }
        Ok(Value::Num(sum.sqrt()))
    });
    cx.method(&math, "random", 0, |_, _, _| {
        Ok(Value::Num(
            (next_random() >> 11) as f64 / (1u64 << 53) as f64,
        ))
    });
    math.borrow_mut().props.insert(
        super::super::value::PropKey::Sym(cx.sym_to_string_tag.clone()),
        super::super::value::Prop::hidden(Value::str("Math")),
    );
    cx.define_global("Math", Value::Obj(math));
}

pub(super) fn install(cx: &Cx) {
    install_string(cx);
    install_number(cx);
    install_math(cx);
}
