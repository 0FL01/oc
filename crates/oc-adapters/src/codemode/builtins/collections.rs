//! `Map`, `Set`, their weak variants and built-in iterator objects.

use indexmap::IndexMap;

use super::super::interp::{Cx, NativeImpl, R, arg};
use super::super::value::{IterKind, IterMode, IterState, Kind, MapKey, Obj, Prop, PropKey, Value};

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

fn this_map(cx: &Cx, this: &Value) -> R<Obj> {
    match this {
        Value::Obj(obj) if matches!(obj.borrow().kind, Kind::Map(_)) => Ok(obj.clone()),
        _ => Err(cx.type_error("Map method called on incompatible receiver")),
    }
}

fn this_set(cx: &Cx, this: &Value) -> R<Obj> {
    match this {
        Value::Obj(obj) if matches!(obj.borrow().kind, Kind::Set(_)) => Ok(obj.clone()),
        _ => Err(cx.type_error("Set method called on incompatible receiver")),
    }
}

fn map_entries(obj: &Obj) -> Vec<(Value, Value)> {
    match &obj.borrow().kind {
        Kind::Map(map) => map.values().cloned().collect(),
        _ => Vec::new(),
    }
}

fn set_values(obj: &Obj) -> Vec<Value> {
    match &obj.borrow().kind {
        Kind::Set(set) => set.values().cloned().collect(),
        _ => Vec::new(),
    }
}

fn weak_key(cx: &Cx, key: &Value) -> R<()> {
    match key {
        Value::Obj(_) | Value::Sym(_) => Ok(()),
        _ => Err(cx.type_error("Invalid value used as weak collection key")),
    }
}

