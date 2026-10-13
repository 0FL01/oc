//! `Object`, `Function.prototype`, `Symbol`, `Boolean` and the error constructors.

use std::rc::Rc;

use super::super::interp::{Cx, ErrorKind, NativeImpl, R, arg};
use super::super::value::{BoundFn, Func, Kind, Obj, Prop, PropKey, Slot, Sym, Symbol, Value};

fn this_obj(cx: &Cx, this: &Value) -> R<Obj> {
    cx.to_object(this)
}

fn obj_arg(cx: &Cx, args: &[Value], index: usize, method: &str) -> R<Obj> {
    match arg(args, index) {
        Value::Obj(obj) => Ok(obj),
        Value::Undefined | Value::Null => {
            Err(cx.type_error(format!("{method} called on null or undefined")))
        }
        other => cx.to_object(&other),
    }
}

fn descriptor(cx: &Cx, prop: &Prop) -> Value {
    let desc = cx.new_object();
    match &prop.slot {
        Slot::Data(value) => {
            desc.set_data("value", value.clone());
            desc.set_data("writable", Value::Bool(prop.writable));
        }
        Slot::Accessor { get, set } => {
            desc.set_data(
                "get",
                get.clone().map(Value::Obj).unwrap_or(Value::Undefined),
            );
            desc.set_data(
                "set",
                set.clone().map(Value::Obj).unwrap_or(Value::Undefined),
            );
        }
    }
    desc.set_data("enumerable", Value::Bool(prop.enumerable));
    desc.set_data("configurable", Value::Bool(prop.configurable));
    Value::Obj(desc)
}

fn own_prop(cx: &Cx, obj: &Obj, key: &PropKey) -> Option<Prop> {
    if let Some(prop) = obj.borrow().props.get(key) {
        return Some(prop.clone());
    }
    cx.own_slot(obj, key).map(|slot| {
        let is_length = key.as_str().is_some_and(|k| k.as_str() == "length");
        Prop {
            slot,
            enumerable: !is_length,
            writable: !obj.borrow().frozen,
            configurable: !is_length,
        }
    })
}

fn define_from_descriptor(cx: &Cx, obj: &Obj, key: PropKey, desc: &Value) -> R<()> {
    let Value::Obj(_) = desc else {
        return Err(cx.type_error("Property description must be an object"));
    };
    let field = |name: &str| cx.get_data(desc, name);
    let has = |name: &str| {
        desc.as_obj()
            .is_some_and(|d| cx.has_property(d, &PropKey::from(name)))
    };
    let existing = obj.borrow().props.get(&key).cloned();
    if obj.borrow().frozen || existing.as_ref().is_some_and(|p| !p.configurable) {
        return Err(cx.type_error(format!(
            "Cannot redefine property: {}",
            key.as_str().map(|s| s.as_str()).unwrap_or("symbol")
        )));
    }
    if has("get") || has("set") {
        let get = field("get")?;
        let set = field("set")?;
        let as_fn = |v: Value| -> R<Option<Obj>> {
            match v {
                Value::Undefined => Ok(None),
                Value::Obj(o) if o.is_callable() => Ok(Some(o)),
                _ => Err(cx.type_error("Getter/setter must be a function")),
            }
        };
        let prop = Prop {
            slot: Slot::Accessor {
                get: as_fn(get)?,
                set: as_fn(set)?,
            },
            enumerable: field("enumerable")?.truthy(),
            writable: false,
            configurable: field("configurable")?.truthy(),
        };
        obj.borrow_mut().props.insert(key, prop);
        return Ok(());
    }
    let value = if has("value") {
        field("value")?
    } else {
        existing
            .as_ref()
            .and_then(|p| match &p.slot {
                Slot::Data(v) => Some(v.clone()),
                Slot::Accessor { .. } => None,
            })
            .unwrap_or(Value::Undefined)
    };
    if obj.is_array()
        && let Some(k) = key.as_str()
        && super::super::value::array_index(k).is_some()
    {
        return cx.define(obj, key, value);
    }
    let prop = Prop {
        slot: Slot::Data(value),
        enumerable: if has("enumerable") {
            field("enumerable")?.truthy()
        } else {
            existing.as_ref().is_some_and(|p| p.enumerable)
        },
        writable: if has("writable") {
            field("writable")?.truthy()
        } else {
            existing.as_ref().is_some_and(|p| p.writable)
        },
        configurable: if has("configurable") {
            field("configurable")?.truthy()
        } else {
            existing.as_ref().is_some_and(|p| p.configurable)
        },
    };
    obj.borrow_mut().props.insert(key, prop);
    Ok(())
}

