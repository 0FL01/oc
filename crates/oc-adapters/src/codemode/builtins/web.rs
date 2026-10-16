//! Web-like globals: `URL`, `URLSearchParams`, `Headers`, text encoding, base64, `Uint8Array`
//! and `crypto`.

use base64::Engine as _;
use reqwest::Url;

use super::super::interp::{Cx, ErrorKind, NativeImpl, R, arg};
use super::super::value::{
    IterKind, IterState, JsStr, Kind, Obj, Prop, PropKey, Slot, UrlData, Value, relative_index,
    to_integer,
};
use super::string::random_bytes;
use super::{int_arg, str_arg};

type NativeSetter = Box<dyn Fn(&Cx, &Value, &[Value]) -> R<Value>>;
type HeaderEdit<'a> = dyn FnMut(&mut Vec<(String, String)>) + 'a;

// ----- form encoding -------------------------------------------------------------------

fn form_encode(text: &str, out: &mut String) {
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
}

fn form_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                match text
                    .get(i + 1..i + 3)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(in super::super) fn serialize_params(list: &[(String, String)]) -> String {
    let mut out = String::new();
    for (index, (key, value)) in list.iter().enumerate() {
        if index > 0 {
            out.push('&');
        }
        form_encode(key, &mut out);
        out.push('=');
        form_encode(value, &mut out);
    }
    out
}

fn parse_params(query: &str) -> Vec<(String, String)> {
    let query = query.strip_prefix('?').unwrap_or(query);
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((k, v)) => (form_decode(k), form_decode(v)),
            None => (form_decode(part), String::new()),
        })
        .collect()
}

fn list_iterator(cx: &Cx, items: Vec<Value>) -> Value {
    Value::Obj(Obj::new(
        Some(cx.protos.iterator.clone()),
        Kind::Iter(IterState {
            kind: IterKind::List(items),
            index: 0,
            done: false,
        }),
    ))
}

fn accessor(
    cx: &Cx,
    target: &Obj,
    name: &str,
    get: impl Fn(&Cx, &Value, &[Value]) -> R<Value> + 'static,
    set: Option<NativeSetter>,
) {
    let getter = cx.native(&format!("get {name}"), 0, get);
    let setter = set.map(|f| cx.native(&format!("set {name}"), 1, f));
    target.borrow_mut().props.insert(
        name.into(),
        Prop {
            slot: Slot::Accessor {
                get: Some(getter),
                set: setter,
            },
            enumerable: false,
            writable: false,
            configurable: true,
        },
    );
}

// ----- URL -----------------------------------------------------------------------------

fn this_url(cx: &Cx, this: &Value) -> R<(Obj, Url)> {
    if let Value::Obj(obj) = this
        && let Kind::Url(data) = &obj.borrow().kind
    {
        let url = Url::parse(&data.href).map_err(|_| cx.type_error("Invalid URL"))?;
        return Ok((obj.clone(), url));
    }
    Err(cx.type_error("URL method called on incompatible receiver"))
}

fn store_url(obj: &Obj, url: &Url) {
    let params = match &mut obj.borrow_mut().kind {
        Kind::Url(data) => {
            data.href = url.as_str().to_string();
            data.search_params.clone()
        }
        _ => None,
    };
    if let Some(params) = params
        && let Kind::SearchParams { list, .. } = &mut params.borrow_mut().kind
    {
        *list = parse_params(url.query().unwrap_or(""));
    }
}

fn sync_url_from_params(params: &Obj) {
    let (url_obj, query) = match &params.borrow().kind {
        Kind::SearchParams {
            list,
            url: Some(url),
        } => (url.clone(), serialize_params(list)),
        _ => return,
    };
    if let Kind::Url(data) = &mut url_obj.borrow_mut().kind
        && let Ok(mut url) = Url::parse(&data.href)
    {
        url.set_query(if query.is_empty() { None } else { Some(&query) });
        data.href = url.as_str().to_string();
    }
}

fn new_params(cx: &Cx, list: Vec<(String, String)>, url: Option<Obj>) -> Obj {
    Obj::new(
        Some(cx.protos.search_params.clone()),
        Kind::SearchParams { list, url },
    )
}

