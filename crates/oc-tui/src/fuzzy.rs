//! Search-only Rust port of fuzzysort 3.1.0's algorithm/algorithmSpaces and keys
//! scoring. Source pinned by upstream v2.0.12 bun.lock. See assets/fuzzysort-LICENSE.
//! UTF-16 offsets, Latin-only accent removal, strict/backtracking/substring
//! bonuses, multi-key partial words and heap tie order follow the original.
//! Prepared targets and compiled queries are owned by the bounded modal cache.
use unicode_script::{Script, UnicodeScript};

fn strip_accents(text: &str) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut previous = None;
    let mut normalized = String::new();
    for c in text.chars() {
        if c.is_ascii() {
            result.push(c);
            continue;
        }
        if previous == Some(c) {
            result.push_str(&normalized);
            continue;
        }
        previous = Some(c);
        normalized.clear();
        let mut push = |c| {
            if !('\u{300}'..='\u{36f}').contains(&c) {
                normalized.push(c);
            }
        };
        if c.script() == Script::Latin {
            unicode_normalization::char::decompose_canonical(c, &mut push);
        } else {
            push(c);
        }
        result.push_str(&normalized);
    }
    result
}

fn beginnings(text: &str) -> Vec<usize> {
    let chars: Vec<_> = text.encode_utf16().collect();
    let mut starts = Vec::new();
    let (mut was_upper, mut was_alphanum) = (false, false);
    for (i, c) in chars.iter().copied().enumerate() {
        let upper = (65..=90).contains(&c);
        let alphanum = upper || (97..=122).contains(&c) || (48..=57).contains(&c);
        if upper && !was_upper || !was_alphanum || !alphanum {
            starts.push(i);
        }
        was_upper = upper;
        was_alphanum = alphanum;
    }
    let mut next = vec![chars.len(); chars.len()];
    let mut cursor = 0;
    for (i, slot) in next.iter_mut().enumerate() {
        while cursor < starts.len() && starts[cursor] <= i {
            cursor += 1;
        }
        *slot = starts.get(cursor).copied().unwrap_or(chars.len());
    }
    next
}

fn flags(codes: &[u16]) -> u128 {
    codes
        .iter()
        .fold(0, |flags, c| flags | (1 << (*c).min(127)))
}

pub(crate) struct Target {
    codes: Vec<u16>,
    next: Vec<usize>,
    flags: u128,
    positions: Vec<(u16, Vec<usize>)>,
}
impl Target {
    pub(crate) fn new(text: &str) -> Self {
        Self::prepare(text, true)
    }
    pub(crate) fn membership(text: &str) -> Self {
        Self::prepare(text, false)
    }
    fn prepare(text: &str, scored: bool) -> Self {
        let stripped = strip_accents(text);
        let codes: Vec<_> = stripped.to_lowercase().encode_utf16().collect();
        let mut ascii: [Vec<usize>; 128] = std::array::from_fn(|_| Vec::new());
        let mut non_ascii = std::collections::BTreeMap::<u16, Vec<usize>>::new();
        for (i, code) in codes.iter().copied().enumerate() {
            if code < 128 {
                ascii[usize::from(code)].push(i);
            } else {
                non_ascii.entry(code).or_default().push(i);
            }
        }
        let mut positions: Vec<_> = ascii
            .into_iter()
            .enumerate()
            .filter(|(_, indexes)| !indexes.is_empty())
            .map(|(code, indexes)| (code as u16, indexes))
            .collect();
        positions.extend(non_ascii);
        Self {
            flags: flags(&codes),
            codes,
            next: if scored {
                beginnings(&stripped)
            } else {
                Vec::new()
            },
            positions,
        }
    }
    fn contains(&self, search: &Search) -> bool {
        if search.codes.is_empty()
            || search.codes.len() > self.codes.len()
            || search.flags & self.flags != search.flags
        {
            return false;
        }
        if search.codes.len() > 32 {
            let mut i = 0;
            for c in &self.codes {
                if *c == search.codes[i] {
                    i += 1;
                    if i == search.codes.len() {
                        return true;
                    }
                }
            }
            return false;
        }
        let mut next = 0;
        for code in &search.codes {
            let Some(index) = self.after(*code, next) else {
                return false;
            };
            next = index;
        }
        true
    }
    fn after(&self, code: u16, next: usize) -> Option<usize> {
        let index = self
            .positions
            .binary_search_by_key(&code, |(c, _)| *c)
            .ok()?;
        let positions = &self.positions[index].1;
        positions
            .get(positions.partition_point(|i| *i < next))
            .map(|i| i + 1)
    }
}