fn install_map(cx: &Cx, weak: bool) {
    let proto = if weak {
        cx.protos.weak_map.clone()
    } else {
        cx.protos.map.clone()
    };
    let name = if weak { "WeakMap" } else { "Map" };
    let construct = NativeImpl::Async(Box::new(move |cx, nt, args| {
        Box::pin(async move {
            let fallback = if weak {
                cx.protos.weak_map.clone()
            } else {
                cx.protos.map.clone()
            };
            let obj = Obj::new(
                Some(cx.proto_from(&nt, &fallback)),
                Kind::Map(IndexMap::new()),
            );
            let source = arg(&args, 0);
            if !source.is_nullish() {
                for entry in cx.iterate_to_vec(&source).await? {
                    let key = cx.get_str(&entry, "0").await?;
                    let value = cx.get_str(&entry, "1").await?;
                    if weak {
                        weak_key(&cx, &key)?;
                    }
                    if let Kind::Map(map) = &mut obj.borrow_mut().kind {
                        map.insert(MapKey::of(&key), (key, value));
                    }
                }
            }
            Ok(Value::Obj(obj))
        })
    }));
    let ctor = cx.constructor(name, 0, &proto, None, Some(construct));
    cx.method(&proto, "get", 1, |cx, this, args| {
        let obj = this_map(cx, this)?;
        let key = MapKey::of(&arg(args, 0));
        Ok(match &obj.borrow().kind {
            Kind::Map(map) => map
                .get(&key)
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Undefined),
            _ => Value::Undefined,
        })
    });
    cx.method(&proto, "set", 2, move |cx, this, args| {
        let obj = this_map(cx, this)?;
        let key = arg(args, 0);
        if weak {
            weak_key(cx, &key)?;
        }
        let key = if let Value::Num(n) = key
            && n == 0.0
        {
            Value::Num(0.0)
        } else {
            key
        };
        if let Kind::Map(map) = &mut obj.borrow_mut().kind {
            let entry = map
                .entry(MapKey::of(&key))
                .or_insert((key.clone(), Value::Undefined));
            entry.1 = arg(args, 1);
            cx.check_array_len(map.len())?;
        }
        Ok(this.clone())
    });
    cx.method(&proto, "has", 1, |cx, this, args| {
        let obj = this_map(cx, this)?;
        let key = MapKey::of(&arg(args, 0));
        Ok(Value::Bool(
            matches!(&obj.borrow().kind, Kind::Map(map) if map.contains_key(&key)),
        ))
    });
    cx.method(&proto, "delete", 1, |cx, this, args| {
        let obj = this_map(cx, this)?;
        let key = MapKey::of(&arg(args, 0));
        Ok(Value::Bool(match &mut obj.borrow_mut().kind {
            Kind::Map(map) => map.shift_remove(&key).is_some(),
            _ => false,
        }))
    });
    if !weak {
        cx.method(&proto, "clear", 0, |cx, this, _| {
            let obj = this_map(cx, this)?;
            if let Kind::Map(map) = &mut obj.borrow_mut().kind {
                map.clear();
            }
            Ok(Value::Undefined)
        });
        cx.getter(&proto, "size", |cx, this, _| {
            let obj = this_map(cx, this)?;
            Ok(Value::Num(match &obj.borrow().kind {
                Kind::Map(map) => map.len() as f64,
                _ => 0.0,
            }))
        });
        cx.method_async(&proto, "forEach", 1, |cx, this, args| {
            Box::pin(async move {
                let obj = this_map(&cx, &this)?;
                let f = arg(&args, 0);
                for (key, value) in map_entries(&obj) {
                    cx.tick()?;
                    cx.call(&f, arg(&args, 1), vec![value, key, this.clone()])
                        .await?;
                }
                Ok(Value::Undefined)
            })
        });
        cx.method(&proto, "keys", 0, |cx, this, _| {
            let obj = this_map(cx, this)?;
            Ok(list_iterator(
                cx,
                map_entries(&obj).into_iter().map(|(k, _)| k).collect(),
            ))
        });
        cx.method(&proto, "values", 0, |cx, this, _| {
            let obj = this_map(cx, this)?;
            Ok(list_iterator(
                cx,
                map_entries(&obj).into_iter().map(|(_, v)| v).collect(),
            ))
        });
        let entries = cx.native("entries", 0, |cx, this, _| {
            let obj = this_map(cx, this)?;
            let items = map_entries(&obj)
                .into_iter()
                .map(|(k, v)| cx.array_value(vec![k, v]))
                .collect();
            Ok(list_iterator(cx, items))
        });
        proto.set_hidden("entries", Value::Obj(entries.clone()));
        proto.borrow_mut().props.insert(
            PropKey::Sym(cx.sym_iterator.clone()),
            Prop::hidden(Value::Obj(entries)),
        );
        cx.method_async(&ctor, "groupBy", 2, |cx, _, args| {
            Box::pin(async move {
                let items = cx.iterate_to_vec(&arg(&args, 0)).await?;
                let callback = arg(&args, 1);
                let mut groups: IndexMap<MapKey, (Value, Value)> = IndexMap::new();
                for (index, item) in items.into_iter().enumerate() {
                    let key = cx
                        .call(
                            &callback,
                            Value::Undefined,
                            vec![item.clone(), Value::Num(index as f64)],
                        )
                        .await?;
                    let entry = groups
                        .entry(MapKey::of(&key))
                        .or_insert_with(|| (key.clone(), cx.array_value(Vec::new())));
                    if let Value::Obj(list) = &entry.1
                        && let Kind::Array(values) = &mut list.borrow_mut().kind
                    {
                        values.push(item);
                    }
                }
                Ok(Value::Obj(Obj::new(
                    Some(cx.protos.map.clone()),
                    Kind::Map(groups),
                )))
            })
        });
    }
    proto.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_to_string_tag.clone()),
        Prop::hidden(Value::str(name)),
    );
    cx.define_global(name, Value::Obj(ctor));
}