fn install_url(cx: &Cx) {
    let proto = cx.protos.url.clone();
    let construct = NativeImpl::Sync(Box::new(|cx: &Cx, nt: &Value, args: &[Value]| {
        let input = str_arg(cx, args, 0)?;
        let parsed = match arg(args, 1) {
            Value::Undefined => Url::parse(&input),
            base => {
                let base = match &base {
                    Value::Obj(obj) => match &obj.borrow().kind {
                        Kind::Url(data) => data.href.clone(),
                        _ => cx.to_str(&base)?.to_string(),
                    },
                    other => cx.to_str(other)?.to_string(),
                };
                Url::parse(&base).and_then(|b| b.join(&input))
            }
        };
        let url = parsed.map_err(|_| cx.type_error(format!("Invalid URL: {input}")))?;
        let obj = Obj::new(
            Some(cx.proto_from(nt, &cx.protos.url)),
            Kind::Url(UrlData {
                href: url.as_str().to_string(),
                search_params: None,
            }),
        );
        let params = new_params(
            cx,
            parse_params(url.query().unwrap_or("")),
            Some(obj.clone()),
        );
        if let Kind::Url(data) = &mut obj.borrow_mut().kind {
            data.search_params = Some(params);
        }
        Ok(Value::Obj(obj))
    }));
    let ctor = cx.constructor("URL", 1, &proto, None, Some(construct));
    cx.method(&ctor, "canParse", 1, |cx, _, args| {
        let input = str_arg(cx, args, 0)?;
        let ok = match arg(args, 1) {
            Value::Undefined => Url::parse(&input).is_ok(),
            base => Url::parse(&cx.to_str(&base)?)
                .and_then(|b| b.join(&input))
                .is_ok(),
        };
        Ok(Value::Bool(ok))
    });
    type Get = fn(&Url) -> String;
    type Set = fn(&mut Url, &str);
    let fields: &[(&str, Get, Option<Set>)] = &[
        ("href", |u| u.as_str().to_string(), None),
        ("origin", |u| u.origin().ascii_serialization(), None),
        (
            "protocol",
            |u| format!("{}:", u.scheme()),
            Some(|u, v| {
                let _ = u.set_scheme(v.trim_end_matches(':'));
            }),
        ),
        (
            "username",
            |u| u.username().to_string(),
            Some(|u, v| {
                let _ = u.set_username(v);
            }),
        ),
        (
            "password",
            |u| u.password().unwrap_or("").to_string(),
            Some(|u, v| {
                let _ = u.set_password(if v.is_empty() { None } else { Some(v) });
            }),
        ),
        (
            "host",
            |u| match (u.host_str(), u.port()) {
                (Some(h), Some(p)) => format!("{h}:{p}"),
                (Some(h), None) => h.to_string(),
                _ => String::new(),
            },
            Some(|u, v| {
                let (host, port) = match v.rsplit_once(':') {
                    Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p.parse().ok()),
                    _ => (v, None),
                };
                if u.set_host(Some(host)).is_ok() {
                    let _ = u.set_port(port);
                }
            }),
        ),
        (
            "hostname",
            |u| u.host_str().unwrap_or("").to_string(),
            Some(|u, v| {
                let _ = u.set_host(Some(v));
            }),
        ),
        (
            "port",
            |u| u.port().map(|p| p.to_string()).unwrap_or_default(),
            Some(|u, v| {
                let _ = u.set_port(if v.is_empty() { None } else { v.parse().ok() });
            }),
        ),
        (
            "pathname",
            |u| u.path().to_string(),
            Some(|u, v| u.set_path(v)),
        ),
        (
            "search",
            |u| {
                u.query()
                    .filter(|q| !q.is_empty())
                    .map(|q| format!("?{q}"))
                    .unwrap_or_default()
            },
            Some(|u, v| {
                let v = v.strip_prefix('?').unwrap_or(v);
                u.set_query(if v.is_empty() { None } else { Some(v) });
            }),
        ),
        (
            "hash",
            |u| {
                u.fragment()
                    .filter(|f| !f.is_empty())
                    .map(|f| format!("#{f}"))
                    .unwrap_or_default()
            },
            Some(|u, v| {
                let v = v.strip_prefix('#').unwrap_or(v);
                u.set_fragment(if v.is_empty() { None } else { Some(v) });
            }),
        ),
    ];
    for (name, get, set) in fields {
        let get = *get;
        let setter: Option<NativeSetter> = if *name == "href" {
            Some(Box::new(|cx: &Cx, this: &Value, args: &[Value]| {
                let (obj, _) = this_url(cx, this)?;
                let text = str_arg(cx, args, 0)?;
                let url =
                    Url::parse(&text).map_err(|_| cx.type_error(format!("Invalid URL: {text}")))?;
                store_url(&obj, &url);
                Ok(Value::Undefined)
            }))
        } else {
            set.map(|set| {
                Box::new(move |cx: &Cx, this: &Value, args: &[Value]| {
                    let (obj, mut url) = this_url(cx, this)?;
                    let text = str_arg(cx, args, 0)?;
                    set(&mut url, &text);
                    store_url(&obj, &url);
                    Ok(Value::Undefined)
                }) as NativeSetter
            })
        };
        accessor(
            cx,
            &proto,
            name,
            move |cx, this, _| Ok(Value::str(get(&this_url(cx, this)?.1))),
            setter,
        );
    }
    accessor(
        cx,
        &proto,
        "searchParams",
        |cx, this, _| {
            let (obj, _) = this_url(cx, this)?;
            let params = match &obj.borrow().kind {
                Kind::Url(data) => data.search_params.clone(),
                _ => None,
            };
            Ok(params.map(Value::Obj).unwrap_or(Value::Undefined))
        },
        None,
    );
    for name in ["toString", "toJSON"] {
        cx.method(&proto, name, 0, |cx, this, _| {
            Ok(Value::str(this_url(cx, this)?.1.as_str()))
        });
    }
    cx.define_global("URL", Value::Obj(ctor));
}

