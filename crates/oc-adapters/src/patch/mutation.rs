//! Native text preparation atop patch's descriptor-safe prepared commit/effects.
//! Matching is derived from OC2 edit.ts at 2670273ff17da96f85c5826ced57aa1b368754fa.
use super::*;
use serde_json::{Value, json};

pub(crate) enum Input<'a> {
    Write {
        path: &'a str,
        content: &'a str,
    },
    Edit {
        path: &'a str,
        old: &'a str,
        new: &'a str,
        all: bool,
    },
}

impl<'a> Input<'a> {
    pub(crate) fn parse(name: &str, args: &'a Value) -> Result<Self, String> {
        let object = args.as_object().ok_or("expected an object")?;
        if args.to_string().len() > PATCH_BYTES_CAP {
            return Err("mutation arguments exceed 2 MiB".into());
        }
        let string = |key| {
            object
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing {key}"))
        };
        let path = string("path")?;
        if path.is_empty() || path.contains('\0') {
            return Err("invalid path".into());
        }
        match name {
            "write"
                if object
                    .keys()
                    .all(|k| matches!(k.as_str(), "path" | "content")) =>
            {
                Ok(Self::Write {
                    path,
                    content: string("content")?,
                })
            }
            "edit"
                if object.keys().all(|k| {
                    matches!(
                        k.as_str(),
                        "path" | "oldString" | "newString" | "replaceAll"
                    )
                }) =>
            {
                let old = string("oldString")?;
                let new = string("newString")?;
                if old == new {
                    return Err("oldString and newString are identical".into());
                }
                if old.is_empty() {
                    return Err("oldString must not be empty; use write".into());
                }
                let all = object.get("replaceAll").map_or(Ok(false), |v| {
                    v.as_bool().ok_or("replaceAll must be boolean")
                })?;
                Ok(Self::Edit {
                    path,
                    old,
                    new,
                    all,
                })
            }
            _ => Err("unexpected mutation property".into()),
        }
    }
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Write { path, .. } | Self::Edit { path, .. } => path,
        }
    }
}

struct Plan {
    root: fs::Root,
    prepared: Prepared,
    digest: String,
    output: Value,
    kind: &'static str,
}

fn text<'a>(bytes: &'a [u8], path: &str) -> Result<&'a str, PatchError> {
    if bytes.contains(&0) {
        return Err(PatchError::Binary {
            path: path.into(),
            reason: "NUL byte in file content".into(),
        });
    }
    std::str::from_utf8(bytes).map_err(|_| PatchError::Binary {
        path: path.into(),
        reason: "non-UTF-8 content".into(),
    })
}