struct Search {
    codes: Vec<u16>,
    prefix: Vec<usize>,
    flags: u128,
}
impl Search {
    fn new(text: &str) -> Self {
        let codes: Vec<_> = strip_accents(text).to_lowercase().encode_utf16().collect();
        let mut prefix = vec![0; codes.len()];
        let mut j = 0;
        for i in 1..codes.len() {
            while j > 0 && codes[i] != codes[j] {
                j = prefix[j - 1];
            }
            if codes[i] == codes[j] {
                j += 1;
            }
            prefix[i] = j;
        }
        Self {
            flags: flags(&codes),
            codes,
            prefix,
        }
    }
}

fn algorithm(compiled: &Search, prepared: &Target, next: &[usize]) -> Option<(f64, Vec<usize>)> {
    let search = &compiled.codes;
    let target = &prepared.codes;
    if compiled.flags & prepared.flags != compiled.flags || search.len() > target.len() {
        return None;
    }
    if search.is_empty() || target.is_empty() {
        return None;
    }
    let (n, len) = (search.len(), target.len());
    let mut simple = Vec::with_capacity(n);
    for (i, c) in target.iter().enumerate() {
        if *c == search[simple.len()] {
            simple.push(i);
            if simple.len() == n {
                break;
            }
        }
    }
    if simple.len() != n {
        return None;
    }
    let mut strict = Vec::with_capacity(n);
    let mut pos = if simple[0] == 0 {
        0
    } else {
        next[simple[0] - 1]
    };
    let mut backtracks = 0;
    if pos != len {
        loop {
            if pos >= len {
                if strict.is_empty() {
                    break;
                }
                backtracks += 1;
                if backtracks > 200 {
                    break;
                }
                pos = next[strict.pop().expect("nonempty")];
            } else if search[strict.len()] == target[pos] {
                strict.push(pos);
                if strict.len() == n {
                    break;
                }
                pos += 1;
            } else {
                pos = next[pos];
            }
        }
    }
    let success = strict.len() == n;
    // Linear substring search, including the later beginning bonus. The source
    // uses the engine's indexOf; naive windows are quadratic for 512-byte input.
    let (mut substring, mut substring_beginning) = (None, false);
    if n > 1 {
        let mut j = 0;
        for (i, c) in target.iter().enumerate().skip(simple[0]) {
            while j > 0 && *c != search[j] {
                j = compiled.prefix[j - 1];
            }
            if *c == search[j] {
                j += 1;
            }
            if j == n {
                let start = i + 1 - n;
                substring.get_or_insert(start);
                if start == 0 || next[start - 1] == start {
                    substring = Some(start);
                    substring_beginning = true;
                    break;
                }
                j = compiled.prefix[j - 1];
            }
        }
    }
    let matches = if (!success || substring_beginning)
        && let Some(i) = substring
    {
        (i..i + n).collect::<Vec<_>>()
    } else if success {
        strict
    } else {
        simple
    };
    let mut score = 0.0;
    let mut groups = 0;
    for pair in matches.windows(2) {
        if pair[1] - pair[0] != 1 {
            score -= pair[1] as f64;
            groups += 1;
        }
    }
    let distance = matches[n - 1] - matches[0] - (n - 1);
    score -= ((12 + distance) * groups) as f64;
    score -= (matches[0] * matches[0]) as f64 * 0.2;
    if !success {
        score *= 1000.0;
    } else {
        let mut count = 1;
        let mut i = next[0];
        while i < len {
            count += 1;
            i = next[i];
        }
        if count > 24 {
            score *= ((count - 24) * 10) as f64;
        }
    }
    score -= (len - n) as f64 / 2.0;
    if substring.is_some() {
        score /= (1 + n * n) as f64;
    }
    if substring_beginning {
        score /= (1 + n * n) as f64;
    }
    score -= (len - n) as f64 / 2.0;
    Some((score, matches))
}

fn normalize(score: f64) -> f64 {
    if score == f64::NEG_INFINITY {
        0.0
    } else if score > 1.0 {
        score
    } else {
        (((-score + 1.0).powf(0.04307) - 1.0) * -2.0).exp()
    }
}

