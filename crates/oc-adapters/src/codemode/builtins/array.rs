//! `Array` and its prototype.

use super::super::interp::{Cx, NativeImpl, R, arg};
use super::super::value::{
    IterKind, IterMode, IterState, Kind, Obj, Value, relative_index, to_integer,
};
use super::{int_arg, num_arg};

fn this_array(cx: &Cx, this: &Value) -> R<Obj> {
    match this {
        Value::Obj(obj) if obj.is_array() => Ok(obj.clone()),
        _ => Err(cx.type_error("Array.prototype method called on a non-array value")),
    }
}

pub(super) fn items(obj: &Obj) -> Vec<Value> {
    obj.array_items().unwrap_or_default()
}

fn len(obj: &Obj) -> usize {
    match &obj.borrow().kind {
        Kind::Array(items) => items.len(),
        _ => 0,
    }
}

fn get(obj: &Obj, index: usize) -> Option<Value> {
    match &obj.borrow().kind {
        Kind::Array(items) => items.get(index).cloned(),
        _ => None,
    }
}

fn with_items<T>(obj: &Obj, f: impl FnOnce(&mut Vec<Value>) -> T) -> T {
    match &mut obj.borrow_mut().kind {
        Kind::Array(items) => f(items),
        _ => f(&mut Vec::new()),
    }
}

fn check_frozen(cx: &Cx, obj: &Obj) -> R<()> {
    if obj.borrow().frozen {
        return Err(cx.type_error("Cannot modify a frozen array"));
    }
    Ok(())
}

pub(super) fn array_iterator(cx: &Cx, array: Obj, mode: IterMode) -> Value {
    Value::Obj(Obj::new(
        Some(cx.protos.iterator.clone()),
        Kind::Iter(IterState {
            kind: IterKind::Array { array, mode },
            index: 0,
            done: false,
        }),
    ))
}

fn callback(cx: &Cx, args: &[Value], name: &str) -> R<(Value, Value)> {
    let f = arg(args, 0);
    if !f.is_callable() {
        return Err(cx.type_error(format!(
            "{} is not a function (Array.prototype.{name})",
            super::super::interp::describe_value(&f)
        )));
    }
    Ok((f, arg(args, 1)))
}

/// Calls `f(item, index, array)` for each index below the initial length.
async fn each<F>(cx: &Cx, obj: &Obj, f: &Value, this_arg: &Value, mut visit: F) -> R<()>
where
    F: FnMut(usize, Value, Value) -> R<bool>,
{
    let count = len(obj);
    for index in 0..count {
        cx.tick()?;
        let Some(item) = get(obj, index) else {
            break;
        };
        let result = cx
            .call(
                f,
                this_arg.clone(),
                vec![
                    item.clone(),
                    Value::Num(index as f64),
                    Value::Obj(obj.clone()),
                ],
            )
            .await?;
        if !visit(index, item, result)? {
            break;
        }
    }
    Ok(())
}

