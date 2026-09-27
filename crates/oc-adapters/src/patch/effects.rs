//! Bounded Myers byte-line diff, donor-compatible ties and four lines of context.
//! Hard work/node limits prevent quadratic inputs from running unbounded. Beyond
//! those limits the explicit streaming replacement describes a valid edit script.
use oc_core::patch::*;
use std::collections::HashMap;

pub(super) fn file_effect(path: &str, op: &str, before: &[u8], after: &[u8]) -> FileEffect {
    let old_count = before.split_inclusive(|b| *b == b'\n').count();
    let new_count = after.split_inclusive(|b| *b == b'\n').count();
    if old_count.max(new_count) > EFFECT_INDEX_LINES_CAP {
        return streaming_effect(path, op, before, after, old_count, new_count);
    }
    let old: Vec<_> = before.split_inclusive(|b| *b == b'\n').collect();
    let new: Vec<_> = after.split_inclusive(|b| *b == b'\n').collect();
    let Some(runs) = minimal_runs(&old, &new) else {
        return streaming_effect(path, op, before, after, old_count, new_count);
    };
    let mut effect = FileEffect {
        algorithm: DiffAlgorithm::Minimal,
        operation: match op {
            "add" => PatchOperation::Create,
            "delete" => PatchOperation::Delete,
            _ => PatchOperation::Update,
        },
        path: path.into(),
        destination: None,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
        truncated: false,
    };
    let (mut o, mut n) = (0, 0);
    let mut steps = Vec::with_capacity(old_count + new_count);
    for (kind, count) in runs {
        for _ in 0..count {
            steps.push((kind, o, n));
            o += usize::from(kind != PatchLineKind::Added);
            n += usize::from(kind != PatchLineKind::Removed);
        }
        effect.additions += if kind == PatchLineKind::Added {
            count
        } else {
            0
        };
        effect.deletions += if kind == PatchLineKind::Removed {
            count
        } else {
            0
        };
    }
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for (i, (kind, _, _)) in steps.iter().enumerate() {
        if *kind == PatchLineKind::Context {
            continue;
        }
        let mut start = i;
        let mut end = i + 1;
        for _ in 0..4 {
            if start > 0 && steps[start - 1].0 == PatchLineKind::Context {
                start -= 1;
            }
            if end < steps.len() && steps[end].0 == PatchLineKind::Context {
                end += 1;
            }
        }
        if let Some(last) = windows.last_mut().filter(|last| start <= last.1) {
            last.1 = last.1.max(end);
        } else {
            windows.push((start, end));
        }
    }
    let mut rendered = 0;
    for (start, end) in windows {
        if effect.hunks.len() == EFFECT_HUNKS_CAP {
            effect.truncated = true;
            break;
        }
        let slice = &steps[start..end];
        let mut hunk = PatchHunk {
            old: LineRange {
                start: slice[0].1 + 1,
                count: slice.iter().filter(|s| s.0 != PatchLineKind::Added).count(),
            },
            new: LineRange {
                start: slice[0].2 + 1,
                count: slice
                    .iter()
                    .filter(|s| s.0 != PatchLineKind::Removed)
                    .count(),
            },
            lines: Vec::new(),
            truncated: false,
        };
        for &(kind, o, n) in slice {
            if rendered == EFFECT_LINES_CAP {
                hunk.truncated = true;
                break;
            }
            let bytes = if kind == PatchLineKind::Added {
                new[n]
            } else {
                old[o]
            };
            let mut line = render_line(
                bytes,
                kind,
                if kind == PatchLineKind::Added { n } else { o },
            );
            if kind == PatchLineKind::Context {
                line.old_line = Some(o + 1);
                line.new_line = Some(n + 1);
            }
            hunk.truncated |= line.truncated;
            hunk.lines.push(line);
            rendered += 1;
        }
        effect.truncated |= hunk.truncated;
        effect.hunks.push(hunk);
    }
    effect
}