fn prepare(
    project: &Path,
    data: &Path,
    input: &Input<'_>,
    policy: &dyn WritePolicy,
) -> Result<Plan, PatchError> {
    let rel = input.path();
    policy.check(rel)?;
    let files = Files::new(project, data).map_err(|_| PatchError::Io { path: rel.into() })?;
    let canonical = project.to_path_buf();
    // Donor absolute/home spellings share the same lexical, no-follow project
    // admission. Strip only the admitted root; never canonicalize a target/link.
    let relative = if Path::new(rel).is_absolute() {
        Path::new(rel)
            .strip_prefix(&canonical)
            .map_err(|_| PatchError::OutsideRoot { path: rel.into() })?
            .to_str()
            .ok_or_else(|| PatchError::OutsideRoot { path: rel.into() })?
    } else {
        rel
    };
    let absolute = map_resolve(&files, relative)?;
    let path = absolute
        .strip_prefix(&canonical)
        .map_err(|_| PatchError::OutsideRoot { path: rel.into() })?
        .to_path_buf();
    let root = fs::Root::new(&canonical).map_err(|_| PatchError::Io { path: rel.into() })?;
    let io = |error: std::io::Error| {
        if error.kind() == std::io::ErrorKind::FileTooLarge {
            PatchError::TooLarge {
                path: rel.into(),
                reason: "file cap".into(),
            }
        } else {
            PatchError::Io { path: rel.into() }
        }
    };
    root.writable_parent(&path).map_err(io)?;
    let before = if root.absent(&path).map_err(io)? {
        None
    } else {
        Some(root.snapshot(&path).map_err(io)?)
    };
    let original = text(before.as_ref().map_or(&[], |s| s.bytes.as_slice()), rel)?;
    let bom = original.starts_with('\u{feff}');
    let source = original.strip_prefix('\u{feff}').unwrap_or(original);
    let (next, replacements) = match input {
        Input::Write { content, .. } => ((*content).to_string(), None),
        Input::Edit { old, new, all, .. } => {
            if before.is_none() {
                return Err(PatchError::Conflict {
                    path: rel.into(),
                    reason: "missing".into(),
                });
            }
            let (next, count) =
                replace(source, old, new, *all).map_err(|reason| PatchError::Conflict {
                    path: rel.into(),
                    reason,
                })?;
            (next, Some(count))
        }
    };
    let next_bom = next.starts_with('\u{feff}');
    let next = next.strip_prefix('\u{feff}').unwrap_or(&next);
    let mut after = Vec::with_capacity(next.len() + 3);
    if bom || next_bom {
        after.extend_from_slice(b"\xef\xbb\xbf");
    }
    after.extend_from_slice(next.as_bytes());
    text(&after, rel)?;
    if after.len() > FILE_BYTES_CAP {
        return Err(PatchError::TooLarge {
            path: rel.into(),
            reason: "file cap".into(),
        });
    }
    let kind = if before.is_some() { "update" } else { "add" };
    let mut hash = Sha256::new();
    hash.update(canonical.as_os_str().as_encoded_bytes());
    let (device, inode) = root.authority_identity().map_err(io)?;
    hash.update(device.to_le_bytes());
    hash.update(inode.to_le_bytes());
    hash.update(path.as_os_str().as_encoded_bytes());
    hash.update([u8::from(before.is_some())]);
    if let Some(before) = &before {
        before.hash_identity(&mut hash);
        hash.update(before.mode.to_le_bytes());
        hash.update(&before.bytes);
    }
    hash.update(&after);
    let output = if let Some(count) = replacements {
        json!({"operation":"edit", "target":absolute, "resource":rel, "replacements":count,
            "files":[{"file":rel,"status":"modified"}]})
    } else {
        json!({"operation":"write","target":absolute,"resource":rel,"existed":before.is_some()})
    };
    Ok(Plan {
        root,
        prepared: Prepared {
            path,
            target: None,
            before,
            after: Some(after),
        },
        digest: format!("{:x}", hash.finalize()),
        output,
        kind,
    })
}

pub(crate) fn preview(
    project: &Path,
    data: &Path,
    input: &Input<'_>,
) -> Result<(oc_core::patch::PatchEffects, String), PatchError> {
    let plan = prepare(project, data, input, &AllowAll)?;
    let mut proposed = oc_core::patch::PatchEffects::default();
    effects::push(
        &mut proposed,
        effects::file_effect(
            input.path(),
            plan.kind,
            plan.prepared
                .before
                .as_ref()
                .map_or(&[], |s| s.bytes.as_slice()),
            plan.prepared.after.as_deref().unwrap_or(&[]),
        ),
    );
    effects::bound_serialized(&mut proposed);
    Ok((proposed, plan.digest))
}

pub(crate) fn execute(
    project: &Path,
    data: &Path,
    input: &Input<'_>,
    policy: &dyn WritePolicy,
    expected: Option<&str>,
) -> (Result<Value, PatchError>, oc_core::patch::PatchEffects) {
    let mut confirmed = oc_core::patch::PatchEffects::default();
    let result = (|| {
        let plan = prepare(project, data, input, policy)?;
        if expected.is_some_and(|expected| expected != plan.digest) {
            return Err(PatchError::Conflict {
                path: input.path().into(),
                reason: "approval-preimage-changed".into(),
            });
        }
        commit_prepared(
            &plan.root,
            input.path(),
            plan.kind,
            None,
            &plan.prepared,
            policy,
            &mut Vec::new(),
            &mut confirmed,
        )?;
        Ok(plan.output)
    })();
    effects::bound_serialized(&mut confirmed);
    (result, confirmed)
}

fn typography(c: char) -> char {
    match c {
        '‘' | '’' | '‚' | '‛' => '\'',
        '“' | '”' | '„' | '‟' => '"',
        '‐' | '‑' | '‒' | '–' | '—' | '―' | '−' => '-',
        '\u{a0}' | '\u{2002}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}' => ' ',
        c => c,
    }
}
fn normalized(value: &str) -> String {
    value.chars().map(typography).collect()
}

