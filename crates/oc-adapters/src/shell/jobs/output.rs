//! Stream admission owned by the existing Jobs Capture. No queue or worker:
//! the two drain readers serialize safe text publication and bounded backpressure.
//! Per-stream carry can defer text past the other stream's earlier publication.
use super::*;
use crate::storage::tool_output::{CaptureState, Resource, ShellStreams, Writer};
use crate::tools::output::StreamRedactor;

pub(super) const RECENT_CAP: usize = 65536;

struct TextStream {
    utf8: Vec<u8>,
    ansi: anstyle_parse::Parser,
    redactor: StreamRedactor,
    bytes: u64,
    newlines: u64,
    terminal_newline: bool,
    suppress: bool,
    first_read: Option<u64>,
    last_read: Option<u64>,
    carry_releases: u64,
}
impl TextStream {
    fn new(secrets: Vec<String>) -> Self {
        Self {
            utf8: Vec::with_capacity(4),
            ansi: Default::default(),
            redactor: StreamRedactor::new(secrets),
            bytes: 0,
            newlines: 0,
            terminal_newline: false,
            suppress: false,
            first_read: None,
            last_read: None,
            carry_releases: 0,
        }
    }
    fn admit(&mut self, bytes: &[u8], end: bool) -> String {
        if self.suppress {
            return String::new();
        }
        let carried = !self.utf8.is_empty() || self.redactor.has_pending();
        // At most one drain read plus a three-byte UTF-8 carry. Invalid sequences
        // normalize to U+FFFD; valid characters split across reads stay intact.
        self.utf8.extend_from_slice(bytes);
        let mut normalized = String::new();
        let mut at = 0;
        while at < self.utf8.len() {
            match std::str::from_utf8(&self.utf8[at..]) {
                Ok(text) => {
                    normalized.push_str(text);
                    at = self.utf8.len();
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    normalized.push_str(
                        std::str::from_utf8(&self.utf8[at..at + valid]).expect("valid prefix"),
                    );
                    at += valid;
                    match error.error_len() {
                        Some(n) => {
                            normalized.push('\u{fffd}');
                            at += n;
                        }
                        None if end => {
                            normalized.push('\u{fffd}');
                            at = self.utf8.len();
                        }
                        None => break,
                    }
                }
            }
        }
        self.utf8.drain(..at);
        // Keep parser state across drain reads; CSI/OSC/DCS payload is never
        // plain text or a terminal capability. `core` fixes OSC retention at 1KiB.
        struct Plain(String);
        impl anstyle_parse::Perform for Plain {
            fn print(&mut self, c: char) {
                if !c.is_control() {
                    self.0.push(c);
                }
            }
            fn execute(&mut self, byte: u8) {
                if matches!(byte, b'\n' | b'\r' | b'\t') {
                    self.0.push(byte as char);
                }
            }
        }
        let mut plain = Plain(String::new());
        for c in normalized.chars() {
            // This parser's state machine uses the 7-bit escape spelling. UTF-8
            // encoded C1 introducers denote the same control, not visible text.
            let spelling: Option<&[u8]> = match c {
                '\u{90}' => Some(b"\x1bP"),
                '\u{98}' => Some(b"\x1bX"),
                '\u{9b}' => Some(b"\x1b["),
                '\u{9c}' => Some(b"\x1b\\"),
                '\u{9d}' => Some(b"\x1b]"),
                '\u{9e}' => Some(b"\x1b^"),
                '\u{9f}' => Some(b"\x1b_"),
                _ => None,
            };
            let mut utf8 = [0; 4];
            for &byte in spelling.unwrap_or_else(|| c.encode_utf8(&mut utf8).as_bytes()) {
                self.ansi.advance(&mut plain, byte);
            }
        }
        let admitted = self.redactor.push(&plain.0, end);
        if carried && !admitted.is_empty() {
            self.carry_releases = self.carry_releases.saturating_add(1);
        }
        self.bytes = self.bytes.saturating_add(admitted.len() as u64);
        self.newlines = self
            .newlines
            .saturating_add(admitted.bytes().filter(|b| *b == b'\n').count() as u64);
        if !admitted.is_empty() {
            self.terminal_newline = admitted.ends_with('\n');
        }
        admitted
    }
    fn lines(&self) -> u64 {
        self.newlines + u64::from(self.bytes > 0 && !self.terminal_newline)
    }
}

