//! Common admitted text preparation. Pinned OC2 tool-output.ts counting, with
//! native typed envelopes, served/capture/quota limits and no truncated bypass.
use crate::storage::{
    Db, StorageError,
    tool_output::{CaptureState, Resource},
};
use serde::{Deserialize, Serialize};

pub const SERVED_CAP: usize = 65536;
pub(crate) const NOTICE_RESERVE: usize = 8192;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Limits {
    pub max_lines: usize,
    pub max_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_lines: 2000,
            max_bytes: 51200,
        }
    }
}
impl Limits {
    pub(crate) fn parse(value: &serde_json::Value) -> Result<Self, crate::config::ConfigError> {
        let invalid = |field: &str| crate::config::ConfigError::Invalid {
            field: format!("tool_output{field}"),
            reason: "expected positive integer limits; max_bytes must not exceed 65536".into(),
        };
        let object = value.as_object().ok_or_else(|| invalid(""))?;
        if object
            .keys()
            .any(|k| !matches!(k.as_str(), "max_lines" | "max_bytes"))
        {
            return Err(invalid(""));
        }
        let mut limits = Self::default();
        for (key, target) in [
            ("max_lines", &mut limits.max_lines),
            ("max_bytes", &mut limits.max_bytes),
        ] {
            if let Some(value) = object.get(key) {
                *target = value
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| invalid(&format!(".{key}")))?;
            }
        }
        if limits.max_bytes > SERVED_CAP {
            return Err(invalid(".max_bytes"));
        }
        Ok(limits)
    }
    pub(crate) fn bytes(self) -> usize {
        self.max_bytes.min(SERVED_CAP - NOTICE_RESERVE)
    }
}

pub(crate) fn lines(text: &str) -> u64 {
    text.bytes().filter(|b| *b == b'\n').count() as u64
        + u64::from(!text.is_empty() && !text.ends_with('\n'))
}

/// No line-index Vec, no full-payload copy. Both ends clip a huge lone line on
/// UTF-8 boundaries; marker is in the independent reserved notice allowance.
pub(crate) fn preview(text: &str, limits: Limits, tail: bool) -> (&str, bool) {
    if text.len() <= limits.max_bytes.min(SERVED_CAP) && lines(text) <= limits.max_lines as u64 {
        return (text, false);
    }
    let bytes = limits.bytes();
    if text.len() <= bytes && lines(text) <= limits.max_lines as u64 {
        return (text, false);
    }
    if !tail {
        let mut end = 0;
        for (index, line) in text.split_inclusive('\n').enumerate() {
            if index >= limits.max_lines {
                break;
            }
            if end + line.len() > bytes {
                if end == 0 {
                    end = line.floor_char_boundary(bytes);
                }
                break;
            }
            end += line.len();
        }
        (&text[..end], true)
    } else {
        let end = text.len();
        let without_terminal = text.strip_suffix('\n').unwrap_or(text);
        let mut start = end;
        for (n, line) in without_terminal.rsplit('\n').enumerate() {
            if n >= limits.max_lines {
                break;
            }
            let candidate = if n == 0 {
                without_terminal.len() - line.len()
            } else {
                start.saturating_sub(line.len() + 1)
            };
            if end - candidate > bytes {
                if n == 0 {
                    start = text.ceil_char_boundary(end.saturating_sub(bytes));
                }
                break;
            }
            start = candidate;
        }
        (&text[start..], true)
    }
}

pub(crate) struct Prepared {
    pub text: String,
    pub logging_failed: bool,
    pub presentation: Option<Box<oc_core::tool_output::Presentation>>,
}

impl Prepared {
    fn plain(text: String, logging_failed: bool) -> Self {
        let presentation = oc_core::tool_output::Presentation::new(&text, text.len() as u64, false);
        Self {
            text,
            logging_failed,
            presentation: Some(Box::new(presentation)),
        }
    }
}