/// Runs use an arena of immutable predecessor nodes rather than copying paths.
/// Tokens are interned once so comparisons in the search are constant-time.
fn minimal_runs(old: &[&[u8]], new: &[&[u8]]) -> Option<Vec<(PatchLineKind, usize)>> {
    const WORK_CAP: usize = 2_000_000;
    const NODE_CAP: usize = 262_144;
    #[derive(Clone, Copy)]
    struct Path {
        old: usize,
        tail: Option<usize>,
    }
    let mut tokens = HashMap::new();
    let ids = old
        .iter()
        .chain(new)
        .map(|line| {
            let next = tokens.len();
            *tokens.entry(*line).or_insert(next)
        })
        .collect::<Vec<_>>();
    let (a, b) = ids.split_at(old.len());
    let mut seen = vec![false; tokens.len()];
    for id in a {
        seen[*id] = true;
    }
    if !b.iter().any(|id| seen[*id]) {
        // With no common token, replacement is provably minimal. This also
        // makes large creates/deletes/full replacements linear-time.
        let mut runs = Vec::new();
        if !a.is_empty() {
            runs.push((PatchLineKind::Removed, a.len()));
        }
        if !b.is_empty() {
            runs.push((PatchLineKind::Added, b.len()));
        }
        return Some(runs);
    }
    let mut nodes: Vec<(PatchLineKind, usize, Option<usize>)> = Vec::new();
    let total = a.len() + b.len();
    let offset = total + 1;
    let mut paths = vec![None; total * 2 + 3];
    let mut initial = Path { old: 0, tail: None };
    while initial.old < a.len().min(b.len()) && a[initial.old] == b[initial.old] {
        initial.old += 1;
    }
    if initial.old > 0 {
        nodes.push((PatchLineKind::Context, initial.old, None));
        initial.tail = Some(0);
    }
    paths[offset] = Some(initial);
    let mut work = initial.old;
    let mut finished = (initial.old == a.len() && initial.old == b.len()).then_some(initial);
    let (mut min_k, mut max_k) = (-(total as isize), total as isize);
    'search: for d in 1..=total {
        if finished.is_some() {
            break;
        }
        for k in (-(d as isize)).max(min_k)..=(d as isize).min(max_k) {
            if (k + d as isize) % 2 != 0 {
                continue;
            }
            work += 1;
            if work > WORK_CAP || nodes.len() + 2 > NODE_CAP {
                return None;
            }
            let slot = (offset as isize + k) as usize;
            let remove = paths[slot - 1].take();
            let add = paths[slot + 1];
            let can_remove = remove.is_some_and(|p| p.old < a.len());
            let can_add = add.is_some_and(|p| {
                let n = p.old as isize - k;
                n > 0 && n <= b.len() as isize
            });
            if !can_add && !can_remove {
                paths[slot] = None;
                continue;
            }
            let (mut path, kind) =
                if !can_remove || (can_add && remove.unwrap().old < add.unwrap().old) {
                    (add.unwrap(), PatchLineKind::Added)
                } else {
                    (remove.unwrap(), PatchLineKind::Removed)
                };
            nodes.push((kind, 1, path.tail));
            path.tail = Some(nodes.len() - 1);
            path.old += usize::from(kind == PatchLineKind::Removed);
            let mut n = (path.old as isize - k) as usize;
            let start = path.old;
            while path.old < a.len() && n < b.len() && a[path.old] == b[n] {
                work += 1;
                if work > WORK_CAP {
                    return None;
                }
                path.old += 1;
                n += 1;
            }
            if path.old > start {
                nodes.push((PatchLineKind::Context, path.old - start, path.tail));
                path.tail = Some(nodes.len() - 1);
            }
            if path.old == a.len() && n == b.len() {
                finished = Some(path);
                break 'search;
            }
            paths[slot] = Some(path);
            if path.old == a.len() {
                max_k = max_k.min(k - 1);
            }
            if n == b.len() {
                min_k = min_k.max(k + 1);
            }
        }
    }
    let mut tail = finished?.tail;
    let mut runs = Vec::new();
    while let Some(i) = tail {
        let (kind, count, prev) = nodes[i];
        runs.push((kind, count));
        tail = prev;
    }
    runs.reverse();
    Some(runs)
}