/// Weighted mode is DialogSelect's title*2 + category + searchText. Unweighted
/// mode is DialogModel's title/category membership before its metadata sort.
pub(crate) struct Query {
    full: Search,
    searches: Vec<Search>,
    spaces: bool,
    prefixes: Vec<(usize, u16)>,
    terminals: Vec<usize>,
}
impl Query {
    pub(crate) fn new(query: &str) -> Self {
        let query = query.trim();
        let full = Search::new(query);
        let spaces = full.codes.contains(&32);
        let mut words = Vec::new();
        if spaces {
            for word in query.split_whitespace() {
                if !words.contains(&word) {
                    words.push(word);
                }
            }
        }
        let searches: Vec<_> = words.iter().map(|s| Search::new(s)).collect();
        let mut prefixes = vec![(0, 0)];
        let mut terminals = Vec::new();
        for word in &searches {
            let mut parent = 0;
            for code in &word.codes {
                let node = (parent, *code);
                parent = prefixes.iter().position(|p| *p == node).unwrap_or_else(|| {
                    prefixes.push(node);
                    prefixes.len() - 1
                });
            }
            terminals.push(parent);
        }
        Self {
            full,
            spaces,
            searches,
            prefixes,
            terminals,
        }
    }
    /// DialogModel discards scores and re-sorts metadata. A source result exists
    /// iff the full subsequence matches, or every distinct word matches a key.
    /// Strict/substring paths alter scores only; using this predicate avoids
    /// computing discarded scores and word-boundary copies for large catalogs.
    pub(crate) fn matches(&self, keys: &[Target]) -> bool {
        if self.spaces {
            if self.full.codes.is_empty() || self.searches.iter().any(|word| word.codes.is_empty())
            {
                return false;
            }
            let mut matched = vec![false; self.terminals.len()];
            let mut reached = vec![None; self.prefixes.len()];
            for key in keys {
                if key.codes.is_empty() {
                    continue;
                }
                reached[0] = Some(0);
                for (i, (parent, code)) in self.prefixes.iter().enumerate().skip(1) {
                    reached[i] = reached[*parent].and_then(|next| key.after(*code, next));
                }
                for (matched, terminal) in matched.iter_mut().zip(&self.terminals) {
                    *matched |= reached[*terminal].is_some();
                }
                if matched.iter().all(|m| *m) {
                    return true;
                }
            }
            false
        } else {
            keys.iter().any(|key| key.contains(&self.full))
        }
    }
    pub(crate) fn score(&self, keys: &[Target], weighted: bool) -> Option<f64> {
        let Self {
            full,
            searches,
            spaces,
            ..
        } = self;
        let spaces = *spaces;
        if full.codes.is_empty() {
            return None;
        }
        let union = keys.iter().fold(0, |all, key| all | key.flags);
        let required = if spaces {
            searches.iter().fold(0, |all, word| all | word.flags)
        } else {
            full.flags
        };
        if union & required != required {
            return None;
        }
        let mut best = vec![f64::NEG_INFINITY; searches.len()];
        let mut raw_keys = Vec::with_capacity(keys.len());
        for key in keys {
            if !spaces {
                raw_keys.push(algorithm(full, key, &key.next).map_or(f64::NEG_INFINITY, |r| r.0));
                continue;
            }
            let mut next = key.next.clone();
            let mut partial = vec![f64::NEG_INFINITY; searches.len()];
            let mut sum = 0.0;
            let mut previous = 0;
            let mut found = false;
            for (i, search) in searches.iter().enumerate() {
                let Some((raw, indexes)) = algorithm(search, key, &next) else {
                    continue;
                };
                found = true;
                if i + 1 != searches.len() && indexes.windows(2).all(|p| p[1] - p[0] == 1) {
                    let new_index = indexes.last().expect("matched") + 1;
                    let replace = next[new_index - 1];
                    for j in (0..new_index).rev() {
                        if next[j] != replace {
                            break;
                        }
                        next[j] = new_index;
                    }
                }
                partial[i] = raw / searches.len() as f64;
                sum += partial[i];
                if indexes[0] < previous {
                    sum -= ((previous - indexes[0]) * 2) as f64;
                }
                previous = indexes[0];
            }
            if !found {
                raw_keys.push(f64::NEG_INFINITY);
                continue;
            }
            if let Some((raw, _)) = algorithm(full, key, &key.next)
                && raw > sum
            {
                sum = raw;
                partial.fill(raw / searches.len() as f64);
            }
            for (best, score) in best.iter_mut().zip(partial) {
                combine(best, score);
            }
            raw_keys.push(sum);
        }
        if spaces && best.contains(&f64::NEG_INFINITY)
            || !spaces && raw_keys.iter().all(|s| *s == f64::NEG_INFINITY)
        {
            return None;
        }
        if weighted {
            Some(
                raw_keys
                    .iter()
                    .enumerate()
                    .map(|(i, s)| normalize(*s) * if i == 0 { 2.0 } else { 1.0 })
                    .sum(),
            )
        } else {
            let raw = if spaces {
                best.iter().sum()
            } else {
                let mut best = f64::NEG_INFINITY;
                for score in raw_keys {
                    combine(&mut best, score);
                }
                best
            };
            Some(normalize(raw))
        }
    }
}

