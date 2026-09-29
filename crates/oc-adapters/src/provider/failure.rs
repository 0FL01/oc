//! Physical HTTP/Responses failure facts. Classification follows pinned
//! packages/ai/src/provider-error.ts; no transcript text enters this boundary.

use super::ProviderError;

/// Affirmatively observed provider failure category, in classifier precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    ContextOverflow,
    PayloadTooLarge,
    ContentPolicy,
    Quota,
    Authentication,
    RateLimit,
    ProviderInternal,
    InvalidRequest,
    UnknownProvider,
    Transport,
    IncompleteStream,
}

/// Request delivery facts, never an assertion of exactly-once effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    NotSent,
    Unknown,
    Rejected,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Request,
    Read,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Network,
    Deadline,
    IdleTimeout,
}

/// Only allowlisted, parsed header facts survive the response lifetime.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetryHeaders {
    pub should_retry: Option<bool>,
    pub retry_after_ms: Option<u64>,
}

impl RetryHeaders {
    pub(super) fn observed(headers: &reqwest::header::HeaderMap, failure: bool) -> Self {
        let bounded = |name| headers.get(name)?.to_str().ok().filter(|s| s.len() <= 128);
        let should_retry = match bounded("x-should-retry") {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        };
        let numeric = |value: &str, scale: f64| {
            let number = value.trim().parse::<f64>().ok()?;
            (number.is_finite() && number >= 0.0)
                .then(|| (number * scale).ceil().min(900_000.0) as u64)
        };
        let retry_after_ms = failure
            .then(|| {
                bounded("retry-after-ms")
                    .and_then(|s| numeric(s, 1.0))
                    .or_else(|| {
                        let value = bounded("retry-after")?;
                        numeric(value, 1000.0).or_else(|| http_date_delay(value))
                    })
            })
            .flatten();
        Self {
            should_retry,
            retry_after_ms,
        }
    }
}

/// Irreducible safe result of one physical request. No raw URL/body/header/code
/// or exception is retained. The optional message is bounded/redacted locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalFailure {
    pub kind: FailureKind,
    pub http_status: Option<u16>,
    pub event_status: Option<u16>,
    pub delivery: Delivery,
    pub operation: Operation,
    pub transport: Option<TransportKind>,
    pub output_committed: bool,
    pub headers: RetryHeaders,
    pub message: Option<String>,
}

impl PhysicalFailure {
    /// Default eligibility facts for the future finite runtime owner. This does
    /// not schedule, wait, or issue a request. Local failures have no such facts.
    pub fn retry_eligible(&self) -> bool {
        if self.kind == FailureKind::ContextOverflow {
            return false;
        }
        if matches!(
            self.kind,
            FailureKind::Transport | FailureKind::IncompleteStream
        ) {
            if self.output_committed && self.operation == Operation::Read {
                return true;
            }
            if let Some(retry) = self.headers.should_retry {
                return retry;
            }
            return matches!(self.delivery, Delivery::Unknown | Delivery::NotSent)
                || (self.delivery == Delivery::Accepted && self.operation == Operation::Read);
        }
        self.headers.should_retry.unwrap_or(matches!(
            self.kind,
            FailureKind::RateLimit | FailureKind::ProviderInternal | FailureKind::UnknownProvider
        ))
    }

    pub(super) fn transport(operation: Operation, delivery: Delivery, status: Option<u16>) -> Self {
        Self {
            kind: FailureKind::Transport,
            http_status: status,
            event_status: None,
            delivery,
            operation,
            output_committed: false,
            transport: Some(TransportKind::Network),
            headers: RetryHeaders::default(),
            message: None,
        }
    }
}

impl std::fmt::Display for PhysicalFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.kind {
            FailureKind::ContextOverflow => "context window exceeded",
            FailureKind::PayloadTooLarge => "payload too large",
            FailureKind::ContentPolicy => "content policy rejection",
            FailureKind::Quota => "quota exceeded",
            FailureKind::Authentication => "authentication rejected",
            FailureKind::RateLimit => "rate limited",
            FailureKind::ProviderInternal => "provider internal error",
            FailureKind::InvalidRequest => "invalid provider request",
            FailureKind::UnknownProvider => "unknown provider failure",
            FailureKind::Transport => "transport error",
            FailureKind::IncompleteStream => "incomplete stream",
        };
        write!(f, "{name}")?;
        if let Some(status) = self.http_status {
            write!(f, " (HTTP {status})")?;
        }
        if let Some(status) = self.event_status {
            write!(f, " (event {status})")?;
        }
        if let Some(message) = &self.message {
            write!(f, ": {message}")?;
        }
        Ok(())
    }
}