/// Shared incremental literal redaction. Callers supply bounded UTF-8 chunks;
/// the unpublished suffix protects secrets split across producer reads.
pub(crate) struct StreamRedactor {
    pending: String,
    secrets: Vec<String>,
    prefixes: Vec<Vec<usize>>,
    matched: Vec<usize>,
}
impl StreamRedactor {
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn new(secrets: Vec<String>) -> Self {
        let secrets: Vec<String> = secrets.into_iter().filter(|s| !s.is_empty()).collect();
        let prefixes = secrets
            .iter()
            .map(|secret| {
                let bytes = secret.as_bytes();
                let mut prefix = vec![0; bytes.len()];
                for at in 1..bytes.len() {
                    let mut n = prefix[at - 1];
                    while n > 0 && bytes[at] != bytes[n] {
                        n = prefix[n - 1];
                    }
                    if bytes[at] == bytes[n] {
                        n += 1;
                    }
                    prefix[at] = n;
                }
                prefix
            })
            .collect();
        let matched = vec![0; secrets.len()];
        Self {
            pending: String::new(),
            secrets,
            prefixes,
            matched,
        }
    }
    pub(crate) fn push(&mut self, text: &str, final_flush: bool) -> String {
        self.pending.push_str(text);
        // Incremental prefix matching is linear per known secret, including
        // repetitive long keys. Only a possible secret suffix needs withholding;
        // benign live output must not wait for an unrelated future read or EOF.
        for ((secret, prefix), matched) in self
            .secrets
            .iter()
            .zip(&self.prefixes)
            .zip(&mut self.matched)
        {
            let pattern = secret.as_bytes();
            for byte in text.bytes() {
                while *matched > 0 && pattern[*matched] != byte {
                    *matched = prefix[*matched - 1];
                }
                if pattern[*matched] == byte {
                    *matched += 1;
                }
                if *matched == pattern.len() {
                    *matched = prefix[*matched - 1];
                }
            }
        }
        let hold = self.matched.iter().copied().max().unwrap_or(0);
        let mut end = if final_flush {
            self.pending.len()
        } else {
            self.pending
                .floor_char_boundary(self.pending.len().saturating_sub(hold))
        };
        loop {
            let before = end;
            for secret in &self.secrets {
                for (at, _) in self.pending.match_indices(secret) {
                    if at < end && at + secret.len() > end {
                        end = at;
                    }
                }
            }
            if end == before || end == 0 {
                break;
            }
        }
        let mut admitted = self.pending[..end].to_owned();
        redact_string(&mut admitted, &self.secrets);
        self.pending.drain(..end);
        admitted
    }
}

#[derive(Clone)]
pub(crate) struct Context<'a> {
    pub db: &'a Db,
    pub operation: &'a str,
    pub session: &'a str,
    pub location: &'a str,
    pub generation: u64,
    pub source: &'a str,
    pub limits: Limits,
    pub secrets: Vec<String>,
}