fn trim_end(value: &str) -> &str {
    // ECMAScript trimEnd, including FEFF but excluding Rust-only NEL (0085).
    value.trim_end_matches(|c| matches!(c, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
}

// Offset pairs stay within the existing 64 MiB plan budget, including the
// original/result/normalized images. Never allocate one index per source byte.
fn occurrences(source: &str, old: &str) -> Result<Vec<(usize, usize)>, String> {
    let mut matches = Vec::new();
    for (start, _) in source.match_indices(old) {
        if matches.len() * std::mem::size_of::<(usize, usize)>() + 4 * FILE_BYTES_CAP
            >= PLAN_BYTES_CAP
        {
            return Err("match plan exceeds 64 MiB".into());
        }
        matches.push((start, start + old.len()));
    }
    Ok(matches)
}

fn replace(source: &str, old: &str, new: &str, all: bool) -> Result<(String, usize), String> {
    let ending = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let old = old.replace("\r\n", "\n").replace('\n', ending);
    let new = new.replace("\r\n", "\n").replace('\n', ending);
    let mut matches = occurrences(source, &old)?;
    if matches.is_empty() {
        let normalized_source = normalized(source);
        matches = occurrences(&normalized_source, &normalized(&old))?;
        // Donor's one-to-one *character* mappings preserve JS offsets. Rust byte
        // offsets differ for typographic punctuation: translate monotonically.
        let mut chars = source.char_indices();
        let (mut normal_offset, mut source_offset) = (0, 0);
        for pair in &mut matches {
            let (start, end) = *pair;
            while normal_offset < start {
                let (i, c) = chars.next().expect("normalized boundary");
                normal_offset += typography(c).len_utf8();
                source_offset = i + c.len_utf8();
            }
            let original_start = source_offset;
            while normal_offset < end {
                let (i, c) = chars.next().expect("normalized boundary");
                normal_offset += typography(c).len_utf8();
                source_offset = i + c.len_utf8();
            }
            *pair = (original_start, source_offset);
        }
    }
    if matches.is_empty() {
        matches = line_occurrences(source, &old)?;
    }
    if matches.is_empty() {
        return Err("could not find oldString".into());
    }
    if matches.len() > 1 && !all {
        return Err(format!(
            "found {} matches; expected exactly one (or replaceAll)",
            matches.len()
        ));
    }
    let count = matches.len();
    let mut result = String::new();
    let mut cursor = 0;
    for (start, end) in matches {
        if result
            .len()
            .saturating_add(start - cursor)
            .saturating_add(new.len())
            > FILE_BYTES_CAP
        {
            return Err("replacement exceeds file cap".into());
        }
        result.push_str(&source[cursor..start]);
        result.push_str(&new);
        cursor = end;
    }
    if result.len() + source.len() - cursor > FILE_BYTES_CAP {
        return Err("replacement exceeds file cap".into());
    }
    result.push_str(&source[cursor..]);
    Ok((result, count))
}

fn line_occurrences(source: &str, old: &str) -> Result<Vec<(usize, usize)>, String> {
    let trailing = old.ends_with('\n');
    let mut expected = Vec::new();
    for line in old.strip_suffix('\n').unwrap_or(old).split('\n') {
        if expected.len() == oc_core::patch::EFFECT_INDEX_LINES_CAP {
            return Err("line match index exceeds budget".into());
        }
        expected.push(normalized(trim_end(line)));
    }
    // Bound index work using the existing diff line-index budget. Exact and
    // typography paths do not require a line index at all.
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if lines.len() == oc_core::patch::EFFECT_INDEX_LINES_CAP {
            return Err("line match index exceeds budget".into());
        }
        lines.push((offset, line));
        offset += line.len();
    }
    let mut matches: Vec<(usize, usize)> = Vec::new();
    let mut work = 0usize;
    for window in lines.windows(expected.len()) {
        let start = window[0].0;
        if matches.last().is_some_and(|m| m.1 > start) {
            continue;
        }
        let mut equal = true;
        for ((_, actual), expected) in window.iter().zip(&expected) {
            work = work.saturating_add(actual.len() + expected.len());
            if work > PLAN_BYTES_CAP {
                return Err("line match work exceeds budget".into());
            }
            if normalized(trim_end(actual)) != *expected {
                equal = false;
                break;
            }
        }
        if !equal {
            continue;
        }
        let (last_start, last) = window.last().expect("nonempty oldString");
        if trailing && !last.ends_with('\n') {
            continue;
        }
        let length = if trailing {
            last.len()
        } else {
            last.strip_suffix('\n')
                .unwrap_or(last)
                .strip_suffix('\r')
                .unwrap_or(last.strip_suffix('\n').unwrap_or(last))
                .len()
        };
        matches.push((start, last_start + length));
    }
    Ok(matches)
}

#[cfg(test)]
mod tests;