fn freeze(obj: &Obj) {
    let mut data = obj.borrow_mut();
    data.frozen = true;
    data.extensible = false;
    for prop in data.props.values_mut() {
        prop.configurable = false;
        if let Slot::Data(_) = prop.slot {
            prop.writable = false;
        }
    }
}

fn is_frozen(obj: &Obj) -> bool {
    let data = obj.borrow();
    if data.frozen {
        return true;
    }
    if data.extensible {
        return false;
    }
    let exotic_empty = match &data.kind {
        Kind::Array(items) => items.is_empty(),
        Kind::Bytes(bytes) => bytes.is_empty(),
        _ => true,
    };
    exotic_empty
        && data
            .props
            .values()
            .all(|p| !p.configurable && !matches!(p.slot, Slot::Data(_) if p.writable))
}

fn install_object(cx: &Cx) {
    let proto = cx.protos.object.clone();
    let call: NativeImpl = NativeImpl::Sync(Box::new(|cx, _, args| match arg(args, 0) {
        Value::Undefined | Value::Null => Ok(Value::Obj(cx.new_object())),
        other => Ok(Value::Obj(cx.to_object(&other)?)),
    }));
    let construct: NativeImpl =
        NativeImpl::Sync(Box::new(|cx, new_target, args| match arg(args, 0) {
            Value::Undefined | Value::Null => Ok(Value::Obj(Obj::new(
                Some(cx.proto_from(new_target, &cx.protos.object)),
                Kind::Plain,
            ))),
            other => Ok(Value::Obj(cx.to_object(&other)?)),
        }));
    let ctor = cx.constructor("Object", 1, &proto, Some(call), Some(construct));

    cx.method(&ctor, "keys", 1, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.keys")?;
        let keys = cx
            .own_keys(&obj, true, false)
            .into_iter()
            .filter_map(|k| k.as_str().cloned().map(Value::Str))
            .collect();
        Ok(cx.array_value(keys))
    });
    cx.method_async(&ctor, "values", 1, |cx, _, args| {
        Box::pin(async move {
            let obj = obj_arg(&cx, &args, 0, "Object.values")?;
            let entries = cx.own_entries(&obj).await?;
            Ok(cx.array_value(entries.into_iter().map(|(_, v)| v).collect()))
        })
    });
    cx.method_async(&ctor, "entries", 1, |cx, _, args| {
        Box::pin(async move {
            let obj = obj_arg(&cx, &args, 0, "Object.entries")?;
            let entries = cx.own_entries(&obj).await?;
            let items = entries
                .into_iter()
                .map(|(k, v)| cx.array_value(vec![Value::Str(k), v]))
                .collect();
            Ok(cx.array_value(items))
        })
    });
    cx.method_async(&ctor, "assign", 2, |cx, _, args| {
        Box::pin(async move {
            let target = obj_arg(&cx, &args, 0, "Object.assign")?;
            for source in args.iter().skip(1) {
                if let Value::Obj(src) = source {
                    for key in cx.own_keys(src, true, true) {
                        let value = cx.get(source, &key).await?;
                        cx.set(&Value::Obj(target.clone()), key, value).await?;
                    }
                } else if let Value::Str(_) = source {
                    cx.copy_data_properties(&target, source).await?;
                }
            }
            Ok(Value::Obj(target))
        })
    });
    cx.method_async(&ctor, "fromEntries", 1, |cx, _, args| {
        Box::pin(async move {
            let items = cx.iterate_to_vec(&arg(&args, 0)).await?;
            let out = cx.new_object();
            for item in items {
                let key = cx.get_str(&item, "0").await?;
                let value = cx.get_str(&item, "1").await?;
                let key = cx.to_property_key(&key)?;
                cx.define(&out, key, value)?;
            }
            Ok(Value::Obj(out))
        })
    });
    cx.method(&ctor, "freeze", 1, |_, _, args| {
        if let Value::Obj(obj) = arg(args, 0) {
            freeze(&obj);
        }
        Ok(arg(args, 0))
    });
    cx.method(&ctor, "isFrozen", 1, |_, _, args| {
        Ok(Value::Bool(match arg(args, 0) {
            Value::Obj(obj) => is_frozen(&obj),
            _ => true,
        }))
    });
    cx.method(&ctor, "seal", 1, |_, _, args| {
        if let Value::Obj(obj) = arg(args, 0) {
            let mut data = obj.borrow_mut();
            data.extensible = false;
            for prop in data.props.values_mut() {
                prop.configurable = false;
            }
        }
        Ok(arg(args, 0))
    });
    cx.method(&ctor, "isSealed", 1, |_, _, args| {
        Ok(Value::Bool(match arg(args, 0) {
            Value::Obj(obj) => {
                let data = obj.borrow();
                !data.extensible && data.props.values().all(|p| !p.configurable)
            }
            _ => true,
        }))
    });
    cx.method(&ctor, "preventExtensions", 1, |_, _, args| {
        if let Value::Obj(obj) = arg(args, 0) {
            obj.borrow_mut().extensible = false;
        }
        Ok(arg(args, 0))
    });
    cx.method(&ctor, "isExtensible", 1, |_, _, args| {
        Ok(Value::Bool(match arg(args, 0) {
            Value::Obj(obj) => obj.borrow().extensible,
            _ => false,
        }))
    });
    cx.method(&ctor, "create", 2, |cx, _, args| {
        let proto = match arg(args, 0) {
            Value::Obj(p) => Some(p),
            Value::Null => None,
            _ => return Err(cx.type_error("Object prototype may only be an Object or null")),
        };
        let obj = Obj::new(proto, Kind::Plain);
        if let Value::Obj(props) = arg(args, 1) {
            for key in cx.own_keys(&props, true, true) {
                let desc = cx.get_data(
                    &Value::Obj(props.clone()),
                    key.as_str().map(|s| s.as_str()).unwrap_or(""),
                )?;
                define_from_descriptor(cx, &obj, key, &desc)?;
            }
        }
        Ok(Value::Obj(obj))
    });
    cx.method(&ctor, "getPrototypeOf", 1, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.getPrototypeOf")?;
        Ok(obj.proto().map(Value::Obj).unwrap_or(Value::Null))
    });
    cx.method(&ctor, "setPrototypeOf", 2, |cx, _, args| {
        if let Value::Obj(obj) = arg(args, 0) {
            match arg(args, 1) {
                Value::Obj(p) => {
                    let mut cursor = Some(p.clone());
                    while let Some(c) = cursor {
                        if c.ptr_eq(&obj) {
                            return Err(cx.type_error("Cyclic __proto__ value"));
                        }
                        cursor = c.proto();
                    }
                    obj.borrow_mut().proto = Some(p);
                }
                Value::Null => obj.borrow_mut().proto = None,
                _ => return Err(cx.type_error("Object prototype may only be an Object or null")),
            }
        }
        Ok(arg(args, 0))
    });
    cx.method(&ctor, "defineProperty", 3, |cx, _, args| {
        let Value::Obj(obj) = arg(args, 0) else {
            return Err(cx.type_error("Object.defineProperty called on non-object"));
        };
        let key = cx.to_property_key(&arg(args, 1))?;
        define_from_descriptor(cx, &obj, key, &arg(args, 2))?;
        Ok(Value::Obj(obj))
    });
    cx.method(&ctor, "defineProperties", 2, |cx, _, args| {
        let Value::Obj(obj) = arg(args, 0) else {
            return Err(cx.type_error("Object.defineProperties called on non-object"));
        };
        let props = arg(args, 1);
        if let Value::Obj(p) = &props {
            for key in cx.own_keys(p, true, true) {
                let desc = match cx.lookup(&props, &key)? {
                    Some(Slot::Data(v)) => v,
                    _ => Value::Undefined,
                };
                define_from_descriptor(cx, &obj, key, &desc)?;
            }
        }
        Ok(Value::Obj(obj))
    });
    cx.method(&ctor, "getOwnPropertyNames", 1, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.getOwnPropertyNames")?;
        let keys = cx
            .own_keys(&obj, false, false)
            .into_iter()
            .filter_map(|k| k.as_str().cloned().map(Value::Str))
            .collect();
        Ok(cx.array_value(keys))
    });
    cx.method(&ctor, "getOwnPropertySymbols", 1, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.getOwnPropertySymbols")?;
        let keys = cx
            .own_keys(&obj, false, true)
            .into_iter()
            .filter(|k| matches!(k, PropKey::Sym(_)))
            .map(|k| k.to_value())
            .collect();
        Ok(cx.array_value(keys))
    });
    cx.method(&ctor, "getOwnPropertyDescriptor", 2, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.getOwnPropertyDescriptor")?;
        let key = cx.to_property_key(&arg(args, 1))?;
        Ok(own_prop(cx, &obj, &key)
            .map(|p| descriptor(cx, &p))
            .unwrap_or(Value::Undefined))
    });
    cx.method(&ctor, "getOwnPropertyDescriptors", 1, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.getOwnPropertyDescriptors")?;
        let out = cx.new_object();
        for key in cx.own_keys(&obj, false, true) {
            if let Some(prop) = own_prop(cx, &obj, &key) {
                out.set_data(key, descriptor(cx, &prop));
            }
        }
        Ok(Value::Obj(out))
    });
    cx.method(&ctor, "hasOwn", 2, |cx, _, args| {
        let obj = obj_arg(cx, args, 0, "Object.hasOwn")?;
        let key = cx.to_property_key(&arg(args, 1))?;
        Ok(Value::Bool(cx.has_own(&obj, &key)))
    });
    cx.method(&ctor, "is", 2, |_, _, args| {
        Ok(Value::Bool(arg(args, 0).same_value(&arg(args, 1))))
    });
    cx.method_async(&ctor, "groupBy", 2, |cx, _, args| {
        Box::pin(async move {
            let items = cx.iterate_to_vec(&arg(&args, 0)).await?;
            let callback = arg(&args, 1);
            let out = Obj::new(None, Kind::Plain);
            for (index, item) in items.into_iter().enumerate() {
                let key = cx
                    .call(
                        &callback,
                        Value::Undefined,
                        vec![item.clone(), Value::Num(index as f64)],
                    )
                    .await?;
                let key = cx.to_property_key(&key)?;
                let existing = out.borrow().props.get(&key).and_then(|p| match &p.slot {
                    Slot::Data(Value::Obj(o)) => Some(o.clone()),
                    _ => None,
                });
                match existing {
                    Some(group) => {
                        if let Kind::Array(list) = &mut group.borrow_mut().kind {
                            list.push(item);
                        }
                    }
                    None => out.set_data(key, cx.array_value(vec![item])),
                }
            }
            Ok(Value::Obj(out))
        })
    });

    cx.method(&proto, "hasOwnProperty", 1, |cx, this, args| {
        let obj = this_obj(cx, this)?;
        let key = cx.to_property_key(&arg(args, 0))?;
        Ok(Value::Bool(cx.has_own(&obj, &key)))
    });
    cx.method(&proto, "isPrototypeOf", 1, |_, this, args| {
        let (Value::Obj(this), Value::Obj(obj)) = (this, arg(args, 0)) else {
            return Ok(Value::Bool(false));
        };
        let mut cursor = obj.proto();
        while let Some(p) = cursor {
            if p.ptr_eq(this) {
                return Ok(Value::Bool(true));
            }
            cursor = p.proto();
        }
        Ok(Value::Bool(false))
    });
    cx.method(&proto, "propertyIsEnumerable", 1, |cx, this, args| {
        let obj = this_obj(cx, this)?;
        let key = cx.to_property_key(&arg(args, 0))?;
        Ok(Value::Bool(
            own_prop(cx, &obj, &key).is_some_and(|p| p.enumerable),
        ))
    });
    cx.method(&proto, "toString", 0, |cx, this, _| match this {
        Value::Undefined => Ok(Value::str("[object Undefined]")),
        Value::Null => Ok(Value::str("[object Null]")),
        Value::Obj(obj) => {
            let tag = match &obj.borrow().kind {
                Kind::Array(_) => Some("Array"),
                Kind::Function(_) => Some("Function"),
                Kind::Error => Some("Error"),
                Kind::Date(_) => Some("Date"),
                Kind::RegExp(_) => Some("RegExp"),
                _ => None,
            };
            match tag {
                Some(tag) => Ok(Value::str(format!("[object {tag}]"))),
                None => Ok(Value::Str(super::object_to_string_builtin(cx, obj)?)),
            }
        }
        Value::Num(_) => Ok(Value::str("[object Number]")),
        Value::Str(_) => Ok(Value::str("[object String]")),
        Value::Bool(_) => Ok(Value::str("[object Boolean]")),
        Value::Sym(_) => Ok(Value::str("[object Symbol]")),
    });
    cx.method_async(&proto, "toLocaleString", 0, |cx, this, _| {
        Box::pin(async move { Ok(Value::Str(cx.to_string_async(&this).await?)) })
    });
    cx.method(&proto, "valueOf", 0, |cx, this, _| {
        Ok(Value::Obj(this_obj(cx, this)?))
    });
    let proto_getter = cx.native("get __proto__", 0, |cx, this, _| {
        let obj = this_obj(cx, this)?;
        Ok(obj.proto().map(Value::Obj).unwrap_or(Value::Null))
    });
    let proto_setter = cx.native("set __proto__", 1, |_, this, args| {
        if let Value::Obj(obj) = this {
            match arg(args, 0) {
                Value::Obj(p) => obj.borrow_mut().proto = Some(p),
                Value::Null => obj.borrow_mut().proto = None,
                _ => {}
            }
        }
        Ok(Value::Undefined)
    });
    proto.borrow_mut().props.insert(
        "__proto__".into(),
        Prop {
            slot: Slot::Accessor {
                get: Some(proto_getter),
                set: Some(proto_setter),
            },
            enumerable: false,
            writable: false,
            configurable: true,
        },
    );
    cx.define_global("Object", Value::Obj(ctor));
}

