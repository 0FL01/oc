//! `Date`. The interpreter has no time zone database, so local time is UTC.

use super::super::interp::{Cx, NativeImpl, R, arg};
use super::super::value::{Kind, Obj, Value, to_integer};

const MS_PER_DAY: f64 = 86_400_000.0;
const MAX_TIME: f64 = 8.64e15;
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `MakeDate(MakeDay(y, m, d), MakeTime(h, min, s, ms))` with month overflow.
pub(super) fn make_time(y: f64, mon: f64, d: f64, h: f64, min: f64, s: f64, ms: f64) -> f64 {
    if [y, mon, d, h, min, s, ms].iter().any(|v| !v.is_finite()) {
        return f64::NAN;
    }
    let (y, mon) = (to_integer(y), to_integer(mon));
    let ym = y + (mon / 12.0).floor();
    let mn = mon.rem_euclid(12.0);
    let days = days_from_civil(ym as i64, mn as i64 + 1, 1) as f64 + to_integer(d) - 1.0;
    let time = to_integer(h) * 3_600_000.0
        + to_integer(min) * 60_000.0
        + to_integer(s) * 1000.0
        + to_integer(ms);
    time_clip(days * MS_PER_DAY + time)
}

fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > MAX_TIME {
        return f64::NAN;
    }
    to_integer(t)
}

struct Parts {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
    weekday: usize,
}

fn parts(t: f64) -> Parts {
    let days = (t / MS_PER_DAY).floor();
    let ms_of_day = t - days * MS_PER_DAY;
    let (year, month, day) = civil_from_days(days as i64);
    let ms_of_day = ms_of_day as i64;
    Parts {
        year,
        month: month - 1,
        day,
        hour: ms_of_day / 3_600_000,
        minute: ms_of_day / 60_000 % 60,
        second: ms_of_day / 1000 % 60,
        ms: ms_of_day % 1000,
        weekday: ((days as i64 + 4).rem_euclid(7)) as usize,
    }
}

fn year_string(year: i64) -> String {
    if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", -year)
    } else {
        format!("+{year:06}")
    }
}

pub(super) fn iso_string(t: f64) -> String {
    let p = parts(t);
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year_string(p.year),
        p.month + 1,
        p.day,
        p.hour,
        p.minute,
        p.second,
        p.ms
    )
}