fn this_params(cx: &Cx, this: &Value) -> R<Obj> {
    match this {
        Value::Obj(obj) if matches!(obj.borrow().kind, Kind::SearchParams { .. }) => {
            Ok(obj.clone())
        }
        _ => Err(cx.type_error("URLSearchParams method called on incompatible receiver")),
    }
}

fn params_list(obj: &Obj) -> Vec<(String, String)> {
    match &obj.borrow().kind {
        Kind::SearchParams { list, .. } => list.clone(),
        _ => Vec::new(),
    }
}

fn update_params(obj: &Obj, f: impl FnOnce(&mut Vec<(String, String)>)) {
    if let Kind::SearchParams { list, .. } = &mut obj.borrow_mut().kind {
        f(list);
    }
    sync_url_from_params(obj);
}

fn install_search_params(cx: &Cx) {
    let proto = cx.protos.search_params.clone();
    let construct = NativeImpl::Async(Box::new(|cx, nt, args| {
        Box::pin(async move {
            let init = arg(&args, 0);
            let list = match &init {
                Value::Undefined | Value::Null => Vec::new(),
                Value::Obj(obj) => {
                    if let Kind::SearchParams { list, .. } = &obj.borrow().kind {
                        list.clone()
                    } else if obj.is_array()
                        || cx.has_property(obj, &PropKey::Sym(cx.sym_iterator.clone()))
                    {
                        let mut list = Vec::new();
                        for pair in cx.iterate_to_vec(&init).await? {
                            let items = cx.iterate_to_vec(&pair).await?;
                            if items.len() != 2 {
                                return Err(cx.type_error(
                                    "Each query pair must be an iterable [name, value] tuple",
                                ));
                            }
                            list.push((
                                cx.to_str(&items[0])?.to_string(),
                                cx.to_str(&items[1])?.to_string(),
                            ));
                        }
                        list
                    } else {
                        let mut list = Vec::new();
                        for (key, value) in cx.own_entries(obj).await? {
                            list.push((
                                key.to_string(),
                                cx.to_string_async(&value).await?.to_string(),
                            ));
                        }
                        list
                    }
                }
                other => parse_params(&cx.to_str(other)?),
            };
            let obj = Obj::new(
                Some(cx.proto_from(&nt, &cx.protos.search_params)),
                Kind::SearchParams { list, url: None },
            );
            Ok(Value::Obj(obj))
        })
    }));
    let ctor = cx.constructor("URLSearchParams", 0, &proto, None, Some(construct));
    cx.method(&proto, "append", 2, |cx, this, args| {
        let obj = this_params(cx, this)?;
        let (k, v) = (
            str_arg(cx, args, 0)?.to_string(),
            str_arg(cx, args, 1)?.to_string(),
        );
        update_params(&obj, |list| list.push((k, v)));
        Ok(Value::Undefined)
    });
    cx.method(&proto, "delete", 1, |cx, this, args| {
        let obj = this_params(cx, this)?;
        let k = str_arg(cx, args, 0)?.to_string();
        let value = match arg(args, 1) {
            Value::Undefined => None,
            other => Some(cx.to_str(&other)?.to_string()),
        };
        update_params(&obj, |list| {
            list.retain(|(key, v)| key != &k || value.as_ref().is_some_and(|want| want != v))
        });
        Ok(Value::Undefined)
    });
    cx.method(&proto, "get", 1, |cx, this, args| {
        let obj = this_params(cx, this)?;
        let k = str_arg(cx, args, 0)?;
        Ok(params_list(&obj)
            .into_iter()
            .find(|(key, _)| key.as_str() == k.as_str())
            .map(|(_, v)| Value::str(v))
            .unwrap_or(Value::Null))
    });
    cx.method(&proto, "getAll", 1, |cx, this, args| {
        let obj = this_params(cx, this)?;
        let k = str_arg(cx, args, 0)?;
        let values = params_list(&obj)
            .into_iter()
            .filter(|(key, _)| key.as_str() == k.as_str())
            .map(|(_, v)| Value::str(v))
            .collect();
        Ok(cx.array_value(values))
    });
    cx.method(&proto, "has", 1, |cx, this, args| {
        let obj = this_params(cx, this)?;
        let k = str_arg(cx, args, 0)?;
        let value = match arg(args, 1) {
            Value::Undefined => None,
            other => Some(cx.to_str(&other)?.to_string()),
        };
        Ok(Value::Bool(params_list(&obj).iter().any(|(key, v)| {
            key.as_str() == k.as_str() && value.as_ref().is_none_or(|want| want == v)
        })))
    });
    cx.method(&proto, "set", 2, |cx, this, args| {
        let obj = this_params(cx, this)?;
        let (k, v) = (
            str_arg(cx, args, 0)?.to_string(),
            str_arg(cx, args, 1)?.to_string(),
        );
        update_params(&obj, |list| {
            match list.iter().position(|(key, _)| key == &k) {
                Some(first) => {
                    list[first].1 = v;
                    let mut index = 0;
                    list.retain(|(key, _)| {
                        let keep = key != &k || index == first;
                        index += 1;
                        keep
                    });
                }
                None => list.push((k, v)),
            }
        });
        Ok(Value::Undefined)
    });
    cx.method(&proto, "sort", 0, |cx, this, _| {
        let obj = this_params(cx, this)?;
        update_params(&obj, |list| {
            list.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()))
        });
        Ok(Value::Undefined)
    });
    cx.method(&proto, "toString", 0, |cx, this, _| {
        Ok(Value::str(serialize_params(&params_list(&this_params(
            cx, this,
        )?))))
    });
    cx.getter(&proto, "size", |cx, this, _| {
        Ok(Value::Num(params_list(&this_params(cx, this)?).len() as f64))
    });
    cx.method_async(&proto, "forEach", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_params(&cx, &this)?;
            let f = arg(&args, 0);
            for (k, v) in params_list(&obj) {
                cx.call(
                    &f,
                    arg(&args, 1),
                    vec![Value::str(v), Value::str(k), this.clone()],
                )
                .await?;
            }
            Ok(Value::Undefined)
        })
    });
    cx.method(&proto, "keys", 0, |cx, this, _| {
        let items = params_list(&this_params(cx, this)?)
            .into_iter()
            .map(|(k, _)| Value::str(k))
            .collect();
        Ok(list_iterator(cx, items))
    });
    cx.method(&proto, "values", 0, |cx, this, _| {
        let items = params_list(&this_params(cx, this)?)
            .into_iter()
            .map(|(_, v)| Value::str(v))
            .collect();
        Ok(list_iterator(cx, items))
    });
    let entries = cx.native("entries", 0, |cx, this, _| {
        let items = params_list(&this_params(cx, this)?)
            .into_iter()
            .map(|(k, v)| cx.array_value(vec![Value::str(k), Value::str(v)]))
            .collect();
        Ok(list_iterator(cx, items))
    });
    proto.set_hidden("entries", Value::Obj(entries.clone()));
    proto.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_iterator.clone()),
        Prop::hidden(Value::Obj(entries)),
    );
    cx.define_global("URLSearchParams", Value::Obj(ctor));
}