fn install_function(cx: &Cx) {
    let proto = cx.protos.function.clone();
    // Function.prototype is itself callable in JS; a plain object suffices here.
    cx.method_async(&proto, "call", 1, |cx, this, args| {
        Box::pin(async move {
            let mut args = args.into_iter();
            let this_arg = args.next().unwrap_or(Value::Undefined);
            cx.call(&this, this_arg, args.collect()).await
        })
    });
    cx.method_async(&proto, "apply", 2, |cx, this, args| {
        Box::pin(async move {
            let this_arg = arg(&args, 0);
            let list = match arg(&args, 1) {
                Value::Undefined | Value::Null => Vec::new(),
                other => cx.iterate_to_vec(&other).await?,
            };
            cx.call(&this, this_arg, list).await
        })
    });
    cx.method(&proto, "bind", 1, |cx, this, args| {
        let Value::Obj(target) = this else {
            return Err(cx.type_error("Bind must be called on a function"));
        };
        if !target.is_callable() {
            return Err(cx.type_error("Bind must be called on a function"));
        }
        let name = match target.own_data("name") {
            Some(Value::Str(s)) => format!("bound {s}"),
            _ => "bound ".into(),
        };
        let bound = BoundFn {
            target: target.clone(),
            this: arg(args, 0),
            args: args.iter().skip(1).cloned().collect(),
        };
        let obj = Obj::new(
            Some(cx.protos.function.clone()),
            Kind::Function(Func::Bound(Rc::new(bound))),
        );
        obj.set_hidden("name", Value::str(name));
        Ok(Value::Obj(obj))
    });
    cx.method(&proto, "toString", 0, |cx, this, _| match this {
        Value::Obj(obj) => Ok(Value::Str(super::object_to_string_builtin(cx, obj)?)),
        _ => Err(cx.type_error("Function.prototype.toString requires that 'this' be a Function")),
    });
}

