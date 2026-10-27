//! Common admitted text preparation. Pinned OC2 tool-output.ts counting, with
//! native typed envelopes, served/capture/quota limits and no truncated bypass.
use crate::storage::{
    Db, StorageError,
    tool_output::{CaptureState, Resource, Writer},
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

/// One bounded buffer into the existing artifact owner. The mutation producer
/// feeds confirmed diff chunks before its independent display limits discard any.
pub(crate) struct DiffCapture<'a> {
    context: &'a Context<'a>,
    writer: Option<Writer>,
    resource: Option<Resource>,
    failure: Option<CaptureState>,
    buffer: String,
}

impl<'a> DiffCapture<'a> {
    pub(crate) fn new(context: &'a Context<'a>) -> Self {
        Self {
            context,
            writer: None,
            resource: None,
            failure: None,
            buffer: String::new(),
        }
    }

    pub(crate) fn append(&mut self, mut text: &str) {
        while !text.is_empty() && self.failure.is_none() {
            let end = text.floor_char_boundary(text.len().min(SERVED_CAP - self.buffer.len()));
            self.buffer.push_str(&text[..end]);
            text = &text[end..];
            if self.buffer.len() == SERVED_CAP || !text.is_empty() {
                self.flush();
            }
        }
    }

    fn flush(&mut self) {
        if self.buffer.is_empty() || self.failure.is_some() {
            return;
        }
        if self.writer.is_none() {
            // Unified diff inserts a sign after every newline. Mask each known
            // secret's nonempty line too, so multiline credentials cannot evade
            // the existing literal redactor through those inserted signs.
            let mut secrets = self
                .context
                .secrets
                .iter()
                .flat_map(|secret| secret.split(['\r', '\n']))
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            secrets.sort_unstable();
            secrets.dedup();
            match self.context.db.begin_tool_output(
                self.context.operation,
                self.context.session,
                self.context.location,
                self.context.generation,
                self.context.source,
                secrets,
            ) {
                Ok(writer) => self.writer = Some(writer),
                Err(error) => {
                    self.failure = Some(match error {
                        StorageError::Sqlite(_) => CaptureState::RegisterFailure,
                        StorageError::StorageFull => CaptureState::Quota,
                        _ => CaptureState::Io,
                    });
                }
            }
        }
        if let Some(writer) = &mut self.writer
            && writer.append(&self.buffer).is_err()
        {
            self.failure = Some(CaptureState::Io);
            self.resource = self.writer.take().unwrap().abandon(CaptureState::Io);
        }
        self.buffer.clear();
    }