pub(super) struct StreamCapture {
    writer: Option<Writer>,
    resource: Option<Resource>,
    stdout: TextStream,
    stderr: TextStream,
    last: Option<Stream>,
    tail: String,
    // Recent plain-text display in safe publication order, without transport
    // labels. Transcript previews borrow only their existing 2KiB suffix; an open
    // output dialog may consume this bounded 64KiB window from the same owner.
    display_tail: String,
    observed: u64,
    lost: bool,
    failure: Option<CaptureState>,
    interrupted: bool,
    admitted_bytes: u64,
    admitted_newlines: u64,
    last_newline: bool,
}
impl StreamCapture {
    pub(super) fn new() -> Self {
        Self {
            writer: None,
            resource: None,
            stdout: TextStream::new(Vec::new()),
            stderr: TextStream::new(Vec::new()),
            last: None,
            tail: String::new(),
            display_tail: String::new(),
            observed: 0,
            lost: false,
            failure: None,
            interrupted: false,
            admitted_bytes: 0,
            admitted_newlines: 0,
            last_newline: false,
        }
    }
    pub(super) fn begin(&mut self, db: &Db, p: &Provenance, secrets: Vec<String>) {
        // Reuse the writer's credential-admission bound before compiling carries.
        // If it cannot be honored, drain/count raw bytes but publish no unsafe text.
        if secrets
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()))
            .is_none_or(|n| n > 65536)
        {
            self.lost = true;
            self.failure = Some(CaptureState::Io);
            self.stdout.suppress = true;
            self.stderr.suppress = true;
            return;
        }
        self.stdout = TextStream::new(secrets.clone());
        self.stderr = TextStream::new(secrets.clone());
        match db.begin_tool_output(
            &p.operation,
            &p.session,
            &p.location,
            p.generation,
            &p.output_source,
            Vec::new(),
        ) {
            Ok(writer) => self.writer = Some(writer),
            Err(error) => {
                self.lost = true;
                self.failure = Some(failure_state(&error));
            }
        }
    }
    pub(super) fn ingest(
        &mut self,
        stream: Stream,
        bytes: &[u8],
        end: bool,
        complete: bool,
    ) -> String {
        if !bytes.is_empty() {
            self.observed = self.observed.saturating_add(1);
        }
        if end && !complete {
            self.interrupted = true;
        }
        let input = match stream {
            Stream::Stdout => &mut self.stdout,
            Stream::Stderr => &mut self.stderr,
        };
        if !bytes.is_empty() {
            input.first_read.get_or_insert(self.observed);
            input.last_read = Some(self.observed);
        }
        let admitted = input.admit(bytes, end);
        if !admitted.is_empty() {
            self.display_tail.push_str(&admitted);
            let drop = self
                .display_tail
                .ceil_char_boundary(self.display_tail.len().saturating_sub(RECENT_CAP));
            self.display_tail.drain(..drop);
            let mut framed = String::new();
            if self.last != Some(stream) {
                if self.last.is_some() && !self.tail.ends_with('\n') {
                    framed.push('\n');
                }
                framed.push_str(match stream {
                    Stream::Stdout => "[stdout]\n",
                    Stream::Stderr => "[stderr]\n",
                });
                self.last = Some(stream);
            }
            framed.push_str(&admitted);
            self.admitted_bytes = self.admitted_bytes.saturating_add(framed.len() as u64);
            self.admitted_newlines = self
                .admitted_newlines
                .saturating_add(framed.bytes().filter(|b| *b == b'\n').count() as u64);
            self.last_newline = framed.ends_with('\n');
            self.tail.push_str(&framed);
            let drop = self
                .tail
                .ceil_char_boundary(self.tail.len().saturating_sub(RECENT_CAP));
            self.tail.drain(..drop);
            let facts = self.facts();
            if let Some(writer) = self.writer.as_mut() {
                writer.shell_facts(facts);
                if let Err(error) = writer.append(&framed) {
                    self.lost = true;
                    let state = match error {
                        StorageError::Sqlite(_) => CaptureState::RegisterFailure,
                        _ => CaptureState::Io,
                    };
                    self.failure = Some(state);
                    self.resource = self.writer.take().expect("writer").abandon(state);
                }
            }
        }
        admitted
    }
    pub(super) fn facts(&self) -> ShellStreams {
        ShellStreams {
            stdout_bytes: self.stdout.bytes,
            stdout_lines: self.stdout.lines(),
            stderr_bytes: self.stderr.bytes,
            stderr_lines: self.stderr.lines(),
            observed_chunks: self.observed,
            format:
                "stdout/stderr labels: serialized normalized/redacted publication; carry can defer text past other raw reads; first/last_read are nonempty Capture ingress sequences, not OS emission order"
                    .into(),
            stdout_first_read: self.stdout.first_read,
            stdout_last_read: self.stdout.last_read,
            stderr_first_read: self.stderr.first_read,
            stderr_last_read: self.stderr.last_read,
            stdout_carry_releases: self.stdout.carry_releases,
            stderr_carry_releases: self.stderr.carry_releases,
        }
    }
    pub(super) fn display(&self) -> (String, bool) {
        let bytes = self.stdout.bytes.saturating_add(self.stderr.bytes);
        (
            self.display_tail.clone(),
            bytes > self.display_tail.len() as u64,
        )
    }
    pub(super) fn finish(
        &mut self,
        db: &Db,
        p: &Provenance,
        interrupted: bool,
    ) -> (String, Option<Resource>, bool) {
        if let Some(mut writer) = self.writer.take() {
            writer.shell_facts(self.facts());
            let state = if interrupted || self.interrupted {
                CaptureState::Interrupted
            } else {
                CaptureState::Complete
            };
            match writer.finish_state(state) {
                Ok(resource) => self.resource = Some(resource),
                Err(error) => {
                    self.lost = true;
                    self.failure = Some(failure_state(&error));
                    // Final rename/registration may have failed. Only an actually
                    // reopenable registered prefix may be advertised.
                    self.resource = db
                        .output_for_operation(&p.operation)
                        .ok()
                        .flatten()
                        .and_then(|r| {
                            db.open_tool_output(&p.session, &r.path)
                                .ok()
                                .map(|reader| reader.resource.clone())
                        });
                }
            }
        }
        if db
            .shell_output_facts(
                &p.operation,
                self.facts(),
                self.admitted_bytes,
                self.admitted_newlines + u64::from(self.admitted_bytes > 0 && !self.last_newline),
                self.failure,
            )
            .is_err()
        {
            self.lost = true;
            self.failure = Some(CaptureState::RegisterFailure);
        }
        self.resource = db
            .output_for_operation(&p.operation)
            .ok()
            .flatten()
            .and_then(|r| {
                db.open_tool_output(&p.session, &r.path)
                    .ok()
                    .map(|reader| reader.resource.clone())
            });
        if self.resource.is_none() {
            self.lost = true;
            self.failure.get_or_insert(CaptureState::Io);
        }
        if self.lost
            && self
                .resource
                .as_ref()
                .is_some_and(|r| r.state == CaptureState::Active)
        {
            self.resource = None;
        }
        let failed = self.lost
            || self.resource.as_ref().is_some_and(|r| {
                matches!(
                    r.state,
                    CaptureState::Quota
                        | CaptureState::ArtifactCap
                        | CaptureState::Io
                        | CaptureState::RegisterFailure
                )
            });
        (
            std::mem::take(&mut self.tail),
            self.resource.clone(),
            failed,
        )
    }
    pub(super) fn failure(&self) -> Option<CaptureState> {
        self.failure.or_else(|| {
            self.resource.as_ref().map(|r| r.state).filter(|s| {
                matches!(
                    s,
                    CaptureState::Quota
                        | CaptureState::ArtifactCap
                        | CaptureState::Io
                        | CaptureState::RegisterFailure
                )
            })
        })
    }
}