fn install_symbol(cx: &Cx) {
    let proto = cx.protos.symbol.clone();
    let call: NativeImpl = NativeImpl::Sync(Box::new(|cx, _, args| {
        let description = match arg(args, 0) {
            Value::Undefined => None,
            other => Some(cx.to_str(&other)?),
        };
        Ok(Value::Sym(Sym(Rc::new(Symbol { description }))))
    }));
    let ctor = cx.constructor("Symbol", 0, &proto, Some(call), None);
    for (name, sym) in [
        ("iterator", cx.sym_iterator.clone()),
        ("asyncIterator", cx.sym_async_iterator.clone()),
        ("hasInstance", cx.sym_has_instance.clone()),
        ("toStringTag", cx.sym_to_string_tag.clone()),
    ] {
        ctor.set_hidden(name, Value::Sym(sym));
    }
    cx.method(&ctor, "for", 1, |cx, _, args| {
        let key = super::str_arg(cx, args, 0)?;
        let mut registry = cx.symbol_registry.borrow_mut();
        let sym = registry
            .entry(key.clone())
            .or_insert_with(|| {
                Sym(Rc::new(Symbol {
                    description: Some(key),
                }))
            })
            .clone();
        Ok(Value::Sym(sym))
    });
    cx.method(&ctor, "keyFor", 1, |cx, _, args| {
        let Value::Sym(sym) = arg(args, 0) else {
            return Err(cx.type_error("Symbol.keyFor requires a symbol"));
        };
        let registry = cx.symbol_registry.borrow();
        Ok(registry
            .iter()
            .find(|(_, s)| **s == sym)
            .map(|(k, _)| Value::Str(k.clone()))
            .unwrap_or(Value::Undefined))
    });
    fn this_sym(cx: &Cx, this: &Value) -> R<Sym> {
        match this {
            Value::Sym(s) => Ok(s.clone()),
            Value::Obj(o) => match &o.borrow().kind {
                Kind::Boxed(Value::Sym(s)) => Ok(s.clone()),
                _ => Err(cx.type_error("not a symbol")),
            },
            _ => Err(cx.type_error("not a symbol")),
        }
    }
    cx.method(&proto, "toString", 0, |cx, this, _| {
        let sym = this_sym(cx, this)?;
        Ok(Value::str(format!(
            "Symbol({})",
            sym.0.description.as_ref().map(|d| d.as_str()).unwrap_or("")
        )))
    });
    cx.getter(&proto, "description", |cx, this, _| {
        let sym = this_sym(cx, this)?;
        Ok(sym
            .0
            .description
            .clone()
            .map(Value::Str)
            .unwrap_or(Value::Undefined))
    });
    cx.define_global("Symbol", Value::Obj(ctor));
}