fn install_set(cx: &Cx, weak: bool) {
    let proto = if weak {
        cx.protos.weak_set.clone()
    } else {
        cx.protos.set.clone()
    };
    let name = if weak { "WeakSet" } else { "Set" };
    let construct = NativeImpl::Async(Box::new(move |cx, nt, args| {
        Box::pin(async move {
            let fallback = if weak {
                cx.protos.weak_set.clone()
            } else {
                cx.protos.set.clone()
            };
            let obj = Obj::new(
                Some(cx.proto_from(&nt, &fallback)),
                Kind::Set(IndexMap::new()),
            );
            let source = arg(&args, 0);
            if !source.is_nullish() {
                for value in cx.iterate_to_vec(&source).await? {
                    if weak {
                        weak_key(&cx, &value)?;
                    }
                    if let Kind::Set(set) = &mut obj.borrow_mut().kind {
                        set.insert(MapKey::of(&value), value);
                    }
                }
            }
            Ok(Value::Obj(obj))
        })
    }));
    let ctor = cx.constructor(name, 0, &proto, None, Some(construct));
    cx.method(&proto, "add", 1, move |cx, this, args| {
        let obj = this_set(cx, this)?;
        let value = arg(args, 0);
        if weak {
            weak_key(cx, &value)?;
        }
        let value = if let Value::Num(n) = value
            && n == 0.0
        {
            Value::Num(0.0)
        } else {
            value
        };
        if let Kind::Set(set) = &mut obj.borrow_mut().kind {
            set.entry(MapKey::of(&value)).or_insert(value);
            cx.check_array_len(set.len())?;
        }
        Ok(this.clone())
    });
    cx.method(&proto, "has", 1, |cx, this, args| {
        let obj = this_set(cx, this)?;
        let key = MapKey::of(&arg(args, 0));
        Ok(Value::Bool(
            matches!(&obj.borrow().kind, Kind::Set(set) if set.contains_key(&key)),
        ))
    });
    cx.method(&proto, "delete", 1, |cx, this, args| {
        let obj = this_set(cx, this)?;
        let key = MapKey::of(&arg(args, 0));
        Ok(Value::Bool(match &mut obj.borrow_mut().kind {
            Kind::Set(set) => set.shift_remove(&key).is_some(),
            _ => false,
        }))
    });
    if !weak {
        cx.method(&proto, "clear", 0, |cx, this, _| {
            let obj = this_set(cx, this)?;
            if let Kind::Set(set) = &mut obj.borrow_mut().kind {
                set.clear();
            }
            Ok(Value::Undefined)
        });
        cx.getter(&proto, "size", |cx, this, _| {
            let obj = this_set(cx, this)?;
            Ok(Value::Num(set_values(&obj).len() as f64))
        });
        cx.method_async(&proto, "forEach", 1, |cx, this, args| {
            Box::pin(async move {
                let obj = this_set(&cx, &this)?;
                let f = arg(&args, 0);
                for value in set_values(&obj) {
                    cx.tick()?;
                    cx.call(&f, arg(&args, 1), vec![value.clone(), value, this.clone()])
                        .await?;
                }
                Ok(Value::Undefined)
            })
        });
        let values = cx.native("values", 0, |cx, this, _| {
            let obj = this_set(cx, this)?;
            Ok(list_iterator(cx, set_values(&obj)))
        });
        proto.set_hidden("values", Value::Obj(values.clone()));
        proto.set_hidden("keys", Value::Obj(values.clone()));
        proto.borrow_mut().props.insert(
            PropKey::Sym(cx.sym_iterator.clone()),
            Prop::hidden(Value::Obj(values)),
        );
        cx.method(&proto, "entries", 0, |cx, this, _| {
            let obj = this_set(cx, this)?;
            let items = set_values(&obj)
                .into_iter()
                .map(|v| cx.array_value(vec![v.clone(), v]))
                .collect();
            Ok(list_iterator(cx, items))
        });
        // ES2025 set algebra over another Set (or set-like with `has`/`keys`, approximated by Sets).
        fn other_values(cx: &Cx, args: &[Value]) -> R<Vec<Value>> {
            match arg(args, 0) {
                Value::Obj(obj) => match &obj.borrow().kind {
                    Kind::Set(set) => Ok(set.values().cloned().collect()),
                    Kind::Map(map) => Ok(map.values().map(|(k, _)| k.clone()).collect()),
                    _ => Err(cx.type_error("Set operation expects a Set argument")),
                },
                _ => Err(cx.type_error("Set operation expects a Set argument")),
            }
        }
        fn make_set(cx: &Cx, values: Vec<Value>) -> Value {
            let mut set = IndexMap::new();
            for value in values {
                set.entry(MapKey::of(&value)).or_insert(value);
            }
            Value::Obj(Obj::new(Some(cx.protos.set.clone()), Kind::Set(set)))
        }
        cx.method(&proto, "union", 1, |cx, this, args| {
            let mut values = set_values(&this_set(cx, this)?);
            values.extend(other_values(cx, args)?);
            Ok(make_set(cx, values))
        });
        cx.method(&proto, "intersection", 1, |cx, this, args| {
            let other: Vec<MapKey> = other_values(cx, args)?.iter().map(MapKey::of).collect();
            let values = set_values(&this_set(cx, this)?)
                .into_iter()
                .filter(|v| other.contains(&MapKey::of(v)))
                .collect();
            Ok(make_set(cx, values))
        });
        cx.method(&proto, "difference", 1, |cx, this, args| {
            let other: Vec<MapKey> = other_values(cx, args)?.iter().map(MapKey::of).collect();
            let values = set_values(&this_set(cx, this)?)
                .into_iter()
                .filter(|v| !other.contains(&MapKey::of(v)))
                .collect();
            Ok(make_set(cx, values))
        });
        cx.method(&proto, "symmetricDifference", 1, |cx, this, args| {
            let mine = set_values(&this_set(cx, this)?);
            let other = other_values(cx, args)?;
            let mine_keys: Vec<MapKey> = mine.iter().map(MapKey::of).collect();
            let other_keys: Vec<MapKey> = other.iter().map(MapKey::of).collect();
            let mut values: Vec<Value> = mine
                .into_iter()
                .filter(|v| !other_keys.contains(&MapKey::of(v)))
                .collect();
            values.extend(
                other
                    .into_iter()
                    .filter(|v| !mine_keys.contains(&MapKey::of(v))),
            );
            Ok(make_set(cx, values))
        });
        for (name, mode) in [
            ("isSubsetOf", 0),
            ("isSupersetOf", 1),
            ("isDisjointFrom", 2),
        ] {
            cx.method(&proto, name, 1, move |cx, this, args| {
                let mine: Vec<MapKey> = set_values(&this_set(cx, this)?)
                    .iter()
                    .map(MapKey::of)
                    .collect();
                let other: Vec<MapKey> = other_values(cx, args)?.iter().map(MapKey::of).collect();
                Ok(Value::Bool(match mode {
                    0 => mine.iter().all(|k| other.contains(k)),
                    1 => other.iter().all(|k| mine.contains(k)),
                    _ => mine.iter().all(|k| !other.contains(k)),
                }))
            });
        }
    }
    proto.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_to_string_tag.clone()),
        Prop::hidden(Value::str(name)),
    );
    cx.define_global(name, Value::Obj(ctor));
}