pub(super) fn classified(
    value: Option<&serde_json::Value>,
    status: Option<u16>,
    event: bool,
) -> PhysicalFailure {
    let mut codes: Vec<String> = [
        "/code",
        "/error_type",
        "/error/code",
        "/error/type",
        "/error/status",
        "/error/error_type",
        "/error/innererror/code",
        "/error/metadata/error_type",
        "/response/error/code",
        "/response/error_type",
        "/exception/type",
    ]
    .iter()
    .filter_map(|path| value?.pointer(path)?.as_str())
    .filter(|code| code.len() <= 128)
    .map(str::to_ascii_lowercase)
    .collect();
    let messages: Vec<&str> = [
        "/message",
        "/error/message",
        "/response/error/message",
        "/error",
        "/response/error",
    ]
    .iter()
    .filter_map(|path| value?.pointer(path)?.as_str())
    .filter(|message| message.len() <= 16 * 1024)
    .collect();
    // Exact donor gateway label, confined to structured provider messages.
    for message in &messages {
        if let Some((prefix, rest)) = message.split_once(": [")
            && !prefix.is_empty()
            && !prefix.contains([':', '\n'])
            && let Some((code, _)) = rest.split_once(']')
            && !code.is_empty()
            && code.len() <= 128
            && code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            codes.push(code.to_ascii_lowercase());
        }
    }
    let message = messages.join("\n").to_ascii_lowercase();
    let has = |allowed: &[&str]| codes.iter().any(|code| allowed.contains(&code.as_str()));
    let client = status.is_none_or(|s| (400..500).contains(&s));
    let policy_text = [
        "violating our usage policy",
        "blocked by content filtering policy",
        "rejected as a result of our safety system",
    ]
    .iter()
    .any(|s| message.contains(s))
        || source_signal(&message, "content", "policy");
    let kind = if client
        && (has(&[
            "context_length_exceeded",
            "model_context_window_exceeded",
            "request_too_large",
        ]) || context_text(&message))
    {
        FailureKind::ContextOverflow
    } else if status == Some(413)
        || [
            "request entity too large",
            "payload too large",
            "request too large",
        ]
        .iter()
        .any(|s| message.contains(s))
    {
        FailureKind::PayloadTooLarge
    } else if has(&[
        "content_filter",
        "responsibleaipolicyviolation",
        "content_policy_violation",
        "image_content_policy_violation",
        "refusal",
    ]) || (client && policy_text)
    {
        FailureKind::ContentPolicy
    } else if status == Some(402)
        || has(&[
            "insufficient_quota",
            "usage_not_included",
            "billing_error",
            "gousagelimiterror",
            "freeusagelimiterror",
            "creditlimitexceeded",
        ])
        || (status == Some(429)
            && (source_signal(&message, "insufficient", "quota")
                || source_signal(&message, "quota", "exceeded")
                || message.contains("budget exceeded")
                || message.contains("usage limit")))
    {
        FailureKind::Quota
    } else if matches!(status, Some(401 | 403))
        || has(&["authentication_error", "permission_error"])
    {
        FailureKind::Authentication
    } else if status == Some(429)
        || codes.iter().any(|c| {
            c.contains("rate_limit")
                || matches!(c.as_str(), "too_many_requests" | "throttlingexception")
        })
        || rate_text(&message)
    {
        FailureKind::RateLimit
    } else if matches!(status, Some(408 | 409 | 500..))
        || (status.is_none_or(|s| s < 400)
            && (has(&[
                "api_error",
                "internal_error",
                "internalserverexception",
                "modelstreamerrorexception",
                "overloaded_error",
                "server_error",
                "server_is_overloaded",
                "slow_down",
                "serviceunavailableexception",
            ]) || codes
                .iter()
                .any(|c| c.contains("exhausted") || c.contains("unavailable"))
                || (!has(&[
                    "invalid_prompt",
                    "invalid_request_error",
                    "validationexception",
                ]) && server_text(&message))))
    {
        FailureKind::ProviderInternal
    } else if has(&[
        "invalid_prompt",
        "invalid_request_error",
        "validationexception",
    ]) || status.is_some_and(|s| (400..500).contains(&s))
    {
        FailureKind::InvalidRequest
    } else {
        FailureKind::UnknownProvider
    };
    PhysicalFailure {
        kind,
        http_status: (!event).then_some(status).flatten(),
        event_status: event.then_some(status).flatten(),
        delivery: if event {
            Delivery::Accepted
        } else {
            Delivery::Rejected
        },
        operation: if event {
            Operation::Read
        } else {
            Operation::Request
        },
        transport: None,
        output_committed: false,
        headers: RetryHeaders::default(),
        message: None,
    }
}