fn install_boolean(cx: &Cx) {
    let proto = cx.protos.boolean.clone();
    proto.borrow_mut().kind = Kind::Boxed(Value::Bool(false));
    let call: NativeImpl = NativeImpl::Sync(Box::new(|_, _, args| {
        Ok(Value::Bool(arg(args, 0).truthy()))
    }));
    let construct: NativeImpl = NativeImpl::Sync(Box::new(|cx, nt, args| {
        Ok(Value::Obj(Obj::new(
            Some(cx.proto_from(nt, &cx.protos.boolean)),
            Kind::Boxed(Value::Bool(arg(args, 0).truthy())),
        )))
    }));
    let ctor = cx.constructor("Boolean", 1, &proto, Some(call), Some(construct));
    fn this_bool(cx: &Cx, this: &Value) -> R<bool> {
        match this {
            Value::Bool(b) => Ok(*b),
            Value::Obj(o) => match &o.borrow().kind {
                Kind::Boxed(Value::Bool(b)) => Ok(*b),
                _ => Err(cx.type_error("not a boolean")),
            },
            _ => Err(cx.type_error("not a boolean")),
        }
    }
    cx.method(&proto, "toString", 0, |cx, this, _| {
        Ok(Value::str(this_bool(cx, this)?.to_string()))
    });
    cx.method(&proto, "valueOf", 0, |cx, this, _| {
        Ok(Value::Bool(this_bool(cx, this)?))
    });
    cx.define_global("Boolean", Value::Obj(ctor));
}