fn streaming_effect(
    path: &str,
    op: &str,
    before: &[u8],
    after: &[u8],
    old_count: usize,
    new_count: usize,
) -> FileEffect {
    // Trim equal byte-lines without indexing the large images. Each byte is
    // visited a constant number of times, including the reverse suffix scan.
    let mut prefix = 0;
    let mut start = 0;
    for (old, new) in before
        .split_inclusive(|b| *b == b'\n')
        .zip(after.split_inclusive(|b| *b == b'\n'))
    {
        if old != new {
            break;
        }
        prefix += 1;
        start += old.len();
    }
    let (mut old_end, mut new_end) = (before.len(), after.len());
    let mut suffix = 0;
    while old_end > start && new_end > start {
        let a = last_line_start(before, start, old_end);
        let b = last_line_start(after, start, new_end);
        if before[a..old_end] != after[b..new_end] {
            break;
        }
        suffix += 1;
        old_end = a;
        new_end = b;
    }
    let old_count = old_count - prefix - suffix;
    let new_count = new_count - prefix - suffix;
    let unchanged = old_count == 0 && new_count == 0;
    let mut effect = FileEffect {
        algorithm: DiffAlgorithm::StreamingReplacement,
        operation: match op {
            "add" => PatchOperation::Create,
            "delete" => PatchOperation::Delete,
            _ => PatchOperation::Update,
        },
        path: path.into(),
        destination: None,
        additions: if unchanged { 0 } else { new_count },
        deletions: if unchanged { 0 } else { old_count },
        hunks: Vec::new(),
        truncated: false,
    };
    if unchanged {
        return effect;
    }
    let mut context_start = start;
    let leading = prefix.min(4);
    for _ in 0..leading {
        context_start = last_line_start(before, 0, context_start);
    }
    let trailing = suffix.min(4);
    let mut hunk = PatchHunk {
        old: LineRange {
            start: prefix + 1 - leading,
            count: old_count + leading + trailing,
        },
        new: LineRange {
            start: prefix + 1 - leading,
            count: new_count + leading + trailing,
        },
        lines: Vec::new(),
        truncated: old_count + new_count + leading + trailing > EFFECT_LINES_CAP,
    };
    for (i, bytes) in before[context_start..start]
        .split_inclusive(|b| *b == b'\n')
        .enumerate()
    {
        let mut line = render_line(bytes, PatchLineKind::Context, prefix - leading + i);
        line.old_line = Some(prefix - leading + i + 1);
        line.new_line = line.old_line;
        hunk.lines.push(line);
    }
    for (bytes, kind) in [
        (&before[start..old_end], PatchLineKind::Removed),
        (&after[start..new_end], PatchLineKind::Added),
    ] {
        for (i, bytes) in bytes
            .split_inclusive(|b| *b == b'\n')
            .enumerate()
            .take(EFFECT_LINES_CAP.saturating_sub(hunk.lines.len()))
        {
            hunk.lines.push(render_line(bytes, kind, prefix + i));
        }
    }
    for (i, bytes) in before[old_end..]
        .split_inclusive(|b| *b == b'\n')
        .take(trailing)
        .enumerate()
    {
        if hunk.lines.len() == EFFECT_LINES_CAP {
            break;
        }
        let mut line = render_line(bytes, PatchLineKind::Context, prefix + old_count + i);
        line.old_line = Some(prefix + old_count + i + 1);
        line.new_line = Some(prefix + new_count + i + 1);
        hunk.lines.push(line);
    }
    hunk.truncated |= hunk.lines.iter().any(|line| line.truncated);
    effect.truncated = hunk.truncated;
    effect.hunks.push(hunk);
    effect
}