pub(in super::super) fn to_display_string(t: f64) -> String {
    if t.is_nan() {
        return "Invalid Date".into();
    }
    let p = parts(t);
    format!(
        "{} {} {:02} {} {:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
        DAYS[p.weekday],
        MONTHS[p.month as usize],
        p.day,
        year_string(p.year),
        p.hour,
        p.minute,
        p.second
    )
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

fn digits(s: &str, start: usize, len: usize) -> Option<i64> {
    let part = s.get(start..start + len)?;
    if !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

/// Parses ISO 8601 date-time strings and the `toString`/`toUTCString` formats.
pub(super) fn parse_date(text: &str) -> f64 {
    let s = text.trim();
    if let Some(t) = parse_iso(s) {
        return t;
    }
    parse_loose(s).unwrap_or(f64::NAN)
}

fn parse_iso(s: &str) -> Option<f64> {
    let (year, mut i) = if s.starts_with('+') || s.starts_with('-') {
        let y = digits(s, 1, 6)?;
        (if s.starts_with('-') { -y } else { y }, 7)
    } else {
        (digits(s, 0, 4)?, 4)
    };
    let mut month = 1;
    let mut day = 1;
    if s.as_bytes().get(i) == Some(&b'-') {
        month = digits(s, i + 1, 2)?;
        i += 3;
        if s.as_bytes().get(i) == Some(&b'-') {
            day = digits(s, i + 1, 2)?;
            i += 3;
        }
    }
    let (mut hour, mut minute, mut second, mut ms) = (0, 0, 0, 0.0);
    let mut offset_minutes = 0i64;
    let date_only = i == s.len();
    if !date_only {
        match s.as_bytes().get(i) {
            Some(b'T' | b't' | b' ') => {}
            _ => return None,
        }
        hour = digits(s, i + 1, 2)?;
        if s.as_bytes().get(i + 3) != Some(&b':') {
            return None;
        }
        minute = digits(s, i + 4, 2)?;
        i += 6;
        if s.as_bytes().get(i) == Some(&b':') {
            second = digits(s, i + 1, 2)?;
            i += 3;
            if s.as_bytes().get(i) == Some(&b'.') {
                let start = i + 1;
                let mut end = start;
                while s.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                let frac = s.get(start..end)?;
                if frac.is_empty() {
                    return None;
                }
                ms = format!("0.{frac}").parse::<f64>().ok()? * 1000.0;
                i = end;
            }
        }
        match s.get(i..) {
            Some("") | Some("Z") | Some("z") => {}
            Some(zone) if zone.starts_with('+') || zone.starts_with('-') => {
                let sign = if zone.starts_with('-') { -1 } else { 1 };
                let zh = digits(zone, 1, 2)?;
                let zm = match zone.as_bytes().get(3) {
                    Some(b':') => digits(zone, 4, 2)?,
                    Some(_) => digits(zone, 3, 2)?,
                    None => 0,
                };
                offset_minutes = sign * (zh * 60 + zm);
            }
            _ => return None,
        }
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 24
        || minute > 59
        || second > 59
    {
        return None;
    }
    let t = make_time(
        year as f64,
        (month - 1) as f64,
        day as f64,
        hour as f64,
        minute as f64,
        second as f64,
        ms.floor(),
    );
    Some(t - offset_minutes as f64 * 60_000.0)
}

fn parse_loose(s: &str) -> Option<f64> {
    // Formats like "Tue Mar 05 2024 10:00:00 GMT+0000" or "Tue, 05 Mar 2024 10:00:00 GMT" or "March 5, 2024".
    let cleaned: String = s.replace(',', " ");
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    if words
        .first()
        .is_some_and(|w| DAYS.iter().any(|d| w.starts_with(d)))
    {
        words.remove(0);
    }
    let month_of = |w: &str| {
        MONTHS
            .iter()
            .position(|m| w.len() >= 3 && w[..3].eq_ignore_ascii_case(m))
    };
    let (month, day, year, rest) = match words.as_slice() {
        [a, b, c, rest @ ..] if month_of(a).is_some() => (
            month_of(a)?,
            b.parse::<i64>().ok()?,
            c.parse::<i64>().ok()?,
            rest,
        ),
        [a, b, c, rest @ ..] if month_of(b).is_some() => (
            month_of(b)?,
            a.parse::<i64>().ok()?,
            c.parse::<i64>().ok()?,
            rest,
        ),
        _ => return None,
    };
    let (mut hour, mut minute, mut second) = (0, 0, 0);
    let mut offset = 0i64;
    for word in rest {
        if word.contains(':') {
            let mut it = word.split(':');
            hour = it.next()?.parse().ok()?;
            minute = it.next().unwrap_or("0").parse().ok()?;
            second = it.next().unwrap_or("0").parse().ok()?;
        } else if let Some(zone) = word
            .strip_prefix("GMT")
            .or_else(|| word.strip_prefix("UTC"))
            && zone.len() >= 5
        {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            offset = sign * (digits(zone, 1, 2)? * 60 + digits(zone, 3, 2)?);
        }
    }
    let t = make_time(
        year as f64,
        month as f64,
        day as f64,
        hour as f64,
        minute as f64,
        second as f64,
        0.0,
    );
    Some(t - offset as f64 * 60_000.0)
}

fn this_time(cx: &Cx, this: &Value) -> R<f64> {
    match this {
        Value::Obj(obj) => match &obj.borrow().kind {
            Kind::Date(t) => Ok(*t),
            _ => Err(cx.type_error("this is not a Date object.")),
        },
        _ => Err(cx.type_error("this is not a Date object.")),
    }
}

fn set_time(this: &Value, t: f64) -> Value {
    if let Value::Obj(obj) = this
        && let Kind::Date(slot) = &mut obj.borrow_mut().kind
    {
        *slot = t;
    }
    Value::Num(t)
}

fn date_args(cx: &Cx, args: &[Value]) -> R<f64> {
    let get = |i: usize, default: f64| -> R<f64> {
        match args.get(i) {
            None => Ok(default),
            Some(v) => cx.to_num(v),
        }
    };
    let mut year = get(0, f64::NAN)?;
    if year.is_finite() && (0.0..=99.0).contains(&to_integer(year)) {
        year += 1900.0;
    }
    Ok(make_time(
        year,
        get(1, 0.0)?,
        get(2, 1.0)?,
        get(3, 0.0)?,
        get(4, 0.0)?,
        get(5, 0.0)?,
        get(6, 0.0)?,
    ))
}

pub(super) fn install(cx: &Cx) {
    let proto = cx.protos.date.clone();
    let call = NativeImpl::Sync(Box::new(|_, _, _| Ok(Value::str(to_display_string(now())))));
    let construct = NativeImpl::Sync(Box::new(|cx: &Cx, nt: &Value, args: &[Value]| {
        let t = match args.len() {
            0 => now(),
            1 => match &args[0] {
                Value::Obj(obj) if matches!(obj.borrow().kind, Kind::Date(_)) => {
                    match &obj.borrow().kind {
                        Kind::Date(t) => *t,
                        _ => f64::NAN,
                    }
                }
                Value::Str(s) => parse_date(s),
                other => {
                    let prim = match other {
                        Value::Obj(obj) => super::object_to_primitive_builtin(cx, obj, true)?,
                        v => v.clone(),
                    };
                    match prim {
                        Value::Str(s) => parse_date(&s),
                        v => time_clip(cx.to_num(&v)?),
                    }
                }
            },
            _ => date_args(cx, args)?,
        };
        Ok(Value::Obj(Obj::new(
            Some(cx.proto_from(nt, &cx.protos.date)),
            Kind::Date(t),
        )))
    }));
    let ctor = cx.constructor("Date", 7, &proto, Some(call), Some(construct));
    cx.method(&ctor, "now", 0, |_, _, _| Ok(Value::Num(now())));
    cx.method(&ctor, "parse", 1, |cx, _, args| {
        Ok(Value::Num(parse_date(&super::str_arg(cx, args, 0)?)))
    });
    cx.method(&ctor, "UTC", 7, |cx, _, args| {
        Ok(Value::Num(date_args(cx, args)?))
    });

    for name in ["getTime", "valueOf"] {
        cx.method(&proto, name, 0, |cx, this, _| {
            Ok(Value::Num(this_time(cx, this)?))
        });
    }
    type PartGetter = fn(&Parts) -> f64;
    let getters: &[(&str, PartGetter)] = &[
        ("FullYear", |p| p.year as f64),
        ("Month", |p| p.month as f64),
        ("Date", |p| p.day as f64),
        ("Day", |p| p.weekday as f64),
        ("Hours", |p| p.hour as f64),
        ("Minutes", |p| p.minute as f64),
        ("Seconds", |p| p.second as f64),
        ("Milliseconds", |p| p.ms as f64),
    ];
    for (name, f) in getters {
        let f = *f;
        for prefix in ["get", "getUTC"] {
            cx.method(&proto, &format!("{prefix}{name}"), 0, move |cx, this, _| {
                let t = this_time(cx, this)?;
                Ok(Value::Num(if t.is_nan() { f64::NAN } else { f(&parts(t)) }))
            });
        }
    }
    cx.method(&proto, "getTimezoneOffset", 0, |cx, this, _| {
        let t = this_time(cx, this)?;
        Ok(Value::Num(if t.is_nan() { f64::NAN } else { 0.0 }))
    });
    cx.method(&proto, "setTime", 1, |cx, this, args| {
        this_time(cx, this)?;
        Ok(set_time(this, time_clip(super::num_arg(cx, args, 0)?)))
    });
    // Field setters: index of the first field each setter replaces in (y, mon, d, h, min, s, ms).
    let setters: &[(&str, usize, usize)] = &[
        ("FullYear", 0, 3),
        ("Month", 1, 2),
        ("Date", 2, 1),
        ("Hours", 3, 4),
        ("Minutes", 4, 3),
        ("Seconds", 5, 2),
        ("Milliseconds", 6, 1),
    ];
    for (name, first, max) in setters {
        let (first, max) = (*first, *max);
        for prefix in ["set", "setUTC"] {
            cx.method(
                &proto,
                &format!("{prefix}{name}"),
                max,
                move |cx, this, args| {
                    let t = this_time(cx, this)?;
                    let base = if t.is_nan() && first == 0 { 0.0 } else { t };
                    if base.is_nan() {
                        return Ok(Value::Num(f64::NAN));
                    }
                    let p = parts(base);
                    let mut fields = [
                        p.year as f64,
                        p.month as f64,
                        p.day as f64,
                        p.hour as f64,
                        p.minute as f64,
                        p.second as f64,
                        p.ms as f64,
                    ];
                    for (offset, value) in args.iter().take(max).enumerate() {
                        fields[first + offset] = cx.to_num(value)?;
                    }
                    let t = make_time(
                        fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6],
                    );
                    Ok(set_time(this, t))
                },
            );
        }
    }
    cx.method(&proto, "toISOString", 0, |cx, this, _| {
        let t = this_time(cx, this)?;
        if t.is_nan() {
            return Err(cx.range_error("Invalid time value"));
        }
        Ok(Value::str(iso_string(t)))
    });
    cx.method(&proto, "toJSON", 1, |cx, this, _| {
        let t = this_time(cx, this)?;
        Ok(if t.is_nan() {
            Value::Null
        } else {
            Value::str(iso_string(t))
        })
    });
    cx.method(&proto, "toString", 0, |cx, this, _| {
        Ok(Value::str(to_display_string(this_time(cx, this)?)))
    });
    cx.method(&proto, "toDateString", 0, |cx, this, _| {
        let t = this_time(cx, this)?;
        if t.is_nan() {
            return Ok(Value::str("Invalid Date"));
        }
        let p = parts(t);
        Ok(Value::str(format!(
            "{} {} {:02} {}",
            DAYS[p.weekday],
            MONTHS[p.month as usize],
            p.day,
            year_string(p.year)
        )))
    });
    cx.method(&proto, "toTimeString", 0, |cx, this, _| {
        let t = this_time(cx, this)?;
        if t.is_nan() {
            return Ok(Value::str("Invalid Date"));
        }
        let p = parts(t);
        Ok(Value::str(format!(
            "{:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
            p.hour, p.minute, p.second
        )))
    });
    cx.method(&proto, "toUTCString", 0, |cx, this, _| {
        let t = this_time(cx, this)?;
        if t.is_nan() {
            return Ok(Value::str("Invalid Date"));
        }
        let p = parts(t);
        Ok(Value::str(format!(
            "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
            DAYS[p.weekday],
            p.day,
            MONTHS[p.month as usize],
            year_string(p.year),
            p.hour,
            p.minute,
            p.second
        )))
    });
    let locale = |t: f64, date: bool, time: bool| -> String {
        if t.is_nan() {
            return "Invalid Date".into();
        }
        let p = parts(t);
        let d = format!("{}/{}/{}", p.month + 1, p.day, p.year);
        let hour12 = if p.hour % 12 == 0 { 12 } else { p.hour % 12 };
        let tm = format!(
            "{}:{:02}:{:02} {}",
            hour12,
            p.minute,
            p.second,
            if p.hour < 12 { "AM" } else { "PM" }
        );
        match (date, time) {
            (true, true) => format!("{d}, {tm}"),
            (true, false) => d,
            _ => tm,
        }
    };
    cx.method(&proto, "toLocaleString", 0, move |cx, this, _| {
        Ok(Value::str(locale(this_time(cx, this)?, true, true)))
    });
    cx.method(&proto, "toLocaleDateString", 0, move |cx, this, _| {
        Ok(Value::str(locale(this_time(cx, this)?, true, false)))
    });
    cx.method(&proto, "toLocaleTimeString", 0, move |cx, this, _| {
        Ok(Value::str(locale(this_time(cx, this)?, false, true)))
    });
    let _ = arg;
    cx.define_global("Date", Value::Obj(ctor));
}
