//! Span edits of the one owner-controlled CLI setting. Comments and unrelated bytes survive.
use crate::config;

struct Token {
    start: usize,
    end: usize,
    kind: u8,
}
struct Member {
    key: String,
    value: usize,
    end: usize,
}

pub(crate) fn update(text: &str, auto: bool) -> Result<String, String> {
    let value = config::parse_jsonc(text, "CLI settings").map_err(|e| e.to_string())?;
    let root = value.as_object().ok_or("CLI settings must be an object")?;
    if root.get("session").is_some_and(|v| !v.is_object()) {
        return Err("session must be an object".into());
    }
    let tokens = tokens(text);
    let (members, closing) = members(text, &tokens, 0)?;
    let replacement = if auto { "\"autoaccept\"" } else { "\"prompt\"" };
    if let Some(session) = members.iter().rev().find(|m| m.key == "session") {
        let (settings, close) = self::members(text, &tokens, session.value)?;
        if let Some(permission) = settings.iter().rev().find(|m| m.key == "permissions") {
            let range = tokens[permission.value].start..tokens[permission.end].end;
            let mut out = text.to_string();
            out.replace_range(range, replacement);
            return Ok(out);
        }
        return Ok(insert(
            text,
            &tokens,
            close,
            !settings.is_empty(),
            &format!("\"permissions\": {replacement}"),
        ));
    }
    Ok(insert(
        text,
        &tokens,
        closing,
        !members.is_empty(),
        &format!("\"session\": {{\"permissions\": {replacement}}}"),
    ))
}

fn insert(text: &str, tokens: &[Token], close: usize, nonempty: bool, property: &str) -> String {
    let offset = tokens[close].start;
    let comma = if nonempty && tokens[close - 1].kind != b',' {
        ","
    } else {
        ""
    };
    let indent = text[..offset]
        .rsplit_once('\n')
        .map_or("", |(_, tail)| tail);
    let indent = if indent.bytes().all(|b| b == b' ' || b == b'\t') {
        indent
    } else {
        ""
    };
    let insertion = format!("{comma}\n{indent}  {property}\n{indent}");
    let mut out = text.to_string();
    out.insert_str(offset, &insertion);
    out
}

fn members(text: &str, tokens: &[Token], open: usize) -> Result<(Vec<Member>, usize), String> {
    if tokens.get(open).is_none_or(|t| t.kind != b'{') {
        return Err("setting container must be an object".into());
    }
    let mut members = Vec::new();
    let mut at = open + 1;
    while tokens.get(at).is_some_and(|t| t.kind != b'}') {
        let key_token = tokens.get(at).ok_or("missing setting key")?;
        let key = serde_json::from_str::<String>(&text[key_token.start..key_token.end])
            .map_err(|e| e.to_string())?;
        at += 2; // key and colon (the complete JSONC was already validated).
        let value = at;
        let mut depth = 0usize;
        loop {
            let token = tokens.get(at).ok_or("missing setting value")?;
            match token.kind {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if depth == 0 {
                break;
            }
            at += 1;
        }
        members.push(Member {
            key,
            value,
            end: at,
        });
        at += 1;
        if tokens.get(at).is_some_and(|t| t.kind == b',') {
            at += 1;
        }
    }
    if tokens.get(at).is_none() {
        return Err("missing object end".into());
    }
    Ok((members, at))
}

fn tokens(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            while at + 1 < bytes.len() && !bytes[at..].starts_with(b"*/") {
                at += 1;
            }
            at += 2;
            continue;
        }
        let start = at;
        let kind = bytes[at];
        at += 1;
        if kind == b'"' {
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at += 2;
                    continue;
                }
                if bytes[at] == b'"' {
                    at += 1;
                    break;
                }
                at += 1;
            }
        } else if !b"{}[]:,".contains(&kind) {
            while at < bytes.len()
                && !bytes[at].is_ascii_whitespace()
                && !b"{}[]:,/".contains(&bytes[at])
            {
                at += 1;
            }
        }
        out.push(Token {
            start,
            end: at,
            kind,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_jsonc_comments_unknown_values_escapes_and_trailing_commas() {
        let original = "// settings 🦊\n{\n  \"session\": { /* before */ \"permissions\": \"prompt\" /* after */, \"tps\": false, },\n  \"unknown\": [\"session\", {\"permissions\": \"untouched\\\"\"}], // last\n}\n";
        assert_eq!(
            update(original, true).unwrap(),
            original.replacen("\"prompt\"", "\"autoaccept\"", 1)
        );
        for original in [
            "{/* keep */}",
            "{\"session\": {/* inner */}}",
            "{\"session\": {\"tps\":false, /* trailing */}}",
            "{\"unknown\":42 // end\n}",
        ] {
            let changed = update(original, true).unwrap();
            assert!(
                changed.contains("keep")
                    || changed.contains("inner")
                    || changed.contains("trailing")
                    || changed.contains("end")
            );
            assert_eq!(
                config::parse_jsonc(&changed, "test")
                    .unwrap()
                    .pointer("/session/permissions")
                    .unwrap(),
                "autoaccept"
            );
        }
        assert!(update("{\"session\":false}", true).is_err());
        assert!(update("{", true).is_err());
    }
}