fn last_line_start(bytes: &[u8], lower: usize, end: usize) -> usize {
    let search_end = end - usize::from(bytes[end - 1] == b'\n');
    bytes[lower..search_end]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(lower, |i| lower + i + 1)
}

fn render_line(bytes: &[u8], kind: PatchLineKind, i: usize) -> PatchLine {
    let (body, ending) = if let Some(body) = bytes.strip_suffix(b"\r\n") {
        (body, LineEnding::CrLf)
    } else if let Some(body) = bytes.strip_suffix(b"\n") {
        (body, LineEnding::Lf)
    } else {
        (bytes, LineEnding::None)
    };
    let text = String::from_utf8_lossy(body);
    let mut cap = text.len().min(EFFECT_LINE_BYTES_CAP);
    while !text.is_char_boundary(cap) {
        cap -= 1;
    }
    PatchLine {
        lossy: matches!(text, std::borrow::Cow::Owned(_)),
        kind,
        old_line: (kind == PatchLineKind::Removed).then_some(i + 1),
        new_line: (kind == PatchLineKind::Added).then_some(i + 1),
        text: text[..cap].into(),
        ending,
        truncated: cap < text.len(),
    }
}

pub(super) fn push(effects: &mut PatchEffects, file: FileEffect) {
    effects.total_files += 1;
    effects.additions += file.additions;
    effects.deletions += file.deletions;
    effects.truncated |= file.truncated;
    if effects.files.len() < EFFECT_FILES_CAP {
        effects.files.push(file);
    } else {
        effects.truncated = true;
    }
}