fn failure_state(error: &StorageError) -> CaptureState {
    match error {
        StorageError::Sqlite(_) => CaptureState::RegisterFailure,
        StorageError::StorageFull => CaptureState::Quota,
        _ => CaptureState::Io,
    }
}

pub(super) fn retain(state: &mut DrainState, text: &str) {
    let mut bytes = String::from_utf8(std::mem::take(&mut state.bytes)).expect("admitted UTF-8");
    bytes.push_str(text);
    let drop = bytes.ceil_char_boundary(bytes.len().saturating_sub(RECENT_CAP));
    state.truncated |= drop > 0;
    bytes.drain(..drop);
    state.bytes = bytes.into_bytes();
}

impl Capture {
    /// Read only safe, bounded projections under the ingress lock. Never clone
    /// the 64 KiB stream windows or expose a secret/UTF-8 carry before admission.
    pub(super) fn presentation(&self) -> Box<oc_core::tool_output::Presentation> {
        let stream = self.stream.lock().expect("stream admission");
        let out = self.stdout.lock().expect("stdout capture");
        let err = self.stderr.lock().expect("stderr capture");
        let cap = (oc_core::tool_output::PREVIEW_BYTES - 128) / 2;
        let stdout = recent(
            std::str::from_utf8(&out.bytes).expect("admitted UTF-8"),
            cap,
        );
        let stderr = recent(
            std::str::from_utf8(&err.bytes).expect("admitted UTF-8"),
            cap,
        );
        let mut presentation = oc_core::tool_output::Presentation::new(
            &recent(&stream.display_tail, oc_core::tool_output::PREVIEW_BYTES),
            stream.stdout.bytes.saturating_add(stream.stderr.bytes),
            false,
        );
        presentation.shell = Some(oc_core::tool_output::Shell {
            stdout_limited: stream.stdout.bytes > stdout.len() as u64,
            stderr_limited: stream.stderr.bytes > stderr.len() as u64,
            stdout,
            stderr,
            exit: None,
            signal: None,
            timed_out: false,
            cancelled: false,
        });
        Box::new(presentation)
    }
}