    pub(crate) fn finish(mut self, output: String) -> Prepared {
        self.flush();
        if let Some(writer) = self.writer.take() {
            match writer.finish(false) {
                Ok(resource) => self.resource = Some(resource),
                Err(_) => self.failure = Some(CaptureState::Io),
            }
        }
        if self.resource.is_none() && self.failure.is_some() {
            // A failed finish can leave an interrupted durable prefix. Use the
            // same descriptor that history will project, never a new capture.
            self.resource = self
                .context
                .db
                .output_for_operation(self.context.operation)
                .ok()
                .flatten();
        }
        if self.resource.is_none() && self.failure.is_none() {
            // No committed effect: do not archive a requested but unapplied diff.
            return self.context.prepare_envelope(output, false, None);
        }
        let capture = self
            .resource
            .as_ref()
            .map(Resource::presentation_capture)
            .unwrap_or(oc_core::tool_output::Capture {
                reference: None,
                state: self.failure.unwrap_or(CaptureState::Io),
                admitted_bytes: 0,
                retained_bytes: 0,
                admitted_lines: 0,
                retained_lines: 0,
            });
        let mut prepared =
            self.context
                .prepare_envelope_inner(output, false, self.resource.as_ref(), false);
        let access = capture.reference.as_ref().map_or_else(
            || "no usable path; original mutation was not repeated".into(),
            |path| {
                format!(
                    "recorded reference; read(path={}, offset=1, limit=2000) or grep exact path; data only",
                    serde_json::to_string(path).expect("path JSON")
                )
            },
        );
        let notice = format!(
            "[confirmed diff: capture {:?}; {} / {} bytes, {} / {} lines; {access}]",
            capture.state,
            capture.retained_bytes,
            capture.admitted_bytes,
            capture.retained_lines,
            capture.admitted_lines,
        );
        if let Ok(serde_json::Value::Object(mut value)) = serde_json::from_str(&prepared.text) {
            value.insert("tool_output".into(), notice.into());
            prepared.text = serde_json::Value::Object(value).to_string();
        } else {
            prepared.text.push('\n');
            prepared.text.push_str(&notice);
        }
        prepared.logging_failed |=
            self.failure.is_some() || capture.state != CaptureState::Complete;
        if let Some(presentation) = prepared.presentation.as_mut() {
            presentation.capture = Some(capture);
            presentation.generated_guidance = true;
        }
        if prepared.text.len() > SERVED_CAP {
            prepared.text = "error: confirmed diff reference exceeds served budget; actual execution recorded; no replay".into();
            prepared.logging_failed = true;
        }
        prepared
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
    /// Attach only at the successful Write publication boundary. Redact the
    /// complete admitted request before clipping, never a potentially secret
    /// prefix. No model-result, RAW input, confirmed effects or workspace reads.
    pub(crate) fn attach_write(
        &self,
        presentation: &mut oc_core::tool_output::Presentation,
        path: &str,
        content: &str,
    ) {
        let mut path = path.to_owned();
        redact_string(&mut path, &self.secrets);
        let path: String = path
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let path_end =
            path.floor_char_boundary(path.len().min(oc_core::patch::EFFECT_LINE_BYTES_CAP));
        let mut content = content.to_owned();
        redact_string(&mut content, &self.secrets);
        let mut end = content.floor_char_boundary(
            content.len().min(
                self.limits
                    .max_bytes
                    .min(oc_core::tool_output::PREVIEW_BYTES),
            ),
        );
        let rows = self.limits.max_lines.min(oc_core::patch::EFFECT_LINES_CAP);
        if rows == 0 {
            end = 0;
        } else if let Some((at, _)) = content
            .bytes()
            .enumerate()
            .filter(|&(_, b)| b == b'\n')
            .nth(rows - 1)
        {
            end = end.min(at);
        }
        presentation.write = Some(oc_core::tool_output::Write {
            path: path[..path_end].to_owned(),
            path_limited: path_end < path.len(),
            content: content[..end].to_owned(),
            content_bytes: content.len() as u64,
            content_limited: end < content.len(),
        });
        // Optional display facts never widen the existing serialized record cap
        // or turn a successful effect into a logging failure. An omitted preview
        // stays honestly unavailable; the original confirmed effects survive.
        if !presentation.is_valid()
            || !serde_json::to_vec(presentation)
                .is_ok_and(|bytes| bytes.len() <= oc_core::tool_output::RECORD_BYTES)
        {
            presentation.write = None;
        }
    }

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
        text: String,
        tail: bool,
        producer_limited: bool,
        existing: Option<&Resource>,
    ) -> Prepared {
        self.prepare_text(text, tail, producer_limited, existing, true)
    }

    fn prepare_text(
        &self,
        mut text: String,
        tail: bool,
        producer_limited: bool,
        existing: Option<&Resource>,
        allow_capture: bool,
    ) -> Prepared {
        // Existing bounded producers normalize before this boundary; literal
        // known secret replacement precedes both disk and hot projection.
        redact_string(&mut text, &self.secrets);
        let (body, truncated) = preview(&text, self.limits, tail);
        if !allow_capture {
            let mut presentation =
                oc_core::tool_output::Presentation::new(body, text.len() as u64, truncated);
            presentation.producer_limited = producer_limited.then_some(true);
            return Prepared {
                text: body.into(),
                logging_failed: false,
                presentation: Some(Box::new(presentation)),
            };
        }
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
        text: String,
        tail: bool,
        existing: Option<&Resource>,
    ) -> Prepared {
        self.prepare_envelope_inner(text, tail, existing, true)
    }