pub(super) fn event_failure(value: &serde_json::Value) -> ProviderError {
    let status = value
        .get("status")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| value.get("status_code").and_then(serde_json::Value::as_u64))
        .and_then(|s| u16::try_from(s).ok());
    let mut failure = classified(Some(value), status, true);
    // Donor's bare error frame is affirmative ProviderInternal evidence.
    if value["type"] == "error"
        && value.get("error").is_none()
        && value.get("response").is_none()
        && value["message"].as_str().is_none()
        && value["code"].as_str().is_none()
        && status.is_none()
    {
        failure.kind = FailureKind::ProviderInternal;
    }
    ProviderError::Request(Box::new(failure))
}

fn source_signal(text: &str, first: &str, second: &str) -> bool {
    text.match_indices(first).any(|(index, _)| {
        let rest = &text[index + first.len()..];
        rest.starts_with(second)
            || rest.chars().next().is_some_and(|c| {
                (c == '-' || c == '_' || c.is_whitespace())
                    && rest[c.len_utf8()..].starts_with(second)
            })
    })
}

fn rate_text(text: &str) -> bool {
    text.contains("rate increased too quickly")
        || source_signal(text, "rate", "limit")
        || text.match_indices("too").any(|(index, _)| {
            let rest = &text[index + 3..];
            let rest = rest
                .chars()
                .next()
                .filter(|c| *c == '_' || c.is_whitespace())
                .map_or(rest, |c| &rest[c.len_utf8()..]);
            let Some(rest) = rest.strip_prefix("many") else {
                return false;
            };
            let rest = rest
                .chars()
                .next()
                .filter(|c| *c == '_' || c.is_whitespace())
                .map_or(rest, |c| &rest[c.len_utf8()..]);
            rest.starts_with("requests")
        })
}

fn server_text(text: &str) -> bool {
    // Literal alternatives of the donor SERVER_ERROR_TEXT, only in error.message.
    [
        "try again",
        "retry request",
        "retry the request",
        "retry this request",
        "retry your request",
        "try the request again",
        "try this request again",
        "try your request again",
        "try request again",
        "at capacity",
        "overloaded",
        "temporarily unavailable",
        "server busy",
        "server is busy",
        "provider returned an error",
        "provider returned error",
        "upstream connect",
        "upstream connection",
        "upstream request",
        "request buffer limit while retrying upstream",
    ]
    .iter()
    .any(|s| {
        text.match_indices(s).any(|(i, _)| {
            let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
            (i == 0 || !word(text.as_bytes()[i - 1]))
                && text.as_bytes().get(i + s.len()).is_none_or(|b| !word(*b))
        })
    }) || [
        ("service", "unavailable"),
        ("server", "error"),
        ("internal", "error"),
        ("resource", "exhausted"),
    ]
    .iter()
    .any(|(a, b)| {
        text.match_indices(a).any(|(index, _)| {
            let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
            if index > 0 && word(text.as_bytes()[index - 1]) {
                return false;
            }
            let rest = &text[index + a.len()..];
            let suffix = if let Some(s) = rest.strip_prefix(b) {
                s
            } else {
                let Some(c) = rest
                    .chars()
                    .next()
                    .filter(|c| *c == '-' || *c == '_' || c.is_whitespace())
                else {
                    return false;
                };
                let Some(s) = rest[c.len_utf8()..].strip_prefix(b) else {
                    return false;
                };
                s
            };
            suffix.as_bytes().first().is_none_or(|c| !word(*c))
        })
    })
}

