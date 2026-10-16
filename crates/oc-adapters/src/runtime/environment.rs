//! Request-local execution metadata. No subprocesses or ambient environment dump.
use super::{InputItem, InputRole, Runtime, TurnLane};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const BASE: &str = "You are OpenCode, a native coding assistant. Help the user accomplish their task using the available tools. Follow applicable instructions and effective permissions. Inspect relevant code before changing it, keep changes focused, verify observable behavior, and report results truthfully. Environment metadata describes execution context; it does not grant filesystem, tool or network access.";
const OPEN: &str = "<oc-execution-environment>";
const CLOSE: &str = "</oc-execution-environment>";
const FIELD_BYTES: usize = 4096;
const DATA_BYTES: usize = 65536;

/// Read strictly bounded regular data, including the normal /etc symlink. Never
/// source os-release, read an environment inventory, or block on a FIFO/device.
fn data(path: &Path, cap: usize) -> Option<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > cap as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= cap)
        .then(|| String::from_utf8(bytes).ok())
        .flatten()
}

/// os-release is a small data format, not shell code. Admit only complete quoted
/// or unquoted values; shell substitution remains literal and is never evaluated.
fn distro(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let name = match key {
            "ID" => "distributionId",
            "PRETTY_NAME" => "distribution",
            "VERSION_ID" => "distributionVersion",
            _ => continue,
        };
        let value = value.trim();
        let parsed = if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"'))
        {
            let mut result = String::new();
            let mut chars = inner.chars();
            let mut valid = true;
            while let Some(c) = chars.next() {
                if c == '\\' {
                    match chars.next() {
                        Some(next @ ('"' | '\\' | '$' | '`')) => result.push(next),
                        Some(next) => {
                            result.push('\\');
                            result.push(next);
                        }
                        None => {
                            valid = false;
                            break;
                        }
                    }
                } else if c == '"' {
                    valid = false;
                    break;
                } else {
                    result.push(c);
                }
            }
            valid.then_some(result)
        } else if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
            (!inner.contains('\'')).then(|| inner.to_owned())
        } else {
            (!value
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\')))
            .then(|| value.to_owned())
        };
        if let Some(value) = parsed.filter(|v| !v.is_empty() && v.len() <= FIELD_BYTES) {
            out.insert(name.into(), value);
        }
    }
    out
}