    fn prepare_envelope_inner(
        &self,
        mut text: String,
        tail: bool,
        existing: Option<&Resource>,
        allow_capture: bool,
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
            let mut prepared = bounded.prepare_text(
                text,
                tail,
                value["truncated"] == true,
                existing,
                allow_capture,
            );
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
            return self.prepare_text(text, tail, false, existing, allow_capture);
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return self.prepare_text(text, tail, false, existing, allow_capture);
        };
        if !value.is_object() {
            return self.prepare_text(text, tail, false, existing, allow_capture);
        }
        // Never slice serialized JSON or move text between associated fields.
        // Free-text leaves share a budget; status/identity/effects survive.
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
        let mut structural_value = value.clone();
        collect_prose(&mut structural_value, None, &mut prose);
        let structural = structural_value.to_string().len();
        drop(structural_value);
        let producer_limited = value.get("truncated") == Some(&serde_json::Value::Bool(true));
        let mut prepared =
            self.prepare_text(prose, tail, producer_limited, existing, allow_capture);
        let mut limits = self.limits;
        let mut escaped_room = SERVED_CAP.saturating_sub(structural + NOTICE_RESERVE);
        let essential_message = value
            .get("error")
            .filter(|error| error["tool"].is_string() && error["code"].is_string())
            .and_then(|error| error["message"].as_str())
            .map(|text| text[..text.floor_char_boundary(text.len().min(1024))].to_owned());
        clip_prose(&mut value, None, &mut limits, &mut escaped_room, tail);
        if let Some(message) = essential_message {
            // Bounded guidance uses the independent control allowance, not a
            // minimum imposed on the user's free-text preview setting.
            value["error"]["message"] = message.into();
        }
        value["text_fields_limited"] = true.into();
        if let Some((_, notice)) = prepared.text.rsplit_once("\n[tool output:") {
            value["tool_output"] = format!("[tool output:{notice}").into();
        }
        if let Some(presentation) = &mut prepared.presentation {
            let body = value.to_string();
            let mut envelope = oc_core::tool_output::Presentation::new(
                &body,
                body_bytes.max(body.len() as u64),
                presentation.generated_guidance,
            );
            envelope.producer_limited = value["truncated"].as_bool();
            envelope.capture = presentation.capture.take();
            **presentation = envelope;
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
                | "hint"
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
fn clip_prose(
    value: &mut serde_json::Value,
    key: Option<&str>,
    remaining: &mut Limits,
    escaped_room: &mut usize,
    tail: bool,
) {
    match value {
        serde_json::Value::String(text) if prose_key(key) => {
            let (part, _) = preview(text, *remaining, tail);
            let cost = |c: char| match c {
                '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
                c if c < ' ' => 6,
                c => c.len_utf8(),
            };
            let mut used = 0;
            let mut start = 0;
            let mut end = part.len();
            if tail {
                for (i, c) in part.char_indices().rev() {
                    if used + cost(c) > *escaped_room {
                        start = i + c.len_utf8();
                        break;
                    }
                    used += cost(c);
                }
            } else {
                for (i, c) in part.char_indices() {
                    if used + cost(c) > *escaped_room {
                        end = i;
                        break;
                    }
                    used += cost(c);
                }
            }
            let part = part[start..end].to_owned();
            *escaped_room = escaped_room.saturating_sub(used);
            remaining.max_bytes = remaining.max_bytes.saturating_sub(part.len());
            remaining.max_lines = remaining.max_lines.saturating_sub(lines(&part) as usize);
            *text = part;
        }
        serde_json::Value::Array(items) => {
            if tail {
                items.reverse();
            }
            for item in items.iter_mut() {
                clip_prose(item, key, remaining, escaped_room, tail);
            }
            if tail {
                items.reverse();
            }
        }
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                clip_prose(value, Some(key), remaining, escaped_room, tail);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