impl Context<'_> {
    pub(crate) fn prepare_shell(
        &self,
        mut text: String,
        already_prepared: bool,
        existing: Option<&Resource>,
    ) -> Prepared {
        // ONLY the native Jobs outcome written by this common preparer supplies
        // this flag, never producer JSON/truncated metadata. Preserve its typed
        // exit/cancel/error envelope and do not retry a publication failure.
        if already_prepared {
            if text.len() > SERVED_CAP {
                return Prepared::plain("error: native shell control envelope exceeds served budget; actual execution recorded; no replay".into(), true);
            }
            return Prepared {
                text,
                logging_failed: false,
                // Legacy prepared shell outcomes have no reliable body boundary.
                // The native shell owner supplies its own typed facts separately.
                presentation: None,
            };
        }
        if text.starts_with("exit ")
            && let Some(end) = text.find('\n')
        {
            let control = text[..end + 1].to_owned();
            text.drain(..end + 1);
            let mut bounded = self.clone();
            bounded.limits.max_bytes = bounded.limits.max_bytes.min(SERVED_CAP - control.len());
            let mut prepared = bounded.prepare(text, true, false, existing);
            if let Some(presentation) = &mut prepared.presentation {
                let body = format!("{control}{}", presentation.body);
                let bytes = presentation.body_bytes + control.len() as u64;
                let mut joined = oc_core::tool_output::Presentation::new(
                    &body,
                    bytes,
                    presentation.generated_guidance,
                );
                joined.producer_limited = presentation.producer_limited;
                joined.capture = presentation.capture.take();
                **presentation = joined;
            }
            return Prepared {
                text: format!("{control}{}", prepared.text),
                logging_failed: prepared.logging_failed,
                presentation: prepared.presentation,
            };
        }
        self.prepare_envelope(text, true, existing)
    }
    pub(crate) fn prepare_page(&self, mut text: String) -> Prepared {
        let body;
        let notice = serde_json::json!({"tool_output_config":{"source":self.source,"generation":self.generation}});
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) {
            redact_value(&mut value, &self.secrets);
            body = value.to_string();
            value["tool_output_config"] = notice["tool_output_config"].clone();
            text = value.to_string();
        } else {
            redact_string(&mut text, &self.secrets);
            body = text.clone();
            text.push_str(&format!(
                "\n[page config source {} generation {}]",
                self.source, self.generation
            ));
        }
        if text.len() > SERVED_CAP {
            return Prepared::plain(
                "error: page redaction/notice exceeds served budget; request a smaller limit"
                    .into(),
                true,
            );
        }
        Prepared {
            text,
            logging_failed: false,
            presentation: Some(Box::new(oc_core::tool_output::Presentation::new(
                &body,
                body.len() as u64,
                true,
            ))),
        }
    }
    pub(crate) fn prepare_question(&self, text: String) -> Result<Prepared, StorageError> {
        // Questions are admitted native control facts, not an arbitrary JSON
        // object's claimed media/control origin. A cold resource is the sole full
        // payload; the native presentation event carries only its provenance ref.
        let Some(mut result) =
            oc_core::question::QuestionResult::from_output("question", "completed", Some(&text))
        else {
            return Ok(self.prepare_envelope(text, false, None));
        };
        let mut changed = false;
        for question in &mut result.questions {
            changed |= redact_string(&mut question.question, &self.secrets);
            changed |= redact_string(&mut question.header, &self.secrets);
            for option in &mut question.options {
                changed |= redact_string(&mut option.label, &self.secrets);
                changed |= redact_string(&mut option.description, &self.secrets);
            }
        }
        for answer in result.answers.iter_mut().flatten() {
            changed |= redact_string(answer, &self.secrets);
        }
        let text = if changed {
            serde_json::to_string(&result).map_err(|_| StorageError::OperationNotFound)?
        } else {
            text
        };
        if !preview(&text, self.limits, false).1 {
            return Ok(Prepared::plain(text, false));
        }
        let prepared = self.prepare(text, false, false, None);
        if !prepared.logging_failed {
            self.db
                .record_tool_output_question(self.operation, self.session)?;
        }
        let preview = json_preview(prepared.text, 51200 - 1024, false);
        Ok(Prepared{text:serde_json::json!({"status":"answered","operationID":self.operation,"question_count":result.questions.len(),"answer_count":result.answers.len(),"preview":preview,"presentation":"typed answers available through the native registered resource owner"}).to_string(),logging_failed:prepared.logging_failed,presentation:prepared.presentation})
    }

    pub(crate) fn prepare(
        &self,
        mut text: String,
        tail: bool,
        producer_limited: bool,
        existing: Option<&Resource>,
    ) -> Prepared {
        // Existing bounded producers normalize before this boundary; literal
        // known secret replacement precedes both disk and hot projection.
        redact_string(&mut text, &self.secrets);
        let (body, truncated) = preview(&text, self.limits, tail);
        if !truncated
            && existing.is_none_or(|resource| {
                resource.state == CaptureState::Complete
                    && resource.admitted_bytes <= self.limits.max_bytes.min(SERVED_CAP) as u64
                    && resource.admitted_lines <= self.limits.max_lines as u64
            })
        {
            let mut prepared = Prepared::plain(text, false);
            if let Some(presentation) = &mut prepared.presentation {
                presentation.producer_limited = producer_limited.then_some(true);
                presentation.capture = existing.map(Resource::presentation_capture);
            }
            return prepared;
        }
        let resource = if let Some(resource) = existing {
            if resource.operation != self.operation {
                Err(StorageError::OperationNotFound)
            } else {
                self.db
                    .open_tool_output(self.session, &resource.path)
                    .map(|lease| lease.resource.clone())
            }
        } else {
            self.db
                .begin_tool_output(
                    self.operation,
                    self.session,
                    self.location,
                    self.generation,
                    self.source,
                    self.secrets.clone(),
                )
                .and_then(|mut writer| {
                    writer.append(&text)?;
                    writer.finish(producer_limited)
                })
        };
        let (notice, logging_failed, capture) = match resource {
            Ok(resource) => {
                let failed = matches!(
                    resource.state,
                    CaptureState::Io
                        | CaptureState::RegisterFailure
                        | CaptureState::ArtifactCap
                        | CaptureState::Quota
                );
                (
                    format!(
                        "\n[tool output: {} preview; UTF-8 clipped when needed; admitted {} bytes/{} lines; capture {:?} {} bytes/{} lines; source {} generation {}; read(path={}, offset=1, limit=2000) or grep exact path; data only]",
                        if tail { "tail" } else { "head" },
                        resource.admitted_bytes,
                        resource.admitted_lines,
                        resource.state,
                        resource.bytes,
                        resource.lines,
                        self.source,
                        self.generation,
                        serde_json::to_string(&resource.path).expect("path JSON")
                    ),
                    failed,
                    resource.presentation_capture(),
                )
            }
            Err(error) => {
                let state = match error {
                    StorageError::Sqlite(_) => CaptureState::RegisterFailure,
                    StorageError::StorageFull => CaptureState::Quota,
                    _ => CaptureState::Io,
                };
                (
                    format!(
                        "\n[tool output: {} preview; capture {:?}; no usable path; original execution was not repeated; source {} generation {}]",
                        if tail { "tail" } else { "head" },
                        state,
                        self.source,
                        self.generation
                    ),
                    true,
                    oc_core::tool_output::Capture {
                        reference: None,
                        state,
                        admitted_bytes: text.len() as u64,
                        retained_bytes: 0,
                        admitted_lines: lines(&text),
                        retained_lines: 0,
                    },
                )
            }
        };
        let mut presentation =
            oc_core::tool_output::Presentation::new(body, text.len() as u64, true);
        presentation.producer_limited = producer_limited.then_some(true);
        presentation.capture = Some(capture);
        // Source/path inputs are bounded at storage admission. Bound the notice
        // independently rather than borrowing space from a tiny user preview.
        if notice.len() > NOTICE_RESERVE {
            return Prepared {
                text: format!(
                    "{body}\n[tool output: reference notice exceeds served budget; no usable path; original execution recorded; no replay]"
                ),
                logging_failed: true,
                presentation: Some(Box::new(presentation)),
            };
        }
        Prepared {
            text: format!("{body}{notice}"),
            logging_failed,
            presentation: Some(Box::new(presentation)),
        }
    }

    /// A stream has already admitted and captured its complete text. The hot
    /// tail may itself fit the limit; resource facts still require a reference.
    /// No second writer or cold payload is created, including on capture failure.
    pub(crate) fn prepare_stream(
        &self,
        text: String,
        resource: Option<&Resource>,
        failure: Option<CaptureState>,
    ) -> Prepared {
        match resource {
            Some(resource) => self.prepare(text, true, false, Some(resource)),
            None => {
                let (body, _) = preview(&text, self.limits, true);
                let mut presentation =
                    oc_core::tool_output::Presentation::new(body, text.len() as u64, true);
                presentation.capture = Some(oc_core::tool_output::Capture {
                    reference: None,
                    state: failure.unwrap_or(CaptureState::Io),
                    admitted_bytes: text.len() as u64,
                    retained_bytes: 0,
                    admitted_lines: lines(&text),
                    retained_lines: 0,
                });
                Prepared {
                    text: format!(
                        "{body}\n[tool output: tail preview; capture {:?}; no usable path; original execution was not repeated; source {} generation {}]",
                        failure.unwrap_or(CaptureState::Io),
                        self.source,
                        self.generation
                    ),
                    logging_failed: true,
                    presentation: Some(Box::new(presentation)),
                }
            }
        }
    }

    pub(crate) fn prepare_envelope(
        &self,
        mut text: String,
        tail: bool,
        existing: Option<&Resource>,
    ) -> Prepared {
        // Webfetch's independent response facts precede free text. Preserve the
        // small header, and move (not clone) its admitted body through limiting.
        if let Some((header, _)) = text.split_once("\n\n")
            && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(header)
            && value.get("final_url").is_some()
            && value.get("status").is_some()
        {
            let n = header.len() + 2;
            let header = if redact_value(&mut value, &self.secrets) {
                value.to_string()
            } else {
                header.to_owned()
            };
            text.drain(..n);
            if header.len() + NOTICE_RESERVE >= SERVED_CAP {
                return Prepared::plain("error: webfetch control envelope exceeds served budget; execution recorded; no replay".into(), true);
            }
            let mut bounded = self.clone();
            if text.len() + header.len() + 2 > SERVED_CAP || preview(&text, self.limits, tail).1 {
                bounded.limits.max_bytes = bounded
                    .limits
                    .max_bytes
                    .min(SERVED_CAP - header.len() - 2 - NOTICE_RESERVE);
            }
            let mut prepared = bounded.prepare(text, tail, value["truncated"] == true, existing);
            if header.len() + prepared.text.len() + 2 > SERVED_CAP {
                return Prepared::plain(
                    format!(
                        "{header}\n\nerror: served text budget exhausted after normalization; execution recorded; no replay"
                    ),
                    true,
                );
            }
            if let Some(presentation) = &mut prepared.presentation {
                let body = format!("{header}\n\n{}", presentation.body);
                let mut joined = oc_core::tool_output::Presentation::new(
                    &body,
                    presentation.body_bytes + header.len() as u64 + 2,
                    presentation.generated_guidance,
                );
                joined.producer_limited = value["truncated"].as_bool();
                joined.capture = presentation.capture.take();
                **presentation = joined;
            }
            return Prepared {
                text: format!("{header}\n\n{}", prepared.text),
                logging_failed: prepared.logging_failed,
                presentation: prepared.presentation,
            };
        }
        if !text.starts_with('{') {
            return self.prepare(text, tail, false, existing);
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return self.prepare(text, tail, false, existing);
        };
        if !value.is_object() {
            return self.prepare(text, tail, false, existing);
        }
        // Never slice serialized JSON. Free-text leaves share one logical joined
        // text budget; scalar status/exit/control/identity and typed effects survive.
        let changed = redact_value(&mut value, &self.secrets);
        if !preview(&text, self.limits, tail).1 {
            let limited = value["truncated"].as_bool();
            let mut prepared =
                Prepared::plain(if changed { value.to_string() } else { text }, false);
            if let Some(presentation) = &mut prepared.presentation {
                presentation.producer_limited = limited;
                presentation.capture = existing.map(Resource::presentation_capture);
            }
            return prepared;
        }
        drop(text);
        let body_bytes = value.to_string().len() as u64;
        let mut prose = String::new();
        collect_prose(&mut value, None, &mut prose);
        let producer_limited = value.get("truncated") == Some(&serde_json::Value::Bool(true));
        let mut prepared = self.prepare(prose, tail, producer_limited, existing);
        if let Some(presentation) = &mut prepared.presentation {
            let mut body_value = value.clone();
            let mut body = Some(presentation.body.clone());
            replace_prose(&mut body_value, None, &mut body);
            if let Some(body) = body {
                body_value["tool_output_preview"] = body.into();
            }
            let body = body_value.to_string();
            let mut envelope = oc_core::tool_output::Presentation::new(
                &body,
                body_bytes.max(body.len() as u64),
                presentation.generated_guidance,
            );
            envelope.producer_limited = value["truncated"].as_bool();
            envelope.capture = presentation.capture.take();
            **presentation = envelope;
        }
        let structural = value.to_string().len();
        let mut replacement = Some(json_preview(
            prepared.text,
            SERVED_CAP.saturating_sub(structural + 256),
            tail,
        ));
        replace_prose(&mut value, None, &mut replacement);
        if let Some(prose) = replacement {
            value["tool_output_preview"] = prose.into();
        }
        let result = value.to_string();
        if result.len() > SERVED_CAP {
            // Structural/typed fields themselves exceeded admission. Preserve
            // independent execution effects in the native typed outcome owner;
            // never publish a malformed or unlimited JSON result.
            return Prepared::plain(serde_json::json!({"error":"tool result envelope exceeds served budget", "execution_state":value.get("status"),"tool_output":"non-success; original effects are recorded; no replay"}).to_string(), true);
        }
        Prepared {
            text: result,
            logging_failed: prepared.logging_failed,
            presentation: prepared.presentation,
        }
    }
}