// ----- Headers -------------------------------------------------------------------------

fn this_headers(cx: &Cx, this: &Value) -> R<Obj> {
    match this {
        Value::Obj(obj) if matches!(obj.borrow().kind, Kind::Headers(_)) => Ok(obj.clone()),
        _ => Err(cx.type_error("Headers method called on incompatible receiver")),
    }
}

fn headers_list(obj: &Obj) -> Vec<(String, String)> {
    match &obj.borrow().kind {
        Kind::Headers(list) => list.clone(),
        _ => Vec::new(),
    }
}

fn header_get(list: &[(String, String)], name: &str) -> Option<String> {
    let values: Vec<&str> = list
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .collect();
    (!values.is_empty()).then(|| values.join(", "))
}

fn install_headers(cx: &Cx) {
    let proto = cx.protos.headers.clone();
    let construct = NativeImpl::Async(Box::new(|cx, nt, args| {
        Box::pin(async move {
            let init = arg(&args, 0);
            let mut list = Vec::new();
            match &init {
                Value::Undefined | Value::Null => {}
                Value::Obj(obj) => {
                    if let Kind::Headers(existing) = &obj.borrow().kind {
                        list = existing.clone();
                    } else if obj.is_array() {
                        for pair in cx.iterate_to_vec(&init).await? {
                            let items = cx.iterate_to_vec(&pair).await?;
                            if items.len() != 2 {
                                return Err(
                                    cx.type_error("Header entries must be [name, value] pairs")
                                );
                            }
                            list.push((
                                cx.to_str(&items[0])?.to_ascii_lowercase(),
                                cx.to_str(&items[1])?.to_string(),
                            ));
                        }
                    } else {
                        for (k, v) in cx.own_entries(obj).await? {
                            list.push((
                                k.to_ascii_lowercase(),
                                cx.to_string_async(&v).await?.to_string(),
                            ));
                        }
                    }
                }
                _ => return Err(cx.type_error("Invalid Headers init")),
            }
            Ok(Value::Obj(Obj::new(
                Some(cx.proto_from(&nt, &cx.protos.headers)),
                Kind::Headers(list),
            )))
        })
    }));
    let ctor = cx.constructor("Headers", 0, &proto, None, Some(construct));
    let update = |obj: &Obj, f: &mut HeaderEdit| {
        if let Kind::Headers(list) = &mut obj.borrow_mut().kind {
            f(list);
        }
    };
    cx.method(&proto, "get", 1, |cx, this, args| {
        let obj = this_headers(cx, this)?;
        let name = str_arg(cx, args, 0)?;
        Ok(header_get(&headers_list(&obj), &name)
            .map(Value::str)
            .unwrap_or(Value::Null))
    });
    cx.method(&proto, "has", 1, |cx, this, args| {
        let obj = this_headers(cx, this)?;
        let name = str_arg(cx, args, 0)?;
        Ok(Value::Bool(
            header_get(&headers_list(&obj), &name).is_some(),
        ))
    });
    cx.method(&proto, "set", 2, move |cx, this, args| {
        let obj = this_headers(cx, this)?;
        let (name, value) = (
            str_arg(cx, args, 0)?.to_ascii_lowercase(),
            str_arg(cx, args, 1)?.to_string(),
        );
        update(&obj, &mut |list| {
            list.retain(|(k, _)| k != &name);
            list.push((name.clone(), value.clone()));
        });
        Ok(Value::Undefined)
    });
    cx.method(&proto, "append", 2, move |cx, this, args| {
        let obj = this_headers(cx, this)?;
        let (name, value) = (
            str_arg(cx, args, 0)?.to_ascii_lowercase(),
            str_arg(cx, args, 1)?.to_string(),
        );
        update(&obj, &mut |list| list.push((name.clone(), value.clone())));
        Ok(Value::Undefined)
    });
    cx.method(&proto, "delete", 1, move |cx, this, args| {
        let obj = this_headers(cx, this)?;
        let name = str_arg(cx, args, 0)?.to_ascii_lowercase();
        update(&obj, &mut |list| list.retain(|(k, _)| k != &name));
        Ok(Value::Undefined)
    });
    fn grouped(list: Vec<(String, String)>) -> Vec<(String, String)> {
        let mut names: Vec<String> = list.iter().map(|(k, _)| k.clone()).collect();
        names.sort();
        names.dedup();
        names
            .into_iter()
            .map(|name| {
                let value = header_get(&list, &name).unwrap_or_default();
                (name, value)
            })
            .collect()
    }
    let entries = cx.native("entries", 0, |cx, this, _| {
        let items = grouped(headers_list(&this_headers(cx, this)?))
            .into_iter()
            .map(|(k, v)| cx.array_value(vec![Value::str(k), Value::str(v)]))
            .collect();
        Ok(list_iterator(cx, items))
    });
    proto.set_hidden("entries", Value::Obj(entries.clone()));
    proto.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_iterator.clone()),
        Prop::hidden(Value::Obj(entries)),
    );
    cx.method(&proto, "keys", 0, |cx, this, _| {
        let items = grouped(headers_list(&this_headers(cx, this)?))
            .into_iter()
            .map(|(k, _)| Value::str(k))
            .collect();
        Ok(list_iterator(cx, items))
    });
    cx.method(&proto, "values", 0, |cx, this, _| {
        let items = grouped(headers_list(&this_headers(cx, this)?))
            .into_iter()
            .map(|(_, v)| Value::str(v))
            .collect();
        Ok(list_iterator(cx, items))
    });
    cx.method_async(&proto, "forEach", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_headers(&cx, &this)?;
            let f = arg(&args, 0);
            for (k, v) in grouped(headers_list(&obj)) {
                cx.call(
                    &f,
                    arg(&args, 1),
                    vec![Value::str(v), Value::str(k), this.clone()],
                )
                .await?;
            }
            Ok(Value::Undefined)
        })
    });
    cx.define_global("Headers", Value::Obj(ctor));
}