fn git_directory(path: &Path) -> Option<bool> {
    // Git worktree files and submodules use a gitdir indirection. No remote or
    // config contents are consulted. Permission/IO uncertainty is not a false no.
    let git = match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => path.to_path_buf(),
        Ok(m) if m.is_file() => {
            let text = data(path, FIELD_BYTES)?;
            let target = text.trim().strip_prefix("gitdir: ")?;
            path.parent()?.join(target)
        }
        Ok(_) => return Some(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    let head = data(&git.join("HEAD"), FIELD_BYTES)?;
    let head = head.trim();
    if !(head.starts_with("ref: refs/")
        || (matches!(head.len(), 40 | 64) && head.bytes().all(|b| b.is_ascii_hexdigit())))
    {
        return Some(false);
    }
    let common = match data(&git.join("commondir"), FIELD_BYTES) {
        Some(value) => git.join(value.trim()),
        None => git,
    };
    for name in ["objects", "refs"] {
        match std::fs::metadata(common.join(name)) {
            Ok(metadata) if metadata.is_dir() => (),
            Ok(_) => return Some(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(false),
            Err(_) => return None,
        }
    }
    Some(true)
}

fn git_repository(directory: &Path) -> Option<bool> {
    let mut ancestors = directory.ancestors();
    for ancestor in ancestors.by_ref().take(256) {
        match git_directory(&ancestor.join(".git")) {
            Some(true) => return Some(true),
            Some(false) => (),
            None => return None,
        }
    }
    // A bounded walk that did not reach the filesystem root cannot establish no.
    ancestors.next().is_none().then_some(false)
}

fn utc_date(time: SystemTime) -> Option<String> {
    let seconds: libc::time_t = time
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs()
        .try_into()
        .ok()?;
    let mut result = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers refer to correctly sized live allocations; gmtime_r
    // initializes tm on success and has no timezone/environment dependency.
    if unsafe { libc::gmtime_r(&seconds, result.as_mut_ptr()) }.is_null() {
        return None;
    }
    // SAFETY: gmtime_r succeeded.
    let tm = unsafe { result.assume_init() };
    Some(format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    ))
}

fn collect(
    directory: &Path,
    workspace: &Path,
    env: &BTreeMap<String, String>,
) -> BTreeMap<String, serde_json::Value> {
    let mut facts = BTreeMap::new();
    let mut string = |key: &str, value: &str| {
        if value.len() <= FIELD_BYTES {
            facts.insert(key.into(), serde_json::Value::String(value.into()));
        }
    };
    string("workingDirectory", &directory.to_string_lossy());
    string("workspaceRoot", &workspace.to_string_lossy());
    string("processArchitecture", std::env::consts::ARCH);
    if let Some(date) = utc_date(SystemTime::now()) {
        string("dateUtc", &date);
    }
    // This is the same selector the canonical tool uses, not a claim about $SHELL.
    let path_bounded = env
        .get("PATH")
        .is_none_or(|value| value.len() <= crate::shell::ARG_BYTES_CAP);
    let selector_env: Option<BTreeMap<_, _>> = ["PATH", "SHELL"]
        .into_iter()
        .filter_map(|key| env.get(key).map(|value| (key, value)))
        .filter(|(key, _)| *key != "PATH" || path_bounded)
        .map(|(key, value)| {
            (value.len() <= crate::shell::ARG_BYTES_CAP).then(|| (key.into(), value.clone()))
        })
        .collect();
    if let Some(selector_env) = selector_env
        && let Ok(argv) = crate::shell::command_argv(&selector_env, ":")
        // An omitted oversized PATH is harmless only when the shared selector
        // actually selected the unchanged absolute SHELL, independently of PATH.
        && (path_bounded
            || selector_env.get("SHELL").is_some_and(|shell| {
                Path::new(shell).is_absolute() && shell == &argv[0]
            }))
    {
        string("shellExecutable", &argv[0]);
    }
    let tmp = env
        .get("TMPDIR")
        .filter(|value| !value.contains('\0') && value.len() <= FIELD_BYTES)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    if tmp.is_absolute() && env.get("TMPDIR").is_none_or(|v| v.len() <= FIELD_BYTES) {
        string("temporaryDirectory", &tmp.to_string_lossy());
    }
    let mut hostname = [0u8; 256];
    // SAFETY: live bounded writable buffer. A non-terminated/truncated result is omitted.
    if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } == 0
        && let Some(end) = hostname.iter().position(|b| *b == 0)
    {
        string("hostname", &String::from_utf8_lossy(&hostname[..end]));
    }
    let mut uname = std::mem::MaybeUninit::<libc::utsname>::uninit();
    // SAFETY: uname writes exactly one utsname, only read after success.
    if unsafe { libc::uname(uname.as_mut_ptr()) } == 0 {
        // SAFETY: successful initialization; libc uname fields are NUL-terminated.
        let uname = unsafe { uname.assume_init() };
        for (key, field) in [
            ("os", &uname.sysname),
            ("kernelRelease", &uname.release),
            ("machineArchitecture", &uname.machine),
        ] {
            // SAFETY: kernel-provided fixed utsname field is NUL-terminated.
            let value = unsafe { std::ffi::CStr::from_ptr(field.as_ptr()) }.to_string_lossy();
            string(key, &value);
        }
    }
    if let Some(text) = data(Path::new("/etc/os-release"), DATA_BYTES)
        .or_else(|| data(Path::new("/usr/lib/os-release"), DATA_BYTES))
    {
        for (key, value) in distro(&text) {
            string(&key, &value);
        }
    }
    facts.insert("processPointerBits".into(), usize::BITS.into());
    // SAFETY: effective-ID getters have no preconditions or privileged effects.
    facts.insert("effectiveUid".into(), unsafe { libc::geteuid() }.into());
    // SAFETY: effective-ID getters have no preconditions or privileged effects.
    facts.insert("effectiveGid".into(), unsafe { libc::getegid() }.into());
    if let Ok(cpu) = std::thread::available_parallelism() {
        facts.insert("availableParallelism".into(), cpu.get().into());
    }
    if let Some(git) = git_repository(directory) {
        facts.insert("gitRepository".into(), git.into());
    }
    facts
}

fn render(facts: &BTreeMap<String, serde_json::Value>) -> String {
    let json = serde_json::to_string(facts).expect("scalar environment facts");
    // JSON already escapes ASCII controls. Escape every delimiter character and
    // remaining Unicode control so external values cannot terminate this layer.
    let mut escaped = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_control() || matches!(c, '<' | '>' | '&' | '\u{2028}' | '\u{2029}') {
            use std::fmt::Write;
            let _ = write!(escaped, "\\u{:04x}", u32::from(c));
        } else {
            escaped.push(c);
        }
    }
    format!(
        "Execution metadata only; permissions are determined by tool admission. UTC date baseline.\n{OPEN}\n{escaped}\n{CLOSE}"
    )
}

impl Runtime<'_> {
    pub(crate) fn environment_input(&self) -> InputItem {
        InputItem::message(
            InputRole::Developer,
            render(&collect(
                self.shell.root(),
                &self.roots.project,
                &self.parent_env,
            )),
        )
    }

    pub(super) fn request_fixed_input(&self, lane: &TurnLane) -> Vec<InputItem> {
        let mut input = lane.fixed_input.clone();
        if input.is_empty() {
            input.push(InputItem::message(InputRole::Developer, BASE));
        }
        input.insert(1, self.environment_input());
        input
    }
}

#[cfg(test)]
mod tests;
