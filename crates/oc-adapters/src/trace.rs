//! Bounded, redacted, per-launch startup trace.
//!
//! Diagnostics only: normal stdout/stderr and application behavior are never
//! changed, and every call is a no-op while tracing is inactive. Callers must
//! never pass environment values, header values, response bodies, config file
//! contents or credential-bearing URLs. Allowed fields are names, booleans,
//! counts, byte sizes, status codes, typed category/variant names, filesystem
//! paths, provider/model/agent ids and URL scheme+host+port+path.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use sha2::{Digest as _, Sha256};

/// Maximum bytes written per launch, including the truncation marker.
const CAP_BYTES: usize = 262_144;
/// Maximum characters accepted from one message.
const MESSAGE_CHARS: usize = 512;
/// Marker written once when the byte cap is reached.
const TRUNCATED: &[u8] = b"trace: truncated\n";
/// Opt-in fingerprint flag; only a digest prefix is ever appended.
const FINGERPRINT_ENV: &str = "OC_STARTUP_TRACE_FINGERPRINT";

struct Sink {
    writer: Option<File>,
    started: Instant,
    bytes: usize,
    truncated: bool,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();

fn lock_sink() -> MutexGuard<'static, Sink> {
    SINK.get_or_init(|| {
        Mutex::new(Sink {
            writer: None,
            started: Instant::now(),
            bytes: 0,
            truncated: false,
        })
    })
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Resolve and open the default trace file; returns its path when active.
///
/// Order: absolute `OC_STARTUP_TRACE`, absolute `XDG_STATE_HOME/oc`, then
/// `$HOME/.local/state/oc`. Any failure returns `None` silently: tracing must
/// never break startup.
pub fn init_default() -> Option<PathBuf> {
    let path = default_path()?;
    init_at(&path).ok()?;
    Some(path)
}

fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OC_STARTUP_TRACE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        return Some(path);
    }
    if let Some(root) = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
    {
        return Some(root.join("oc").join("startup-trace.log"));
    }
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".local/state/oc/startup-trace.log"))
}