// ----- bytes and encodings -------------------------------------------------------------

pub(in super::super) fn bytes_of(value: &Value) -> Option<Vec<u8>> {
    match &value.as_obj()?.borrow().kind {
        Kind::Bytes(bytes) => Some(bytes.clone()),
        _ => None,
    }
}

fn new_bytes(cx: &Cx, bytes: Vec<u8>) -> Value {
    Value::Obj(Obj::new(Some(cx.protos.bytes.clone()), Kind::Bytes(bytes)))
}

fn this_bytes(cx: &Cx, this: &Value) -> R<(Obj, Vec<u8>)> {
    match this {
        Value::Obj(obj) => match &obj.borrow().kind {
            Kind::Bytes(bytes) => Ok((obj.clone(), bytes.clone())),
            _ => Err(cx.type_error("Uint8Array method called on incompatible receiver")),
        },
        _ => Err(cx.type_error("Uint8Array method called on incompatible receiver")),
    }
}

fn byte_of(cx: &Cx, value: &Value) -> R<u8> {
    Ok(super::super::value::to_uint32(cx.to_num(value)?) as u8)
}

fn install_bytes(cx: &Cx) {
    let proto = cx.protos.bytes.clone();
    let construct = NativeImpl::Async(Box::new(|cx, nt, args| {
        Box::pin(async move {
            let source = arg(&args, 0);
            let bytes = match &source {
                Value::Undefined => Vec::new(),
                Value::Num(n) => {
                    if *n < 0.0 || n.fract() != 0.0 {
                        return Err(cx.range_error("Invalid typed array length"));
                    }
                    cx.check_array_len(*n as usize)?;
                    vec![0; *n as usize]
                }
                Value::Obj(_) => match bytes_of(&source) {
                    Some(bytes) => bytes,
                    None => {
                        let mut out = Vec::new();
                        for item in cx.iterate_to_vec(&source).await? {
                            out.push(byte_of(&cx, &item)?);
                        }
                        out
                    }
                },
                other => vec![0; to_integer(cx.to_num(other)?).max(0.0) as usize],
            };
            Ok(Value::Obj(Obj::new(
                Some(cx.proto_from(&nt, &cx.protos.bytes)),
                Kind::Bytes(bytes),
            )))
        })
    }));
    let ctor = cx.constructor("Uint8Array", 3, &proto, None, Some(construct));
    cx.method_async(&ctor, "from", 1, |cx, _, args| {
        Box::pin(async move {
            let mut out = Vec::new();
            let map = arg(&args, 1);
            for (index, item) in cx
                .iterate_to_vec(&arg(&args, 0))
                .await?
                .into_iter()
                .enumerate()
            {
                let item = if map.is_callable() {
                    cx.call(&map, Value::Undefined, vec![item, Value::Num(index as f64)])
                        .await?
                } else {
                    item
                };
                out.push(byte_of(&cx, &item)?);
            }
            Ok(new_bytes(&cx, out))
        })
    });
    cx.method(&ctor, "of", 0, |cx, _, args| {
        let bytes = args
            .iter()
            .map(|v| byte_of(cx, v))
            .collect::<R<Vec<u8>>>()?;
        Ok(new_bytes(cx, bytes))
    });
    cx.method(&ctor, "fromBase64", 1, |cx, _, args| {
        let text = str_arg(cx, args, 0)?;
        let cleaned: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(cleaned.as_bytes())
            .or_else(|_| {
                base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned.as_bytes())
            })
            .or_else(|_| {
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(cleaned.trim_end_matches('=').as_bytes())
            })
            .map_err(|_| cx.throw(ErrorKind::Syntax, "Invalid base64 string"))?;
        Ok(new_bytes(cx, decoded))
    });
    cx.method(&ctor, "fromHex", 1, |cx, _, args| {
        let text = str_arg(cx, args, 0)?;
        if text.len() % 2 != 0 {
            return Err(cx.throw(ErrorKind::Syntax, "Invalid hex string"));
        }
        let mut out = Vec::with_capacity(text.len() / 2);
        for i in (0..text.len()).step_by(2) {
            let byte = text
                .get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            out.push(byte.ok_or_else(|| cx.throw(ErrorKind::Syntax, "Invalid hex string"))?);
        }
        Ok(new_bytes(cx, out))
    });
    cx.method(&proto, "toBase64", 0, |cx, this, _| {
        let (_, bytes) = this_bytes(cx, this)?;
        Ok(Value::str(
            base64::engine::general_purpose::STANDARD.encode(bytes),
        ))
    });
    cx.method(&proto, "toHex", 0, |cx, this, _| {
        let (_, bytes) = this_bytes(cx, this)?;
        Ok(Value::str(
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        ))
    });
    for name in ["slice", "subarray"] {
        cx.method(&proto, name, 2, |cx, this, args| {
            let (_, bytes) = this_bytes(cx, this)?;
            let start = relative_index(int_arg(cx, args, 0, 0.0)?, bytes.len());
            let end = relative_index(int_arg(cx, args, 1, bytes.len() as f64)?, bytes.len());
            Ok(new_bytes(
                cx,
                if start < end {
                    bytes[start..end].to_vec()
                } else {
                    Vec::new()
                },
            ))
        });
    }
    cx.method(&proto, "set", 2, |cx, this, args| {
        let (obj, _) = this_bytes(cx, this)?;
        let source = match bytes_of(&arg(args, 0)) {
            Some(bytes) => bytes,
            None => arg(args, 0)
                .as_obj()
                .and_then(Obj::array_items)
                .unwrap_or_default()
                .iter()
                .map(|v| byte_of(cx, v))
                .collect::<R<Vec<u8>>>()?,
        };
        let offset = int_arg(cx, args, 1, 0.0)?.max(0.0) as usize;
        if let Kind::Bytes(bytes) = &mut obj.borrow_mut().kind {
            if offset + source.len() > bytes.len() {
                return Err(cx.range_error("offset is out of bounds"));
            }
            bytes[offset..offset + source.len()].copy_from_slice(&source);
        }
        Ok(Value::Undefined)
    });
    cx.method(&proto, "fill", 1, |cx, this, args| {
        let (obj, bytes) = this_bytes(cx, this)?;
        let value = byte_of(cx, &arg(args, 0))?;
        let start = relative_index(int_arg(cx, args, 1, 0.0)?, bytes.len());
        let end = relative_index(int_arg(cx, args, 2, bytes.len() as f64)?, bytes.len());
        if let Kind::Bytes(bytes) = &mut obj.borrow_mut().kind {
            for b in bytes.iter_mut().take(end).skip(start) {
                *b = value;
            }
        }
        Ok(this.clone())
    });
    cx.method(&proto, "join", 1, |cx, this, args| {
        let (_, bytes) = this_bytes(cx, this)?;
        let sep = match arg(args, 0) {
            Value::Undefined => JsStr::from(","),
            other => cx.to_str(&other)?,
        };
        Ok(Value::str(
            bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(&sep),
        ))
    });
    cx.method(&proto, "indexOf", 1, |cx, this, args| {
        let (_, bytes) = this_bytes(cx, this)?;
        let target = cx.to_num(&arg(args, 0))?;
        Ok(Value::Num(
            bytes
                .iter()
                .position(|b| f64::from(*b) == target)
                .map_or(-1.0, |i| i as f64),
        ))
    });
    cx.method(&proto, "includes", 1, |cx, this, args| {
        let (_, bytes) = this_bytes(cx, this)?;
        let target = cx.to_num(&arg(args, 0))?;
        Ok(Value::Bool(bytes.iter().any(|b| f64::from(*b) == target)))
    });
    cx.method(&proto, "toString", 0, |cx, this, _| {
        let (_, bytes) = this_bytes(cx, this)?;
        Ok(Value::str(
            bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ))
    });
    let values = cx.native("values", 0, |cx, this, _| {
        let (_, bytes) = this_bytes(cx, this)?;
        Ok(list_iterator(
            cx,
            bytes.iter().map(|b| Value::Num(f64::from(*b))).collect(),
        ))
    });
    proto.set_hidden("values", Value::Obj(values.clone()));
    proto.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_iterator.clone()),
        Prop::hidden(Value::Obj(values)),
    );
    cx.method_async(&proto, "forEach", 1, |cx, this, args| {
        Box::pin(async move {
            let (_, bytes) = this_bytes(&cx, &this)?;
            let f = arg(&args, 0);
            for (index, b) in bytes.into_iter().enumerate() {
                cx.call(
                    &f,
                    Value::Undefined,
                    vec![
                        Value::Num(f64::from(b)),
                        Value::Num(index as f64),
                        this.clone(),
                    ],
                )
                .await?;
            }
            Ok(Value::Undefined)
        })
    });
    cx.method_async(&proto, "map", 1, |cx, this, args| {
        Box::pin(async move {
            let (_, bytes) = this_bytes(&cx, &this)?;
            let f = arg(&args, 0);
            let mut out = Vec::with_capacity(bytes.len());
            for (index, b) in bytes.into_iter().enumerate() {
                let value = cx
                    .call(
                        &f,
                        Value::Undefined,
                        vec![
                            Value::Num(f64::from(b)),
                            Value::Num(index as f64),
                            this.clone(),
                        ],
                    )
                    .await?;
                out.push(byte_of(&cx, &value)?);
            }
            Ok(new_bytes(&cx, out))
        })
    });
    cx.define_global("Uint8Array", Value::Obj(ctor));

    let encoder_proto = cx.new_object();
    let encoder_construct = NativeImpl::Sync(Box::new(|cx: &Cx, nt: &Value, _: &[Value]| {
        let fallback = cx
            .builtins_lookup("TextEncoder")
            .and_then(|c| c.as_obj().and_then(|o| o.own_data("prototype")));
        let proto = match fallback {
            Some(Value::Obj(p)) => cx.proto_from(nt, &p),
            _ => cx.protos.object.clone(),
        };
        let obj = Obj::new(Some(proto), Kind::Plain);
        obj.set_hidden("encoding", Value::str("utf-8"));
        Ok(Value::Obj(obj))
    }));
    let encoder = cx.constructor(
        "TextEncoder",
        0,
        &encoder_proto,
        None,
        Some(encoder_construct),
    );
    cx.method(&encoder_proto, "encode", 1, |cx, _, args| {
        let text = match arg(args, 0) {
            Value::Undefined => JsStr::from(""),
            other => cx.to_str(&other)?,
        };
        Ok(new_bytes(cx, text.as_bytes().to_vec()))
    });
    cx.define_global("TextEncoder", Value::Obj(encoder));
    let decoder_proto = cx.new_object();
    let decoder_construct = NativeImpl::Sync(Box::new(|cx: &Cx, nt: &Value, args: &[Value]| {
        let label = match arg(args, 0) {
            Value::Undefined => "utf-8".to_string(),
            other => cx.to_str(&other)?.to_ascii_lowercase(),
        };
        if label != "utf-8" && label != "utf8" {
            return Err(cx.range_error(format!(
                "The encoding label provided ('{label}') is not supported"
            )));
        }
        let fallback = cx
            .builtins_lookup("TextDecoder")
            .and_then(|c| c.as_obj().and_then(|o| o.own_data("prototype")));
        let proto = match fallback {
            Some(Value::Obj(p)) => cx.proto_from(nt, &p),
            _ => cx.protos.object.clone(),
        };
        let obj = Obj::new(Some(proto), Kind::Plain);
        obj.set_hidden("encoding", Value::str("utf-8"));
        Ok(Value::Obj(obj))
    }));
    let decoder = cx.constructor(
        "TextDecoder",
        0,
        &decoder_proto,
        None,
        Some(decoder_construct),
    );
    cx.method(&decoder_proto, "decode", 1, |cx, _, args| {
        let bytes = match arg(args, 0) {
            Value::Undefined => Vec::new(),
            other => bytes_of(&other)
                .ok_or_else(|| cx.type_error("TextDecoder.decode expects a Uint8Array"))?,
        };
        cx.string(String::from_utf8_lossy(&bytes).into_owned())
    });
    cx.define_global("TextDecoder", Value::Obj(decoder));

    let btoa = cx.native("btoa", 1, |cx, _, args| {
        let text = str_arg(cx, args, 0)?;
        let mut latin1 = Vec::with_capacity(text.len());
        for c in text.chars() {
            if c as u32 > 0xff {
                return Err(cx.throw(
                    ErrorKind::Error,
                    "Invalid character: btoa accepts Latin-1 text only",
                ));
            }
            latin1.push(c as u8);
        }
        Ok(Value::str(
            base64::engine::general_purpose::STANDARD.encode(latin1),
        ))
    });
    cx.define_global("btoa", Value::Obj(btoa));
    let atob = cx.native("atob", 1, |cx, _, args| {
        let text = str_arg(cx, args, 0)?;
        let cleaned: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(cleaned.as_bytes())
            .or_else(|_| {
                base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned.as_bytes())
            })
            .map_err(|_| {
                cx.throw(
                    ErrorKind::Error,
                    "The string to be decoded is not correctly encoded.",
                )
            })?;
        Ok(Value::str(
            decoded.iter().map(|b| *b as char).collect::<String>(),
        ))
    });
    cx.define_global("atob", Value::Obj(atob));

    let crypto = cx.new_object();
    cx.method(&crypto, "randomUUID", 0, |_, _, _| {
        let mut bytes = [0u8; 16];
        random_bytes(&mut bytes);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Value::str(format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )))
    });
    cx.method(&crypto, "getRandomValues", 1, |cx, _, args| {
        let target = arg(args, 0);
        let Some(obj) = target.as_obj() else {
            return Err(cx.type_error("getRandomValues expects a Uint8Array"));
        };
        if let Kind::Bytes(bytes) = &mut obj.borrow_mut().kind {
            if bytes.len() > 65_536 {
                return Err(cx.throw(ErrorKind::Error, "getRandomValues length exceeds 65536"));
            }
            random_bytes(bytes);
        } else {
            return Err(cx.type_error("getRandomValues expects a Uint8Array"));
        }
        Ok(target)
    });
    cx.define_global("crypto", Value::Obj(crypto));
}

pub(super) fn install(cx: &Cx) {
    install_url(cx);
    install_search_params(cx);
    install_headers(cx);
    install_bytes(cx);
}