/// Bound the canonical settled DTO once, before storage and event publication.
/// Counts, paths and hunk ranges survive text/line truncation. Serving must never
/// invent a different preview based on the input size or its remaining quota.
pub(super) fn bound_serialized(effects: &mut PatchEffects) {
    let size = |value: &PatchEffects| serde_json::to_vec(value).expect("effects JSON").len();
    if size(effects) <= EFFECT_PREVIEW_BYTES_CAP {
        return;
    }
    effects.truncated = true;
    // JSON escaping and field overhead count toward the cap. Binary search a
    // common text limit, preserving all files/hunks and UTF-8 boundaries.
    let original = effects.clone();
    let trim = |value: &mut PatchEffects, cap: usize| {
        for file in &mut value.files {
            for hunk in &mut file.hunks {
                for line in &mut hunk.lines {
                    let end = line.text.floor_char_boundary(cap.min(line.text.len()));
                    if end < line.text.len() {
                        line.text.truncate(end);
                        line.truncated = true;
                        hunk.truncated = true;
                        file.truncated = true;
                    }
                }
            }
        }
    };
    let (mut lo, mut hi) = (0, EFFECT_LINE_BYTES_CAP);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let mut candidate = original.clone();
        trim(&mut candidate, mid);
        if size(&candidate) <= EFFECT_PREVIEW_BYTES_CAP {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    trim(effects, lo);
    while size(effects) > EFFECT_PREVIEW_BYTES_CAP {
        // Overhead alone can exceed the budget across many files. Remove only
        // trailing preview lines; exact ranges/counts and file headers remain.
        let Some(file) = effects
            .files
            .iter_mut()
            .rev()
            .find(|f| f.hunks.iter().any(|h| !h.lines.is_empty()))
        else {
            // Only unusually long paths can exhaust the header-only budget.
            // Keep exact global totals and report omitted trailing files.
            effects.files.pop();
            continue;
        };
        let hunk = file
            .hunks
            .iter_mut()
            .rev()
            .find(|h| !h.lines.is_empty())
            .unwrap();
        hunk.lines.pop();
        hunk.truncated = true;
        file.truncated = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vis35_serialized_escaping_and_many_file_overhead_retain_headers_and_ranges() {
        let image = format!("{}\n", "\"\\界".repeat(150)).repeat(120);
        let mut effects = PatchEffects::default();
        for i in 0..EFFECT_FILES_CAP {
            push(
                &mut effects,
                file_effect(&format!("f{i}"), "add", b"", image.as_bytes()),
            );
        }
        bound_serialized(&mut effects);
        assert!(serde_json::to_vec(&effects).unwrap().len() <= EFFECT_PREVIEW_BYTES_CAP);
        assert!(effects.truncated);
        assert_eq!(effects.files.len(), EFFECT_FILES_CAP);
        assert_eq!(effects.total_files, EFFECT_FILES_CAP);
        assert_eq!(effects.additions, 120 * EFFECT_FILES_CAP);
        for (i, file) in effects.files.iter().enumerate() {
            assert_eq!(file.path, format!("f{i}"));
            assert_eq!(file.hunks.len(), 1);
            assert_eq!(file.hunks[0].new.count, 120);
            assert!(file.truncated);
        }
    }

    #[test]
    fn vis35_donor_minimal_ties_context_and_merge_boundaries() {
        // diff@8.0.4 diffLines + structuredPatch(default context=4).
        for (before, after, removed_text, added_text) in [
            (
                "start\na\nb\na\nb\nend\n",
                "start\nb\na\nb\na\nend\n",
                "a",
                "a",
            ),
            ("a\nb\n", "b\na\n", "a", "a"),
            ("b\na\n", "a\nb\n", "b", "b"),
        ] {
            let effect = file_effect("f", "update", before.as_bytes(), after.as_bytes());
            assert_eq!(effect.algorithm, DiffAlgorithm::Minimal);
            assert_eq!((effect.additions, effect.deletions), (1, 1));
            let lines = &effect.hunks[0].lines;
            let removed = lines
                .iter()
                .find(|l| l.kind == PatchLineKind::Removed)
                .unwrap();
            let added = lines
                .iter()
                .find(|l| l.kind == PatchLineKind::Added)
                .unwrap();
            assert_eq!(removed.text, removed_text);
            assert_eq!(added.text, added_text);
            if before.starts_with("start") {
                assert_eq!(removed.old_line, Some(2));
                assert_eq!(added.new_line, Some(5));
                assert_eq!(lines.last().unwrap().text, "end");
                assert_eq!(lines.last().unwrap().old_line, Some(6));
                assert_eq!(lines.last().unwrap().new_line, Some(6));
            } else {
                assert_eq!(removed.old_line, Some(1));
                assert_eq!(added.new_line, Some(2));
            }
        }
        for gap in [8, 9] {
            let middle = (0..gap)
                .map(|i| format!("context{i}\r\n"))
                .collect::<String>();
            let before = format!("old\r\n{middle}tail");
            let after = format!("NEW\r\n{middle}TAIL");
            let effect = file_effect("f", "update", before.as_bytes(), after.as_bytes());
            assert_eq!(effect.hunks.len(), if gap == 8 { 1 } else { 2 });
            assert_eq!((effect.additions, effect.deletions), (2, 2));
            if gap == 9 {
                assert_eq!(effect.hunks[0].old.count, 5);
                assert_eq!(effect.hunks[1].old.start, 7);
                assert_eq!(effect.hunks[1].new.start, 7);
            }
            assert!(
                effect
                    .hunks
                    .last()
                    .unwrap()
                    .lines
                    .iter()
                    .any(|l| l.text == "TAIL" && l.ending == LineEnding::None)
            );
        }
        // Repetition alone must not cause fallback, even for medium images.
        let before = format!("start\n{}end\n", "a\nb\n".repeat(16_000));
        let after = format!("start\nb\n{}a\nend\n", "a\nb\n".repeat(15_999));
        let effect = file_effect("f", "update", before.as_bytes(), after.as_bytes());
        assert_eq!(effect.algorithm, DiffAlgorithm::Minimal);
        assert_eq!((effect.additions, effect.deletions), (1, 1));
        let effect = file_effect(
            "f",
            "update",
            "a\n".repeat(3000).as_bytes(),
            "b\n".repeat(3000).as_bytes(),
        );
        assert_eq!(effect.algorithm, DiffAlgorithm::Minimal);
        assert_eq!((effect.additions, effect.deletions), (3000, 3000));
        let before = format!("{}{}", "a\n".repeat(3000), "b\n".repeat(3000));
        let after = format!("{}{}", "b\n".repeat(3000), "a\n".repeat(3000));
        let effect = file_effect("f", "update", before.as_bytes(), after.as_bytes());
        assert_eq!(effect.algorithm, DiffAlgorithm::StreamingReplacement);
        assert_eq!((effect.additions, effect.deletions), (6000, 6000));
    }

    #[test]
    fn vis35_bounded_scripts_reconstruct_exact_bytes_with_repeated_lines() {
        let mut seed = 17u64;
        let mut image = || {
            let mut bytes = Vec::new();
            for _ in 0..10 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let token: &[u8] = match seed >> 61 {
                    0 | 1 => b"repeat\n",
                    2 => "界\r\n".as_bytes(),
                    3 => b"\n",
                    4 => b"other\r\n",
                    _ => b"unique\n",
                };
                bytes.extend_from_slice(token);
            }
            if seed & 1 != 0 {
                bytes.pop();
            }
            bytes
        };
        for _ in 0..200 {
            let before = image();
            let after = image();
            let effect = file_effect("fixture", "update", &before, &after);
            assert!(!effect.truncated);
            let old: Vec<_> = before.split_inclusive(|b| *b == b'\n').collect();
            let new: Vec<_> = after.split_inclusive(|b| *b == b'\n').collect();
            // Independent small-image LCS oracle verifies shortest-script counts.
            let mut lcs = vec![vec![0; new.len() + 1]; old.len() + 1];
            for i in 0..old.len() {
                for j in 0..new.len() {
                    lcs[i + 1][j + 1] = if old[i] == new[j] {
                        lcs[i][j] + 1
                    } else {
                        lcs[i][j + 1].max(lcs[i + 1][j])
                    };
                }
            }
            assert_eq!(
                (effect.additions, effect.deletions),
                (
                    new.len() - lcs[old.len()][new.len()],
                    old.len() - lcs[old.len()][new.len()]
                )
            );
            let mut reconstructed = Vec::new();
            let mut cursor = 0;
            let (mut added, mut removed) = (0, 0);
            for hunk in &effect.hunks {
                let start = hunk.old.start - 1;
                for bytes in &old[cursor..start] {
                    reconstructed.extend_from_slice(bytes);
                }
                cursor = start + hunk.old.count;
                let mut old_bytes = Vec::new();
                for line in &hunk.lines {
                    let mut bytes = line.text.as_bytes().to_vec();
                    bytes.extend_from_slice(match line.ending {
                        LineEnding::Lf => b"\n",
                        LineEnding::CrLf => b"\r\n",
                        LineEnding::None => b"",
                    });
                    if line.kind != PatchLineKind::Added {
                        old_bytes.extend_from_slice(&bytes);
                    }
                    if line.kind != PatchLineKind::Removed {
                        reconstructed.extend_from_slice(&bytes);
                    }
                    removed += usize::from(line.kind == PatchLineKind::Removed);
                    added += usize::from(line.kind == PatchLineKind::Added);
                }
                assert_eq!(old_bytes, old[start..cursor].concat());
            }
            for bytes in &old[cursor..] {
                reconstructed.extend_from_slice(bytes);
            }
            assert_eq!(reconstructed, after);
            assert_eq!((effect.additions, effect.deletions), (added, removed));
        }
    }
}