fn context_text(text: &str) -> bool {
    if text.starts_with("throttling error:")
        || text.starts_with("service unavailable:")
        || text.contains("rate limit")
        || text.contains("too many requests")
    {
        return false;
    }
    for prefix in ["400", "413"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = rest.trim_start();
            let rest = rest
                .strip_prefix("status code")
                .unwrap_or(rest)
                .trim_start();
            if rest.starts_with("(no body)") {
                return true;
            }
        }
    }
    for prefix in [
        "exceeds maximum context length",
        "exceeds the maximum context length",
        "exceeds model's maximum context length",
        "exceeds the model's maximum context length",
        "exceeds models maximum context length",
        "exceeds the models maximum context length",
    ] {
        if text.match_indices(prefix).any(|(index, _)| {
            let rest = &text[index + prefix.len()..];
            source_pattern_anchored(rest, " of % token")
                || source_pattern_anchored(rest.trim_start(), "(%)")
        }) {
            return true;
        }
    }
    // The donor's fixed patterns: # = digits, % = digits/commas, * = same-line gap.
    // Optional literal groups are expanded, not inferred from arbitrary prose.
    let patterns = [
        "prompt is too long",
        "input is too long for requested model",
        "exceeds the context window",
        "input token count*exceeds the maximum",
        "tokens in request more than max tokens allowed",
        "maximum prompt length is #",
        "reduce the length of the messages",
        "maximum context length is # tokens",
        "exceeds maximum allowed input length of % token",
        "exceeds the maximum allowed input length of % token",
        "input (# tokens) is longer than the model's context length (# tokens)",
        "input (# tokens) is longer than the models context length (# tokens)",
        "exceeds the limit of #",
        "exceeds the available context size",
        "greater than the context length",
        "context window exceeds limit",
        "exceeded model token limit",
        "context_length_exceeded",
        "context length exceeded",
        "context length is only # tokens",
        "input length*exceeds*context length",
        "context_length exceeded",
        "context length_exceeded",
        "prompt too long; exceeded context length",
        "prompt too long; exceeded max context length",
        "too large for model with # maximum context length",
        "prompt has % token, but the configured context size is % token",
        "prompt has % tokens, but the configured context size is % token",
        "model_context_window_exceeded",
        "range of input length should be",
        "too many tokens",
        "token limit exceeded",
        "request_too_large",
    ];
    patterns.iter().any(|pattern| source_pattern(text, pattern))
}

fn source_pattern(text: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        // These donor gap patterns contain only fixed literals. Earliest-match
        // scanning avoids regex-style backtracking on adversarial error bodies.
        let mut pieces = pattern.split('*');
        let first = pieces.next().expect("fixed prefix");
        let suffix: Vec<_> = pieces.collect();
        return text.match_indices(first).any(|(i, _)| {
            let mut rest = &text[i + first.len()..];
            for piece in &suffix {
                let Some(index) = rest.find(piece) else {
                    return false;
                };
                if rest[..index].contains('\n') {
                    return false;
                }
                rest = &rest[index + piece.len()..];
            }
            true
        });
    }
    (0..text.len()).any(|i| source_pattern_bytes(&text.as_bytes()[i..], pattern.as_bytes()))
}

fn source_pattern_anchored(text: &str, pattern: &str) -> bool {
    source_pattern_bytes(text.as_bytes(), pattern.as_bytes())
}

fn source_pattern_bytes(text: &[u8], pattern: &[u8]) -> bool {
    match pattern.first() {
        None => true,
        Some(b'#' | b'%') => {
            let count = text
                .iter()
                .take_while(|b| b.is_ascii_digit() || (pattern[0] == b'%' && **b == b','))
                .count();
            count > 0 && source_pattern_bytes(&text[count..], &pattern[1..])
        }
        Some(b) => text.first() == Some(b) && source_pattern_bytes(&text[1..], &pattern[1..]),
    }
}

fn http_date_delay(value: &str) -> Option<u64> {
    // IMF-fixdate, RFC850 and asctime are the HTTP date forms. No raw date is retained.
    let value = value.replace([',', '-'], " ");
    let fields: Vec<_> = value.split_ascii_whitespace().collect();
    let (day, month, year, time) = match fields.as_slice() {
        [_, day, month, year, time, "GMT"] => (*day, *month, *year, *time),
        [_, month, day, time, year] => (*day, *month, *year, *time),
        _ => return None,
    };
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == month)? as i64
        + 1;
    let mut year: i64 = year.parse().ok()?;
    if year < 100 {
        year += if year < 70 { 2000 } else { 1900 };
    }
    let day: i64 = day.parse().ok()?;
    let days = [
        31,
        if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1970..=9999).contains(&year) || day < 1 || day > days[(month - 1) as usize] {
        return None;
    }
    let time: Vec<_> = time
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<_, _>>()
        .ok()?;
    let [hour, minute, second] = time.as_slice() else {
        return None;
    };
    if !(0..24).contains(hour) || !(0..60).contains(minute) || !(0..60).contains(second) {
        return None;
    }
    let y = year - i64::from(month <= 2);
    let era = y / 400;
    let yoe = y - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * m + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    let millis = (days * 86400 + hour * 3600 + minute * 60 + second) as u64 * 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some((u128::from(millis).saturating_sub(now)).min(900_000) as u64)
}