#[cfg(test)]
fn score(query: &str, keys: &[&str], weighted: bool) -> Option<f64> {
    Query::new(query).score(
        &keys.iter().map(|key| Target::new(key)).collect::<Vec<_>>(),
        weighted,
    )
}

fn combine(best: &mut f64, score: f64) {
    if score > -1000.0 && *best > f64::NEG_INFINITY {
        *best = best.max((*best + score) / 4.0);
    }
    *best = best.max(score);
}

/// The original's priority queue is deliberately not a stable sort on ties.
pub(crate) fn rank(scores: impl Iterator<Item = (usize, f64)>) -> Vec<usize> {
    let mut heap: Vec<(usize, f64)> = Vec::new();
    for entry in scores {
        let mut i = heap.len();
        heap.push(entry);
        while i > 0 && entry.1 < heap[(i - 1) / 2].1 {
            heap[i] = heap[(i - 1) / 2];
            i = (i - 1) / 2;
        }
        heap[i] = entry;
    }
    let mut results = Vec::with_capacity(heap.len());
    while !heap.is_empty() {
        results.push(heap[0].0);
        let last = heap.pop().expect("nonempty");
        if heap.is_empty() {
            break;
        }
        let mut i = 0;
        let mut child = 1;
        while child < heap.len() {
            if child + 1 < heap.len() && heap[child + 1].1 < heap[child].1 {
                child += 1;
            }
            heap[i] = heap[child];
            i = child;
            child = 1 + i * 2;
        }
        while i > 0 && last.1 < heap[(i - 1) / 2].1 {
            heap[i] = heap[(i - 1) / 2];
            i = (i - 1) / 2;
        }
        heap[i] = last;
    }
    results.reverse();
    results
}

#[cfg(test)]
mod tests {
    #[test]
    fn pinned_external_oracle_scores_and_order() {
        // Original 3.1.0 additionally returns [] for a word made only of an
        // accent that normalization removes (checked through the isolated oracle).
        for query in ["abc \u{301}", "\u{301} abc"] {
            let keys = [super::Target::new("abc"), super::Target::new("a b c")];
            assert!(!super::Query::new(query).matches(&keys));
            assert!(super::Query::new(query).score(&keys, false).is_none());
        }
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../assets/fuzzysort-oracle.json")).unwrap();
        for case in cases["cases"].as_array().unwrap() {
            let query = case["query"].as_str().unwrap();
            let weighted = case["weighted"].as_bool().unwrap();
            let threshold = case["threshold"].as_f64().unwrap();
            let scores: Vec<_> = cases["options"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter_map(|(i, keys)| {
                    let keys: Vec<_> = keys
                        .as_array()
                        .unwrap()
                        .iter()
                        .take(if weighted { 3 } else { 2 })
                        .map(|k| k.as_str().unwrap())
                        .collect();
                    let prepared: Vec<_> = keys.iter().map(|key| super::Target::new(key)).collect();
                    assert_eq!(
                        super::Query::new(query).matches(&prepared),
                        super::score(query, &keys, weighted).is_some(),
                        "membership {query:?}"
                    );
                    super::score(query, &keys, weighted)
                        .filter(|s| *s >= threshold)
                        .map(|s| (i, s))
                })
                .collect();
            let ranked = super::rank(scores.iter().copied());
            let expected: Vec<_> = case["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["index"].as_u64().unwrap() as usize)
                .collect();
            assert_eq!(ranked, expected, "query={query:?} weighted={weighted}");
            for result in case["results"].as_array().unwrap() {
                let i = result["index"].as_u64().unwrap() as usize;
                let expected = result["score"].as_f64().unwrap();
                let actual = scores.iter().find(|(index, _)| *index == i).unwrap().1;
                assert!(
                    (actual - expected).abs() < 1e-10,
                    "query={query:?} index={i} {actual} != {expected}"
                );
            }
        }
    }
}