async fn sort_values(cx: &Cx, values: Vec<Value>, comparator: &Value) -> R<Vec<Value>> {
    if !comparator.is_nullish() && !comparator.is_callable() {
        return Err(cx.type_error("The comparison function must be either a function or undefined"));
    }
    let (mut defined, undefined): (Vec<Value>, Vec<Value>) = values
        .into_iter()
        .partition(|v| !matches!(v, Value::Undefined));
    if comparator.is_callable() {
        // Bottom-up merge sort: stable and only needs an async comparator.
        let mut width = 1;
        let n = defined.len();
        let mut buffer = defined.clone();
        while width < n {
            let mut start = 0;
            while start < n {
                let mid = (start + width).min(n);
                let end = (start + 2 * width).min(n);
                let (mut i, mut j, mut k) = (start, mid, start);
                while i < mid && j < end {
                    cx.tick()?;
                    let order = cx
                        .call(
                            comparator,
                            Value::Undefined,
                            vec![defined[j].clone(), defined[i].clone()],
                        )
                        .await?;
                    let order = cx.to_num(&order)?;
                    if order < 0.0 {
                        buffer[k] = defined[j].clone();
                        j += 1;
                    } else {
                        buffer[k] = defined[i].clone();
                        i += 1;
                    }
                    k += 1;
                }
                while i < mid {
                    buffer[k] = defined[i].clone();
                    i += 1;
                    k += 1;
                }
                while j < end {
                    buffer[k] = defined[j].clone();
                    j += 1;
                    k += 1;
                }
                start += 2 * width;
            }
            std::mem::swap(&mut defined, &mut buffer);
            width *= 2;
        }
    } else {
        let mut keyed = Vec::with_capacity(defined.len());
        for value in defined {
            let key = cx.to_string_async(&value).await?;
            keyed.push((key, value));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        defined = keyed.into_iter().map(|(_, v)| v).collect();
    }
    defined.extend(undefined);
    Ok(defined)
}

fn flatten(cx: &Cx, values: Vec<Value>, depth: f64, out: &mut Vec<Value>, level: usize) -> R<()> {
    if level > 512 {
        return Err(cx.range_error("Array nesting is too deep to flatten"));
    }
    for value in values {
        let nested = match &value {
            Value::Obj(obj) if depth >= 1.0 => obj.array_items(),
            _ => None,
        };
        match nested {
            Some(inner) => flatten(cx, inner, depth - 1.0, out, level + 1)?,
            None => {
                out.push(value);
                cx.check_array_len(out.len())?;
            }
        }
    }
    Ok(())
}

fn index_of(values: &[Value], target: &Value, from: usize, same_value_zero: bool) -> Option<usize> {
    values.iter().enumerate().skip(from).find_map(|(i, v)| {
        let hit = if same_value_zero {
            v.same_value_zero(target)
        } else {
            v.strict_equals(target)
        };
        hit.then_some(i)
    })
}

fn join(cx: &Cx, values: &[Value], sep: &str) -> R<String> {
    let mut out = String::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(sep);
        }
        if !value.is_nullish() {
            out.push_str(&cx.to_str(value)?);
        }
        cx.check_string_len(out.len())?;
    }
    Ok(out)
}