pub(crate) fn json_preview(text: String, budget: usize, tail: bool) -> String {
    if serde_json::to_string(&text).is_ok_and(|s| s.len() <= budget) {
        return text;
    }
    let (body, notice) = text
        .rsplit_once("\n[tool output:")
        .map_or((text.as_str(), String::new()), |(body, notice)| {
            (body, format!("\n[tool output:{notice}"))
        });
    let notice_bytes = serde_json::to_string(&notice).map_or(budget, |s| s.len());
    let available = budget.saturating_sub(notice_bytes + 64);
    let mut end = body.floor_char_boundary(body.len().min(available / 2));
    if tail {
        let start = body.ceil_char_boundary(body.len().saturating_sub(end));
        format!("{}[JSON text preview clipped]{notice}", &body[start..])
    } else {
        end = body.floor_char_boundary(end);
        format!("{}[JSON text preview clipped]{notice}", &body[..end])
    }
}

fn prose_key(key: Option<&str>) -> bool {
    matches!(
        key,
        Some(
            "text"
                | "output"
                | "stdout"
                | "stderr"
                | "body"
                | "content"
                | "description"
                | "message"
                | "diff"
        )
    )
}
fn collect_prose(value: &mut serde_json::Value, key: Option<&str>, out: &mut String) {
    match value {
        serde_json::Value::String(text) if prose_key(key) => {
            if out.is_empty() {
                *out = std::mem::take(text);
                return;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
            *text = String::new();
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_prose(item, key, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                collect_prose(value, Some(key), out);
            }
        }
        _ => {}
    }
}

fn redact_value(value: &mut serde_json::Value, secrets: &[String]) -> bool {
    match value {
        serde_json::Value::String(text) => redact_string(text, secrets),
        serde_json::Value::Array(values) => values.iter_mut().fold(false, |changed, value| {
            redact_value(value, secrets) || changed
        }),
        serde_json::Value::Object(map) => map.values_mut().fold(false, |changed, value| {
            redact_value(value, secrets) || changed
        }),
        _ => false,
    }
}
pub(crate) fn redact_string(text: &mut String, secrets: &[String]) -> bool {
    if !secrets
        .iter()
        .any(|s| !s.is_empty() && text.contains(s.as_str()))
    {
        return false;
    }
    let mut secrets = secrets.iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
    if secrets.is_empty() {
        return false;
    }
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let (mut changed, mut at) = (false, 0);
    while at < text.len() {
        if let Some(secret) = secrets
            .iter()
            .find(|secret| text[at..].starts_with(secret.as_str()))
        {
            text.replace_range(at..at + secret.len(), "[redacted]");
            at += "[redacted]".len();
            changed = true;
        } else {
            at += text[at..].chars().next().expect("UTF-8").len_utf8();
        }
    }
    changed
}
fn replace_prose(
    value: &mut serde_json::Value,
    key: Option<&str>,
    replacement: &mut Option<String>,
) {
    match value {
        serde_json::Value::String(text) if prose_key(key) => {
            *text = replacement.take().unwrap_or_default()
        }
        serde_json::Value::Array(items) => {
            for item in items {
                replace_prose(item, key, replacement);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                replace_prose(value, Some(key), replacement);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