fn error_construct(kind: ErrorKind) -> impl Fn(&Cx, &Value, &[Value]) -> R<Value> {
    move |cx, new_target, args| {
        let fallback = cx.error_proto(kind);
        let proto = cx.proto_from(new_target, &fallback);
        let (message_index, errors) = if kind == ErrorKind::Aggregate {
            (1, Some(arg(args, 0)))
        } else {
            (0, None)
        };
        let obj = Obj::new(Some(proto), Kind::Error);
        let message = arg(args, message_index);
        if !matches!(message, Value::Undefined) {
            let text = cx.to_str(&message)?;
            obj.set_hidden("message", Value::Str(text));
        }
        if let Value::Obj(options) = arg(args, message_index + 1)
            && cx.has_property(&options, &PropKey::from("cause"))
        {
            let cause = cx.get_data(&Value::Obj(options.clone()), "cause")?;
            obj.set_hidden("cause", cause);
        }
        if let Some(errors) = errors {
            let list = match &errors {
                Value::Obj(o) => o.array_items().unwrap_or_default(),
                _ => Vec::new(),
            };
            obj.set_hidden("errors", cx.array_value(list));
        }
        let stack_message = match obj.own_data("message") {
            Some(Value::Str(m)) => m.to_string(),
            _ => String::new(),
        };
        obj.set_hidden(
            "stack",
            Value::str(format!("{}: {stack_message}", kind.name())),
        );
        Ok(Value::Obj(obj))
    }
}