pub(super) fn install(cx: &Cx) {
    let proto = cx.protos.array.clone();
    let make = |cx: &Cx, nt: &Value, args: &[Value]| -> R<Value> {
        let proto = cx.proto_from(nt, &cx.protos.array);
        let items = if args.len() == 1
            && let Value::Num(n) = args[0]
        {
            if n < 0.0 || n.fract() != 0.0 || n > f64::from(u32::MAX) {
                return Err(cx.range_error("Invalid array length"));
            }
            cx.check_array_len(n as usize)?;
            vec![Value::Undefined; n as usize]
        } else {
            args.to_vec()
        };
        Ok(Value::Obj(Obj::new(Some(proto), Kind::Array(items))))
    };
    let call = NativeImpl::Sync(Box::new(move |cx: &Cx, _: &Value, args: &[Value]| {
        make(cx, &Value::Undefined, args)
    }));
    let construct = NativeImpl::Sync(Box::new(make));
    let ctor = cx.constructor("Array", 1, &proto, Some(call), Some(construct));
    cx.method(&ctor, "isArray", 1, |_, _, args| {
        Ok(Value::Bool(
            arg(args, 0).as_obj().is_some_and(Obj::is_array),
        ))
    });
    cx.method(&ctor, "of", 0, |cx, _, args| {
        Ok(cx.array_value(args.to_vec()))
    });
    cx.method_async(&ctor, "from", 1, |cx, _, args| {
        Box::pin(async move {
            let source = arg(&args, 0);
            let map = arg(&args, 1);
            let this_arg = arg(&args, 2);
            if !map.is_nullish() && !map.is_callable() {
                return Err(cx.type_error(
                    "Array.from: when provided, the second argument must be a function",
                ));
            }
            let has_iterator = match &source {
                Value::Str(_) => true,
                Value::Obj(_) => cx
                    .get(
                        &source,
                        &super::super::value::PropKey::Sym(cx.sym_iterator.clone()),
                    )
                    .await?
                    .is_callable(),
                _ => false,
            };
            let values = if has_iterator {
                cx.iterate_to_vec(&source).await?
            } else if let Value::Obj(_) = &source {
                let length = cx.get_str(&source, "length").await?;
                let length = to_integer(cx.to_num(&length)?).max(0.0) as usize;
                cx.check_array_len(length)?;
                let mut out = Vec::with_capacity(length);
                for index in 0..length {
                    out.push(cx.get_str(&source, &index.to_string()).await?);
                }
                out
            } else if source.is_nullish() {
                return Err(cx.type_error("Array.from requires an array-like object"));
            } else {
                Vec::new()
            };
            if map.is_callable() {
                let mut out = Vec::with_capacity(values.len());
                for (index, value) in values.into_iter().enumerate() {
                    cx.tick()?;
                    out.push(
                        cx.call(
                            &map,
                            this_arg.clone(),
                            vec![value, Value::Num(index as f64)],
                        )
                        .await?,
                    );
                }
                return Ok(cx.array_value(out));
            }
            Ok(cx.array_value(values))
        })
    });

    cx.method(&proto, "at", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let n = to_integer(num_arg(cx, args, 0)?);
        let count = len(&obj) as f64;
        let index = if n < 0.0 { count + n } else { n };
        if index < 0.0 || index >= count {
            return Ok(Value::Undefined);
        }
        Ok(get(&obj, index as usize).unwrap_or(Value::Undefined))
    });
    cx.method(&proto, "push", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        let new_len = len(&obj) + args.len();
        cx.check_array_len(new_len)?;
        with_items(&obj, |items| items.extend(args.iter().cloned()));
        Ok(Value::Num(new_len as f64))
    });
    cx.method(&proto, "pop", 0, |cx, this, _| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        Ok(with_items(&obj, Vec::pop).unwrap_or(Value::Undefined))
    });
    cx.method(&proto, "shift", 0, |cx, this, _| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        Ok(with_items(&obj, |items| {
            if items.is_empty() {
                None
            } else {
                Some(items.remove(0))
            }
        })
        .unwrap_or(Value::Undefined))
    });
    cx.method(&proto, "unshift", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        let new_len = len(&obj) + args.len();
        cx.check_array_len(new_len)?;
        with_items(&obj, |items| {
            items.splice(0..0, args.iter().cloned());
        });
        Ok(Value::Num(new_len as f64))
    });
    cx.method(&proto, "slice", 2, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let values = items(&obj);
        let start = relative_index(int_arg(cx, args, 0, 0.0)?, values.len());
        let end = relative_index(int_arg(cx, args, 1, values.len() as f64)?, values.len());
        let slice = if start < end {
            values[start..end].to_vec()
        } else {
            Vec::new()
        };
        Ok(cx.array_value(slice))
    });
    fn splice_args(cx: &Cx, args: &[Value], count: usize) -> R<(usize, usize, Vec<Value>)> {
        let start = relative_index(int_arg(cx, args, 0, 0.0)?, count);
        let delete = match args.len() {
            0 => 0,
            1 => count - start,
            _ => (to_integer(num_arg(cx, args, 1)?).max(0.0) as usize).min(count - start),
        };
        Ok((start, delete, args.iter().skip(2).cloned().collect()))
    }
    cx.method(&proto, "splice", 2, |cx, this, args| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        let (start, delete, insert) = splice_args(cx, args, len(&obj))?;
        cx.check_array_len(len(&obj) - delete + insert.len())?;
        let removed: Vec<Value> = with_items(&obj, |items| {
            items.splice(start..start + delete, insert).collect()
        });
        Ok(cx.array_value(removed))
    });
    cx.method(&proto, "toSpliced", 2, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let mut values = items(&obj);
        let (start, delete, insert) = splice_args(cx, args, values.len())?;
        cx.check_array_len(values.len() - delete + insert.len())?;
        values.splice(start..start + delete, insert);
        Ok(cx.array_value(values))
    });
    cx.method(&proto, "concat", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let mut values = items(&obj);
        for value in args {
            match value.as_obj().and_then(Obj::array_items) {
                Some(more) => values.extend(more),
                None => values.push(value.clone()),
            }
            cx.check_array_len(values.len())?;
        }
        Ok(cx.array_value(values))
    });
    cx.method(&proto, "join", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let sep = match arg(args, 0) {
            Value::Undefined => ",".into(),
            other => cx.to_str(&other)?,
        };
        Ok(Value::str(join(cx, &items(&obj), &sep)?))
    });
    cx.method(&proto, "toString", 0, |cx, this, _| {
        let obj = this_array(cx, this)?;
        Ok(Value::str(join(cx, &items(&obj), ",")?))
    });
    cx.method(&proto, "reverse", 0, |cx, this, _| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        with_items(&obj, |items| items.reverse());
        Ok(this.clone())
    });
    cx.method(&proto, "toReversed", 0, |cx, this, _| {
        let obj = this_array(cx, this)?;
        let mut values = items(&obj);
        values.reverse();
        Ok(cx.array_value(values))
    });
    cx.method(&proto, "indexOf", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let values = items(&obj);
        let from = relative_index(int_arg(cx, args, 1, 0.0)?, values.len());
        Ok(Value::Num(
            index_of(&values, &arg(args, 0), from, false).map_or(-1.0, |i| i as f64),
        ))
    });
    cx.method(&proto, "lastIndexOf", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let values = items(&obj);
        if values.is_empty() {
            return Ok(Value::Num(-1.0));
        }
        let from = int_arg(cx, args, 1, values.len() as f64 - 1.0)?;
        let from = if from < 0.0 {
            values.len() as f64 + from
        } else {
            from.min(values.len() as f64 - 1.0)
        };
        if from < 0.0 {
            return Ok(Value::Num(-1.0));
        }
        let target = arg(args, 0);
        let found = (0..=from as usize)
            .rev()
            .find(|i| values[*i].strict_equals(&target));
        Ok(Value::Num(found.map_or(-1.0, |i| i as f64)))
    });
    cx.method(&proto, "includes", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let values = items(&obj);
        let from = relative_index(int_arg(cx, args, 1, 0.0)?, values.len());
        Ok(Value::Bool(
            index_of(&values, &arg(args, 0), from, true).is_some(),
        ))
    });
    cx.method(&proto, "fill", 1, |cx, this, args| {
        let obj = this_array(cx, this)?;
        check_frozen(cx, &obj)?;
        let count = len(&obj);
        let start = relative_index(int_arg(cx, args, 1, 0.0)?, count);
        let end = relative_index(int_arg(cx, args, 2, count as f64)?, count);
        let value = arg(args, 0);
        with_items(&obj, |items| {
            for item in items.iter_mut().take(end).skip(start) {
                *item = value.clone();
            }
        });
        Ok(this.clone())
    });
    cx.method(&proto, "flat", 0, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let depth = int_arg(cx, args, 0, 1.0)?;
        let mut out = Vec::new();
        flatten(cx, items(&obj), depth, &mut out, 0)?;
        Ok(cx.array_value(out))
    });
    cx.method(&proto, "with", 2, |cx, this, args| {
        let obj = this_array(cx, this)?;
        let mut values = items(&obj);
        let n = to_integer(num_arg(cx, args, 0)?);
        let index = if n < 0.0 { values.len() as f64 + n } else { n };
        if index < 0.0 || index >= values.len() as f64 {
            return Err(cx.range_error("Invalid index"));
        }
        values[index as usize] = arg(args, 1);
        Ok(cx.array_value(values))
    });
    cx.method(&proto, "keys", 0, |cx, this, _| {
        Ok(array_iterator(cx, this_array(cx, this)?, IterMode::Keys))
    });
    cx.method(&proto, "entries", 0, |cx, this, _| {
        Ok(array_iterator(cx, this_array(cx, this)?, IterMode::Entries))
    });
    let values_fn = cx.native("values", 0, |cx, this, _| {
        Ok(array_iterator(cx, this_array(cx, this)?, IterMode::Values))
    });
    proto.set_hidden("values", Value::Obj(values_fn.clone()));
    proto.borrow_mut().props.insert(
        super::super::value::PropKey::Sym(cx.sym_iterator.clone()),
        super::super::value::Prop::hidden(Value::Obj(values_fn)),
    );

    cx.method_async(&proto, "forEach", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_array(&cx, &this)?;
            let (f, this_arg) = callback(&cx, &args, "forEach")?;
            each(&cx, &obj, &f, &this_arg, |_, _, _| Ok(true)).await?;
            Ok(Value::Undefined)
        })
    });
    cx.method_async(&proto, "map", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_array(&cx, &this)?;
            let (f, this_arg) = callback(&cx, &args, "map")?;
            let mut out = Vec::with_capacity(len(&obj));
            each(&cx, &obj, &f, &this_arg, |_, _, result| {
                out.push(result);
                Ok(true)
            })
            .await?;
            Ok(cx.array_value(out))
        })
    });
    cx.method_async(&proto, "filter", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_array(&cx, &this)?;
            let (f, this_arg) = callback(&cx, &args, "filter")?;
            let mut out = Vec::new();
            each(&cx, &obj, &f, &this_arg, |_, item, result| {
                if result.truthy() {
                    out.push(item);
                }
                Ok(true)
            })
            .await?;
            Ok(cx.array_value(out))
        })
    });
    cx.method_async(&proto, "flatMap", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_array(&cx, &this)?;
            let (f, this_arg) = callback(&cx, &args, "flatMap")?;
            let mut mapped = Vec::new();
            each(&cx, &obj, &f, &this_arg, |_, _, result| {
                mapped.push(result);
                Ok(true)
            })
            .await?;
            let mut out = Vec::new();
            flatten(&cx, mapped, 1.0, &mut out, 0)?;
            Ok(cx.array_value(out))
        })
    });
    for (name, want_index, from_end, want_value_on_miss) in [
        ("find", false, false, true),
        ("findIndex", true, false, false),
        ("findLast", false, true, true),
        ("findLastIndex", true, true, false),
    ] {
        cx.method_async(&proto, name, 1, move |cx, this, args| {
            Box::pin(async move {
                let obj = this_array(&cx, &this)?;
                let (f, this_arg) = callback(&cx, &args, name)?;
                let count = len(&obj);
                let order: Box<dyn Iterator<Item = usize>> = if from_end {
                    Box::new((0..count).rev())
                } else {
                    Box::new(0..count)
                };
                for index in order {
                    cx.tick()?;
                    let item = get(&obj, index).unwrap_or(Value::Undefined);
                    let hit = cx
                        .call(
                            &f,
                            this_arg.clone(),
                            vec![
                                item.clone(),
                                Value::Num(index as f64),
                                Value::Obj(obj.clone()),
                            ],
                        )
                        .await?;
                    if hit.truthy() {
                        return Ok(if want_index {
                            Value::Num(index as f64)
                        } else {
                            item
                        });
                    }
                }
                Ok(if want_value_on_miss {
                    Value::Undefined
                } else {
                    Value::Num(-1.0)
                })
            })
        });
    }
    for (name, every) in [("every", true), ("some", false)] {
        cx.method_async(&proto, name, 1, move |cx, this, args| {
            Box::pin(async move {
                let obj = this_array(&cx, &this)?;
                let (f, this_arg) = callback(&cx, &args, name)?;
                let mut answer = every;
                each(&cx, &obj, &f, &this_arg, |_, _, result| {
                    if result.truthy() != every {
                        answer = !every;
                        return Ok(false);
                    }
                    Ok(true)
                })
                .await?;
                Ok(Value::Bool(answer))
            })
        });
    }
    for (name, from_end) in [("reduce", false), ("reduceRight", true)] {
        cx.method_async(&proto, name, 1, move |cx, this, args| {
            Box::pin(async move {
                let obj = this_array(&cx, &this)?;
                let f = arg(&args, 0);
                if !f.is_callable() {
                    return Err(cx.type_error(format!("{} is not a function", super::super::interp::describe_value(&f))));
                }
                let count = len(&obj);
                let mut order: Vec<usize> = (0..count).collect();
                if from_end {
                    order.reverse();
                }
                let mut order = order.into_iter();
                let mut acc = if args.len() >= 2 {
                    args[1].clone()
                } else {
                    match order.next() {
                        Some(index) => get(&obj, index).unwrap_or(Value::Undefined),
                        None => return Err(cx.type_error("Reduce of empty array with no initial value")),
                    }
                };
                for index in order {
                    cx.tick()?;
                    let item = get(&obj, index).unwrap_or(Value::Undefined);
                    acc = cx
                        .call(&f, Value::Undefined, vec![acc, item, Value::Num(index as f64), Value::Obj(obj.clone())])
                        .await?;
                }
                Ok(acc)
            })
        });
    }
    cx.method_async(&proto, "sort", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_array(&cx, &this)?;
            check_frozen(&cx, &obj)?;
            let sorted = sort_values(&cx, items(&obj), &arg(&args, 0)).await?;
            with_items(&obj, |items| *items = sorted);
            Ok(this)
        })
    });
    cx.method_async(&proto, "toSorted", 1, |cx, this, args| {
        Box::pin(async move {
            let obj = this_array(&cx, &this)?;
            let sorted = sort_values(&cx, items(&obj), &arg(&args, 0)).await?;
            Ok(cx.array_value(sorted))
        })
    });
    cx.define_global("Array", Value::Obj(ctor));
}