fn install_iterator(cx: &Cx) {
    let proto = cx.protos.iterator.clone();
    cx.method(&proto, "next", 0, |cx, this, _| {
        let Value::Obj(obj) = this else {
            return Err(cx.type_error("next called on incompatible receiver"));
        };
        let mut data = obj.borrow_mut();
        let Kind::Iter(state) = &mut data.kind else {
            return Err(cx.type_error("next called on incompatible receiver"));
        };
        if state.done {
            drop(data);
            return Ok(cx.iter_result(Value::Undefined, true));
        }
        let item = match &state.kind {
            IterKind::List(items) => items.get(state.index).cloned(),
            IterKind::Array { array, mode } => {
                let value = match &array.borrow().kind {
                    Kind::Array(items) => items.get(state.index).cloned(),
                    _ => None,
                };
                value.map(|v| match mode {
                    IterMode::Values => v,
                    IterMode::Keys => Value::Num(state.index as f64),
                    IterMode::Entries => cx.array_value(vec![Value::Num(state.index as f64), v]),
                })
            }
        };
        match item {
            Some(value) => {
                state.index += 1;
                drop(data);
                Ok(cx.iter_result(value, false))
            }
            None => {
                state.done = true;
                drop(data);
                Ok(cx.iter_result(Value::Undefined, true))
            }
        }
    });
    cx.method_async(&proto, "toArray", 0, |cx, this, _| {
        Box::pin(async move { Ok(cx.array_value(cx.iterate_to_vec(&this).await?)) })
    });
    proto.borrow_mut().props.insert(
        PropKey::Sym(cx.sym_to_string_tag.clone()),
        Prop::hidden(Value::str("Iterator")),
    );
}

pub(super) fn install(cx: &Cx) {
    install_map(cx, false);
    install_map(cx, true);
    install_set(cx, false);
    install_set(cx, true);
    install_iterator(cx);
}