fn install_errors(cx: &Cx) {
    let base = cx.protos.error.clone();
    for kind in [
        ErrorKind::Error,
        ErrorKind::Type,
        ErrorKind::Range,
        ErrorKind::Syntax,
        ErrorKind::Reference,
        ErrorKind::Uri,
        ErrorKind::Aggregate,
    ] {
        let proto = cx.error_proto(kind);
        proto.set_hidden("name", Value::str(kind.name()));
        proto.set_hidden("message", Value::str(""));
        let call = NativeImpl::Sync(Box::new(move |cx: &Cx, _: &Value, args: &[Value]| {
            let target = Value::Undefined;
            error_construct(kind)(cx, &target, args)
        }));
        let construct = NativeImpl::Sync(Box::new(error_construct(kind)));
        let ctor = cx.constructor(kind.name(), 1, &proto, Some(call), Some(construct));
        if kind == ErrorKind::Error {
            cx.method(&ctor, "captureStackTrace", 1, |_, _, _| {
                Ok(Value::Undefined)
            });
        } else if let Some(Value::Obj(error_ctor)) = cx.builtins_lookup("Error") {
            ctor.borrow_mut().proto = Some(error_ctor);
        }
        cx.define_global(kind.name(), Value::Obj(ctor));
    }
    cx.method(&base, "toString", 0, |cx, this, _| match this {
        Value::Obj(obj) => Ok(Value::Str(super::object_to_string_builtin(cx, obj)?)),
        _ => Err(cx.type_error("Error.prototype.toString called on non-object")),
    });
}

pub(super) fn install(cx: &Cx) {
    install_object(cx);
    install_function(cx);
    install_symbol(cx);
    install_boolean(cx);
    install_errors(cx);
}