/// Open `path` as the trace sink, creating parent directories.
///
/// If tracing is already initialized this does nothing and returns `Ok(())`.
/// The file is truncated per launch and created with mode 0600.
pub fn init_at(path: &Path) -> std::io::Result<()> {
    let mut sink = lock_sink();
    if sink.writer.is_some() {
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    let writer = options.open(path)?;
    sink.writer = Some(writer);
    sink.started = Instant::now();
    sink.bytes = 0;
    sink.truncated = false;
    emit(&mut sink, "trace", "started");
    Ok(())
}

/// Append one `+<elapsed_ms>ms pid=<pid> <stage>: <message>` line.
///
/// No-op while not initialized; each message is truncated to 512 characters
/// and forced onto a single line.
pub fn log(stage: &str, message: &str) {
    let Some(slot) = SINK.get() else {
        return;
    };
    let mut sink = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    emit(&mut sink, stage, message);
}

/// True while a trace sink is installed.
pub fn active() -> bool {
    SINK.get().is_some_and(|slot| {
        slot.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .writer
            .is_some()
    })
}

/// `"<name> present=<bool> empty=<bool>"`; absent values report `empty=false`.
///
/// When `OC_STARTUP_TRACE_FINGERPRINT=1` a nonempty value also contributes
/// `fingerprint=<first 8 hex of sha256(value)>`. The value itself is never
/// part of the result.
pub fn env_fact(name: &str, value: Option<&str>) -> String {
    let fingerprint = std::env::var_os(FINGERPRINT_ENV).is_some_and(|flag| flag == "1");
    env_fact_with(name, value, fingerprint)
}

fn env_fact_with(name: &str, value: Option<&str>, fingerprint: bool) -> String {
    let present = value.is_some();
    let empty = value.is_some_and(str::is_empty);
    let mut fact = format!("{name} present={present} empty={empty}");
    if fingerprint && let Some(value) = value.filter(|value| !value.is_empty()) {
        let digest = format!("{:x}", Sha256::digest(value.as_bytes()));
        fact.push_str(" fingerprint=");
        fact.push_str(&digest[..8]);
    }
    fact
}

fn emit(sink: &mut Sink, stage: &str, message: &str) {
    if sink.truncated {
        return;
    }
    let Some(writer) = sink.writer.as_mut() else {
        return;
    };
    let message: String = message
        .chars()
        .take(MESSAGE_CHARS)
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let line = format!(
        "+{}ms pid={} {stage}: {message}\n",
        sink.started.elapsed().as_millis(),
        std::process::id()
    );
    if sink.bytes + line.len() + TRUNCATED.len() > CAP_BYTES {
        let _ = writer.write_all(TRUNCATED);
        let _ = writer.flush();
        sink.bytes += TRUNCATED.len();
        sink.truncated = true;
        return;
    }
    if writer.write_all(line.as_bytes()).is_ok() && writer.flush().is_ok() {
        sink.bytes += line.len();
    } else {
        sink.truncated = true;
    }
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    let mut sink = lock_sink();
    sink.writer = None;
    sink.bytes = 0;
    sink.truncated = false;
    sink.started = Instant::now();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::Mutex as StdMutex;

    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn lock_tests() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn fresh(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        reset_for_tests();
        dir.path().join(name)
    }

    #[test]
    fn inactive_log_is_a_noop() {
        let _guard = lock_tests();
        reset_for_tests();
        assert!(!active());
        log("stage", "message");
    }

    #[test]
    fn init_at_writes_documented_lines() {
        let _guard = lock_tests();
        let dir = tempfile::tempdir().expect("temp");
        let path = fresh(&dir, "nested/startup-trace.log");
        init_at(&path).expect("init");
        assert!(active());
        log("stage.a", "message one");
        let text = std::fs::read_to_string(&path).expect("read");
        let pid = std::process::id();
        assert!(text.starts_with('+'), "{text}");
        assert!(
            text.contains(&format!(" pid={pid} trace: started\n")),
            "{text}"
        );
        assert!(
            text.contains(&format!(" pid={pid} stage.a: message one\n")),
            "{text}"
        );
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn byte_cap_writes_one_truncation_marker() {
        let _guard = lock_tests();
        let dir = tempfile::tempdir().expect("temp");
        let path = fresh(&dir, "cap.log");
        init_at(&path).expect("init");
        let message = "x".repeat(MESSAGE_CHARS);
        for _ in 0..1000 {
            log("cap", &message);
        }
        let len = std::fs::metadata(&path).expect("meta").len() as usize;
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(len <= CAP_BYTES, "{len}");
        assert!(text.ends_with("trace: truncated\n"), "{}", text.len());
        assert_eq!(text.matches("trace: truncated").count(), 1);
        log("cap", "after truncation");
        assert_eq!(std::fs::metadata(&path).expect("meta").len() as usize, len);
    }

    #[test]
    fn message_is_bounded_to_512_chars() {
        let _guard = lock_tests();
        let dir = tempfile::tempdir().expect("temp");
        let path = fresh(&dir, "bounded.log");
        init_at(&path).expect("init");
        log("m", &"y".repeat(600));
        let text = std::fs::read_to_string(&path).expect("read");
        let line = text.lines().nth(1).expect("line");
        let body = line.split_once(": ").expect("body").1;
        assert_eq!(body.chars().count(), MESSAGE_CHARS);
    }

    #[test]
    fn env_fact_never_contains_the_value() {
        assert_eq!(
            env_fact_with("TOKEN", None, false),
            "TOKEN present=false empty=false"
        );
        assert_eq!(
            env_fact_with("TOKEN", Some(""), false),
            "TOKEN present=true empty=true"
        );
        assert_eq!(
            env_fact_with("TOKEN", Some("v"), false),
            "TOKEN present=true empty=false"
        );
        let public = env_fact("TOKEN", Some("super-secret-value"));
        assert!(!public.contains("super-secret-value"), "{public}");
        assert!(
            public.starts_with("TOKEN present=true empty=false"),
            "{public}"
        );
    }

    #[test]
    fn fingerprint_flag_appends_eight_hex_chars() {
        let fact = env_fact_with("TOKEN", Some("super-secret-value"), true);
        let prefix = "TOKEN present=true empty=false fingerprint=";
        let suffix = fact.strip_prefix(prefix).expect(&fact);
        assert_eq!(suffix.len(), 8, "{fact}");
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()), "{fact}");
        assert!(!fact.contains("super-secret-value"), "{fact}");
        assert_eq!(
            env_fact_with("TOKEN", Some(""), true),
            "TOKEN present=true empty=true"
        );
        assert_eq!(
            env_fact_with("TOKEN", None, true),
            "TOKEN present=false empty=false"
        );
    }
}
