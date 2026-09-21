//! AUD19-AUD21: DCP qualification through bounded, offline `oc` processes.

#![cfg(target_os = "linux")]

use std::ffi::{CStr, CString, c_char, c_int};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::ptr;
use std::time::{Duration, Instant};

use oc_adapters::storage::Db;
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(10);
const MODEL: &str = "aud-dcp-runtime-model";
const SESSION: &str = "s-aud19-21";
const FACT: &str = "AUD21_RETAINED_FACT=violet";
const FILLER: &str = "AUD21_EARLY_FILLER";
const NUDGE_FRAGMENT: &str = "exceeds soft limit";
const CRASH_FILLER: &str = "AUD21_CRASH_COVERED_FILLER";
const CRASH_RANGES: usize = 32;
const CRASH_MEMBERS_PER_RANGE: usize = 56;

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
    listener: TcpListener,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated DCP fixture");
        let home = root.path().join("home");
        let config = home.join("config/opencode");
        let project = root.path().join("project");
        fs::create_dir_all(&config).expect("config directory");
        fs::create_dir_all(project.join("search")).expect("project directory");
        fs::write(
            project.join("search/alpha.rs"),
            "fn aud_runtime_needle() {}\n",
        )
        .expect("glob/grep fixture");
        fs::write(project.join("search/beta.txt"), "unrelated\n").expect("second fixture");

        let listener = TcpListener::bind("127.0.0.1:0").expect("fake Responses endpoint");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let address = listener.local_addr().expect("fake address");
        fs::write(
            config.join("opencode.json"),
            json!({
                "model": format!("fixture/{MODEL}"),
                "plugin": ["@tarquinen/opencode-dcp@3.1.15"],
                "permissions": {
                    "compress": "allow",
                    "glob": "allow",
                    "grep": "allow"
                },
                "provider": {"fixture": {
                    "npm": "@ai-sdk/openai",
                    "options": {
                        "baseURL": format!("http://{address}/proxy/v1"),
                        "apiKey": "offline-fixture-key"
                    },
                    "models": {MODEL: {
                        "name": "DCP runtime fixture",
                        "limit": {"context": 1_000_000, "output": 64_000}
                    }}
                }}
            })
            .to_string(),
        )
        .expect("provider configuration");

        let fixture = Self {
            _root: root,
            home,
            project,
            listener,
        };
        fixture.write_dcp(false);
        fixture
    }

    fn write_dcp(&self, manual_mode: bool) {
        // Exercise the documented nested DCP shape through composition.
        fs::write(
            self.project.join("dcp.jsonc"),
            json!({
                "enabled": true,
                "autoUpdate": false,
                "manualMode": {"enabled": manual_mode, "automaticStrategies": true},
                "compress": {
                    "mode": "range",
                    "permission": "allow",
                    "minContextLimit": 1,
                    "maxContextLimit": 1,
                    "nudgeFrequency": 3,
                    "iterationNudgeThreshold": 100,
                    "nudgeForce": "soft"
                },
                "strategies": {
                    "deduplication": {"enabled": true},
                    "purgeErrors": {"enabled": true, "turns": 1}
                },
                "protectedFilePatterns": []
            })
            .to_string(),
        )
        .expect("DCP configuration");
    }

    fn data(&self) -> PathBuf {
        self.home.join("data/oc")
    }

    fn spawn(&self, session: &str, prompt: &str, label: &str) -> Process {
        self.spawn_inner(session, prompt, label, None)
    }

    fn spawn_with_sync_barrier(
        &self,
        session: &str,
        prompt: &str,
        label: &str,
        barrier: &SyncBarrier,
    ) -> Process {
        self.spawn_inner(session, prompt, label, Some(barrier))
    }

    fn spawn_inner(
        &self,
        session: &str,
        prompt: &str,
        label: &str,
        barrier: Option<&SyncBarrier>,
    ) -> Process {
        let stdout = self.home.join(format!("{label}.stdout"));
        let stderr = self.home.join(format!("{label}.stderr"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_oc"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(&self.project)
            .args(["run", "--session", session, prompt])
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).expect("stdout file"))
            .stderr(fs::File::create(&stderr).expect("stderr file"));
        if let Some(barrier) = barrier {
            command
                .env("LD_PRELOAD", &barrier.library)
                .env("OC_TEST_SYNC_ARM", &barrier.arm)
                .env("OC_TEST_SYNC_REACHED", &barrier.reached)
                .env("OC_TEST_SYNC_DATABASE", &barrier.database);
        }
        let child = command.spawn().expect("actual oc binary");
        Process {
            child,
            stdout,
            stderr,
        }
    }

    fn accept(&self, process: &mut Process) -> (TcpStream, Value) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match self.listener.accept() {
                Ok((socket, _)) => return read_request(socket),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = process.child.try_wait().expect("poll actual binary") {
                        panic!(
                            "actual oc exited before provider request ({status}): {}",
                            process.diagnostics()
                        );
                    }
                    assert!(
                        Instant::now() < deadline,
                        "actual oc never contacted fake Responses endpoint: {}",
                        process.diagnostics()
                    );
                    std::thread::sleep(POLL);
                }
                Err(error) => panic!("accept fake provider request: {error}"),
            }
        }
    }

    fn seed_turn(&self, session: &str, prompt: &str, answer: &str, label: &str) {
        let mut process = self.spawn(session, prompt, label);
        let (mut socket, _) = self.accept(&mut process);
        respond_text(&mut socket, answer);
        assert!(process.wait().success(), "{}", process.diagnostics());
    }
}

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Process {
    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("bounded child wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "actual oc timeout: {}",
                self.diagnostics()
            );
            std::thread::sleep(POLL);
        }
    }

    fn diagnostics(&self) -> String {
        fs::read_to_string(&self.stderr).unwrap_or_default()
    }

    fn output(&self) -> String {
        fs::read_to_string(&self.stdout).unwrap_or_default()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let deadline = Instant::now() + TIMEOUT;
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
    }
}

struct SyncBarrier {
    library: PathBuf,
    arm: PathBuf,
    reached: PathBuf,
    database: PathBuf,
}

impl SyncBarrier {
    fn compile(directory: &Path, database: PathBuf) -> Self {
        const SOURCE: &str = r#"
#define _GNU_SOURCE
#include <fcntl.h>
#include <limits.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <unistd.h>

static _Atomic int claimed = 0;

static void fail_marker(void) {
    syscall(SYS_exit_group, 190);
    __builtin_unreachable();
}

static int target_is_database(int fd, const char *database) {
    char descriptor[64];
    int descriptor_len = snprintf(descriptor, sizeof(descriptor), "/proc/self/fd/%d", fd);
    if (descriptor_len <= 0 || (size_t)descriptor_len >= sizeof(descriptor)) {
        return 0;
    }

    char target[PATH_MAX + 32];
    ssize_t target_len = syscall(
        SYS_readlinkat,
        AT_FDCWD,
        descriptor,
        target,
        sizeof(target) - 1
    );
    if (target_len < 0 || (size_t)target_len >= sizeof(target)) {
        return 0;
    }
    target[target_len] = '\0';

    size_t database_len = strlen(database);
    return ((size_t)target_len == database_len && memcmp(target, database, database_len) == 0)
        || ((size_t)target_len == database_len + 4
            && memcmp(target, database, database_len) == 0
            && memcmp(target + database_len, "-wal", 4) == 0);
}

static void block_if_armed(int fd, char operation) {
    const char *arm = getenv("OC_TEST_SYNC_ARM");
    const char *reached = getenv("OC_TEST_SYNC_REACHED");
    const char *database = getenv("OC_TEST_SYNC_DATABASE");
    if (arm == NULL || reached == NULL || database == NULL) {
        return;
    }
    if (syscall(SYS_access, arm, F_OK) != 0 || !target_is_database(fd, database)) {
        return;
    }

    int expected = 0;
    if (!atomic_compare_exchange_strong_explicit(
            &claimed,
            &expected,
            1,
            memory_order_acq_rel,
            memory_order_acquire)) {
        return;
    }

    long marker = syscall(
        SYS_openat,
        AT_FDCWD,
        reached,
        O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC,
        0600
    );
    if (marker < 0) {
        fail_marker();
    }
    if (syscall(SYS_write, (int)marker, &operation, 1) != 1) {
        fail_marker();
    }
    if (syscall(SYS_close, (int)marker) != 0) {
        fail_marker();
    }

    for (;;) {
        syscall(SYS_pause);
    }
}

int fsync(int fd) {
    block_if_armed(fd, 'f');
    return (int)syscall(SYS_fsync, fd);
}

int fdatasync(int fd) {
    block_if_armed(fd, 'd');
    return (int)syscall(SYS_fdatasync, fd);
}
"#;

        fs::create_dir_all(directory).expect("preload helper directory");
        let source = directory.join("sync_barrier.c");
        let library = directory.join("sync_barrier.so");
        let stdout = directory.join("cc.stdout");
        let stderr = directory.join("cc.stderr");
        fs::write(&source, SOURCE).expect("write preload helper source");

        let mut compiler = Command::new("cc")
            .args([
                "-shared", "-fPIC", "-O2", "-std=c11", "-Wall", "-Wextra", "-Werror",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&library)
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).expect("compiler stdout"))
            .stderr(fs::File::create(&stderr).expect("compiler stderr"))
            .spawn()
            .expect("cc is required for the crash atomicity test");
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            if let Some(status) = compiler.try_wait().expect("poll preload helper compiler") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = compiler.kill();
                let _ = compiler.wait();
                panic!("timed out compiling preload helper");
            }
            std::thread::sleep(POLL);
        };
        assert!(
            status.success(),
            "compile preload helper ({status}): stdout={} stderr={}",
            fs::read_to_string(stdout).unwrap_or_default(),
            fs::read_to_string(stderr).unwrap_or_default()
        );
        assert!(library.is_file(), "cc did not produce preload helper");

        Self {
            library,
            arm: directory.join("arm"),
            reached: directory.join("reached"),
            database,
        }
    }

    fn arm(&self) {
        assert!(!self.arm.exists(), "sync barrier was already armed");
        assert!(!self.reached.exists(), "sync barrier was already reached");
        fs::write(&self.arm, b"armed\n").expect("arm SQLite sync barrier");
    }

    fn wait_until_reached(&self, process: &mut Process) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if self.reached.try_exists().expect("inspect reached marker") {
                assert_eq!(
                    fs::read(&self.reached).expect("read reached marker").len(),
                    1,
                    "preload helper wrote malformed reached marker"
                );
                return;
            }
            if let Some(status) = process.child.try_wait().expect("poll sync-blocked oc") {
                panic!(
                    "actual oc exited before reaching armed SQLite sync ({status}): {}",
                    process.diagnostics()
                );
            }
            assert!(
                Instant::now() < deadline,
                "actual oc never reached armed SQLite sync: {}",
                process.diagnostics()
            );
            std::thread::sleep(POLL);
        }
    }
}

#[test]
fn aud19_aud20_aud21_binary_model_compress_nudges_and_restart() {
    let fixture = Fixture::new();
    // Seed immutable history without consuming autonomous nudge cadence.
    fixture.write_dcp(true);
    let early_user = format!(
        "Remember {FACT}. {FILLER} {}",
        "user-padding ".repeat(1_100)
    );
    let early_assistant = format!("noted {FACT}. {FILLER} {}", "reply-padding ".repeat(1_100));
    fixture.seed_turn(SESSION, &early_user, &early_assistant, "seed-early");
    fixture.seed_turn(
        SESSION,
        "Keep this recent turn outside the compression range",
        "recent tail retained",
        "seed-tail",
    );
    fixture.write_dcp(false);

    let db = Db::open(&fixture.data()).expect("inspect seeded raw history");
    let seeded = db.read_history_full(SESSION).expect("seeded history");
    assert_eq!(seeded.len(), 4);
    let start_id = seeded[0].0.clone();
    let end_id = seeded[1].0.clone();
    drop(db);

    let mut process = fixture.spawn(
        SESSION,
        "Use glob, then grep, then compress the closed early range",
        "tool-loop",
    );
    let (mut socket, first) = fixture.accept(&mut process);
    assert_dcp_tool_schemas(&first);
    assert_visible_anchor(&first, &start_id);
    assert_visible_anchor(&first, &end_id);
    assert_nudges(&first, 1, "first over-threshold iteration");
    let uncompressed_bytes = input_bytes(&first);
    respond_tool(
        &mut socket,
        "glob",
        json!({"pattern": "search/**/*.rs", "offset": 0, "limit": 10}),
        "aud19-glob",
    );

    let (mut socket, second) = fixture.accept(&mut process);
    let glob_output = function_output(&second, "aud19-glob", "glob");
    assert!(
        glob_output.contains("search/alpha.rs") && !glob_output.starts_with("error:"),
        "glob output was not a successful project result: {glob_output}"
    );
    assert_nudges(&second, 0, "frequency=3 cooldown iteration");
    respond_tool(
        &mut socket,
        "grep",
        json!({
            "pattern": "aud_runtime_needle",
            "literal": true,
            "offset": 0,
            "limit": 10
        }),
        "aud19-grep",
    );

    let (mut socket, third) = fixture.accept(&mut process);
    let grep_output = function_output(&third, "aud19-grep", "grep");
    assert!(
        grep_output.contains("search/alpha.rs")
            && grep_output.contains("aud_runtime_needle")
            && !grep_output.starts_with("error:"),
        "grep output was not a successful structured continuation: {grep_output}"
    );
    assert_nudges(&third, 0, "frequency=3 cooldown iteration");
    respond_tool(
        &mut socket,
        "compress",
        json!({
            "topic": "closed early work",
            "content": [{
                "startId": start_id,
                "endId": end_id,
                "summary": format!("Early work is complete; retain {FACT}.")
            }]
        }),
        "aud19-compress",
    );

    let (mut socket, fourth) = fixture.accept(&mut process);
    let compress_output = function_output(&fourth, "aud19-compress", "compress");
    assert!(
        !compress_output.starts_with("error:"),
        "model-triggered compress failed: {compress_output}"
    );
    assert_nudges(&fourth, 0, "immediate post-compress cooldown");
    let compressed_bytes = input_bytes(&fourth);
    assert!(
        compressed_bytes < uncompressed_bytes,
        "model compress did not reduce the next request: {compressed_bytes} >= {uncompressed_bytes}"
    );
    let projected_messages = message_text(&fourth);
    assert!(
        projected_messages.contains("[compressed b") && projected_messages.contains(FACT),
        "retained fact did not arrive in a compressed projection: {projected_messages}"
    );
    assert!(
        !projected_messages.contains(FILLER),
        "covered filler remained in the compressed message projection"
    );
    respond_text(&mut socket, "DCP tool loop complete");
    assert!(process.wait().success(), "{}", process.diagnostics());
    assert_eq!(process.output().trim(), "DCP tool loop complete");
    drop(process);

    let db = Db::open(&fixture.data()).expect("inspect completed compression");
    let raw_after = db
        .read_history_full(SESSION)
        .expect("raw history after DCP");
    assert_eq!(
        &raw_after[..seeded.len()],
        seeded.as_slice(),
        "compression changed pre-existing raw transcript rows"
    );
    assert_eq!(raw_after.len(), seeded.len() + 2);
    assert_eq!(raw_after[seeded.len()].1, "user");
    assert_eq!(
        raw_after[seeded.len()].2,
        "Use glob, then grep, then compress the closed early range"
    );
    assert_eq!(raw_after[seeded.len() + 1].1, "assistant");
    assert_eq!(raw_after[seeded.len() + 1].2, "DCP tool loop complete");
    let blocks = db
        .load_compression_blocks(SESSION)
        .expect("durable compression block");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].start_msg, seeded[0].0);
    assert_eq!(blocks[0].end_msg, seeded[1].0);
    assert!(blocks[0].summary.contains(FACT));
    let block_id = blocks[0].id.clone();
    drop(db);

    // The compression reset and cadence are durable. With frequency=3 the
    // next process is still inside cooldown; a process-local reset would emit.
    let mut cadence_restart = fixture.spawn(
        SESSION,
        "Verify durable cadence immediately after compression",
        "cadence-restart",
    );
    let (mut socket, cadence_request) = fixture.accept(&mut cadence_restart);
    assert_nudges(&cadence_request, 0, "restart restores compression cooldown");
    respond_text(&mut socket, "cadence restored");
    assert!(
        cadence_restart.wait().success(),
        "{}",
        cadence_restart.diagnostics()
    );

    // A new session in a new CLI process starts its own cadence. This proves
    // the isolation visible through `oc run`; it does not claim same-process
    // multi-session coverage that the headless CLI cannot exercise.
    let mut isolated = fixture.spawn(
        "s-aud20-isolated",
        "independent over-threshold session",
        "isolated",
    );
    let (mut socket, isolated_request) = fixture.accept(&mut isolated);
    assert_nudges(&isolated_request, 1, "independent session first iteration");
    respond_text(&mut socket, "isolated complete");
    assert!(isolated.wait().success(), "{}", isolated.diagnostics());

    fixture.write_dcp(true);
    let mut manual = fixture.spawn(
        "s-aud20-manual",
        "manual mode remains over threshold",
        "manual",
    );
    let (mut socket, manual_request) = fixture.accept(&mut manual);
    assert_nudges(&manual_request, 0, "manualMode=true");
    respond_text(&mut socket, "manual complete");
    assert!(manual.wait().success(), "{}", manual.diagnostics());

    let mut restarted = fixture.spawn(
        SESSION,
        "After restart, report the retained fact",
        "restart",
    );
    let (mut socket, restart_request) = fixture.accept(&mut restarted);
    assert_visible_anchor(&restart_request, &block_id);
    let restart_messages = message_text(&restart_request);
    assert!(
        restart_messages.contains(&format!("[compressed {block_id}]"))
            && restart_messages.contains(FACT),
        "restart lost the durable summary/fact: {restart_messages}"
    );
    assert!(!restart_messages.contains(FILLER));
    assert!(
        input_bytes(&restart_request) < uncompressed_bytes,
        "restart did not retain the smaller active projection"
    );
    assert_nudges(&restart_request, 0, "manual mode after restart");
    respond_text(&mut socket, "violet retained after restart");
    assert!(restarted.wait().success(), "{}", restarted.diagnostics());
    assert_eq!(restarted.output().trim(), "violet retained after restart");

    let db = Db::open(&fixture.data()).expect("inspect restarted raw history");
    let raw_restarted = db
        .read_history_full(SESSION)
        .expect("raw history after restart");
    assert_eq!(
        &raw_restarted[..raw_after.len()],
        raw_after.as_slice(),
        "restart changed the pre-existing raw transcript"
    );
    assert_eq!(
        db.load_compression_blocks(SESSION)
            .expect("compression block after restart"),
        blocks,
        "restart changed durable compression membership"
    );
}

#[test]
fn aud21_binary_sigkill_never_publishes_a_partial_multi_range_compression() {
    run_crash_atomicity_at_sqlite_sync();
}

#[derive(Clone)]
struct RequestedRange {
    start: String,
    end: String,
    summary: String,
    members: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReopenedCompression {
    Absent,
    Committed,
}

fn run_crash_atomicity_at_sqlite_sync() {
    let fixture = Fixture::new();
    let session = "s-aud21-crash";
    let prompt = "AUD21 crash compression";
    let next_prompt = "AUD21 explicit prompt after crash";
    let call_id = "aud21-crash-compress";
    let (seeded, ranges) = seed_crash_history(&fixture, session);
    assert!(
        ranges.len() >= 2,
        "crash test requires multiple model ranges"
    );
    let arguments = json!({
        "topic": "atomic crash at SQLite sync",
        // Reverse model order to ensure a complete commit also keeps each
        // summary attached to its own transcript-ordered anchors.
        "content": ranges.iter().rev().map(|range| json!({
            "startId": range.start,
            "endId": range.end,
            "summary": range.summary,
        })).collect::<Vec<_>>()
    });
    let barrier = SyncBarrier::compile(
        &fixture.home.join("sync-barrier"),
        fixture.data().join("oc.sqlite"),
    );

    let mut process = fixture.spawn_with_sync_barrier(session, prompt, "crash-at-sync", &barrier);
    let (mut socket, request) = fixture.accept(&mut process);
    assert_visible_anchor(&request, &ranges[0].start);
    assert_visible_anchor(&request, &ranges.last().expect("last range").end);

    // This connection deliberately ignores oc.lock. It is read-only and is
    // opened before dispatch while the actual application process owns root.
    let sqlite = DirectSqlite::open(&fixture.data().join("oc.sqlite"));
    assert!(
        sqlite.compress_operation(session).is_none(),
        "compress intent cannot precede the fake model call"
    );
    respond_tool(&mut socket, "compress", arguments.clone(), call_id);
    drop(socket);

    let deadline = Instant::now() + TIMEOUT;
    let observed = loop {
        if let Some(operation) = sqlite.compress_operation(session) {
            assert_eq!(operation.state, "started");
            assert_eq!(operation.output, None);
            break operation;
        }
        if let Some(status) = process.child.try_wait().expect("poll crash target") {
            panic!(
                "actual oc exited before durable compress intent ({status}): {}",
                process.diagnostics()
            );
        }
        assert!(
            Instant::now() < deadline,
            "external SQLite reader never observed started compress intent: {}",
            process.diagnostics()
        );
        std::thread::sleep(Duration::from_micros(250));
    };

    // Arm only after another SQLite connection has observed the committed
    // intent. The preload shim remains inert before this point, then stops the
    // actual oc process inside its next database/WAL durability sync.
    barrier.arm();
    barrier.wait_until_reached(&mut process);
    // SAFETY: process.child.id() names this test's live child; SIGKILL has no
    // handler and the child is synchronously reaped immediately below.
    let killed = unsafe { libc::kill(process.child.id() as libc::pid_t, libc::SIGKILL) };
    assert_eq!(killed, 0, "SIGKILL actual oc");
    assert_eq!(
        process.wait().signal(),
        Some(libc::SIGKILL),
        "actual oc terminated by SIGKILL"
    );

    // Drop the observer that was open before the interrupted WAL commit. The
    // fresh Db below is the authoritative crash-recovery image.
    drop(sqlite);
    drain_pending_requests(&fixture.listener);

    // Db::open is possible only after the killed owner was reaped. It does not
    // run application recovery, so this is the crash image itself.
    let db = Db::open(&fixture.data()).expect("reopen killed process database");
    let raw_after_kill = db
        .read_history_full(session)
        .expect("raw transcript after SIGKILL");
    assert_eq!(
        &raw_after_kill[..seeded.len()],
        seeded.as_slice(),
        "compression changed seeded raw transcript"
    );
    assert_eq!(raw_after_kill.len(), seeded.len() + 1);
    assert_eq!(raw_after_kill.last().expect("accepted prompt").1, "user");
    assert_eq!(raw_after_kill.last().expect("accepted prompt").2, prompt);

    let blocks = db
        .load_compression_blocks(session)
        .expect("compression state after kill");

    let operations = db.list_tool_ops(session).expect("compress journal");
    assert_eq!(operations.len(), 1);
    let operation = &operations[0];
    assert_eq!(operation.op, observed.id);
    assert_eq!(operation.turn.as_deref(), Some(observed.turn.as_str()));
    assert_eq!(operation.name, "compress");
    assert_eq!(
        serde_json::from_str::<Value>(operation.input.as_deref().expect("compress input"))
            .expect("compress input JSON"),
        arguments
    );

    let expected_members: usize = ranges.iter().map(|range| range.members.len()).sum();
    let reopened_sqlite = DirectSqlite::open(&fixture.data().join("oc.sqlite"));
    let counts = reopened_sqlite.compression_counts();
    drop(reopened_sqlite);
    let reopened = if counts == (0, 0)
        && blocks.is_empty()
        && operation.state == "started"
        && operation.output.is_none()
    {
        ReopenedCompression::Absent
    } else if counts == (ranges.len(), expected_members)
        && blocks.len() == ranges.len()
        && operation.state == "completed"
        && operation.output.is_some()
    {
        ReopenedCompression::Committed
    } else {
        panic!(
            "reopen exposed partial compression: counts={counts:?}, blocks={}, operation_state={}, operation_output={:?}",
            blocks.len(),
            operation.state,
            operation.output
        );
    };
    if reopened == ReopenedCompression::Committed {
        for (block, requested) in blocks.iter().zip(&ranges) {
            assert_eq!(block.start_msg, requested.start);
            assert_eq!(block.end_msg, requested.end);
            assert_eq!(block.summary, requested.summary);
            assert_eq!(block.members, requested.members);
        }
    }

    let (turn_status, turn_result) = db
        .turn_result(&observed.turn)
        .expect("interrupted compression turn");
    assert_eq!(turn_status, "started");
    let journal_text = turn_result.expect("durable turn journal");
    let journal: Value = serde_json::from_str(&journal_text).expect("turn journal JSON");
    let call = journal["input"]
        .as_array()
        .expect("turn journal input")
        .iter()
        .find(|item| item["type"] == "function_call" && item["call_id"] == call_id)
        .expect("journaled compress call");
    assert_eq!(call["name"], "compress");
    assert_eq!(
        serde_json::from_str::<Value>(call["arguments"].as_str().expect("call arguments"))
            .expect("call arguments JSON"),
        arguments
    );
    let journal_outputs = journal["input"]
        .as_array()
        .expect("turn journal input")
        .iter()
        .filter(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .collect::<Vec<_>>();
    if reopened == ReopenedCompression::Absent {
        assert!(
            journal_outputs.is_empty(),
            "uncommitted outcome entered turn journal"
        );
    } else {
        assert_eq!(journal_outputs.len(), 1);
        let output = journal_outputs[0]["output"]
            .as_str()
            .expect("compress output string");
        assert_eq!(operation.output.as_deref(), Some(output));
        let output: Value = serde_json::from_str(output).expect("compress output JSON");
        assert_eq!(output["status"], "compressed");
        assert_eq!(
            output["blocks"],
            Value::Array(
                blocks
                    .iter()
                    .map(|block| Value::String(block.id.clone()))
                    .collect()
            )
        );
    }
    drop(db);

    // Starting an ordinary explicit prompt runs application recovery. It must
    // neither replay compress nor create a provider request before this prompt.
    let answer = "AUD21 recovered after sync crash";
    let mut restarted = fixture.spawn(session, next_prompt, "crash-restart");
    let (mut socket, restart_request) = fixture.accept(&mut restarted);
    let restart_text = message_text(&restart_request);
    assert!(restart_text.contains(next_prompt));
    if reopened == ReopenedCompression::Absent {
        assert!(restart_text.contains(CRASH_FILLER));
        assert!(!restart_request["input"].to_string().contains(call_id));
        for requested in &ranges {
            assert!(!restart_text.contains(&requested.summary));
        }
    } else {
        assert!(!restart_text.contains(CRASH_FILLER));
        for requested in &ranges {
            assert!(
                restart_text.contains(&requested.summary),
                "committed summary fact missing after recovery: {}",
                requested.summary
            );
        }
        assert_eq!(
            function_output(&restart_request, call_id, "compress"),
            operation.output.as_deref().expect("completed output")
        );
    }
    respond_text(&mut socket, answer);
    assert!(restarted.wait().success(), "{}", restarted.diagnostics());
    assert_eq!(restarted.output().trim(), answer);
    assert_eq!(
        fixture
            .listener
            .accept()
            .expect_err("one explicit post-crash provider request; no replay")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );

    let db = Db::open(&fixture.data()).expect("inspect application recovery");
    let recovered_operations = db
        .list_tool_ops(session)
        .expect("recovered compress operation");
    assert_eq!(recovered_operations.len(), 1, "compress was not replayed");
    let recovered = &recovered_operations[0];
    if reopened == ReopenedCompression::Absent {
        assert_eq!(recovered.state, "unknown");
        assert_eq!(recovered.output, None);
    } else {
        assert_eq!(recovered.state, "completed");
        assert_eq!(recovered.output, operation.output);
    }
    assert_eq!(
        db.turn_result(&observed.turn)
            .expect("application-recovered turn"),
        ("unknown".to_string(), Some(journal_text)),
        "recovery marks the interrupted turn unknown without changing its journal"
    );
    assert_eq!(
        db.load_compression_blocks(session)
            .expect("blocks after application recovery"),
        blocks,
        "application recovery changed the crash-atomic projection"
    );
    let raw_after_recovery = db
        .read_history_full(session)
        .expect("raw transcript after recovery");
    assert_eq!(
        &raw_after_recovery[..raw_after_kill.len()],
        raw_after_kill.as_slice(),
        "recovery changed pre-existing raw transcript"
    );
    assert_eq!(raw_after_recovery.len(), raw_after_kill.len() + 2);
    assert_eq!(
        (
            raw_after_recovery[raw_after_kill.len()].1.as_str(),
            raw_after_recovery[raw_after_kill.len()].2.as_str()
        ),
        ("user", next_prompt)
    );
    assert_eq!(
        (
            raw_after_recovery[raw_after_kill.len() + 1].1.as_str(),
            raw_after_recovery[raw_after_kill.len() + 1].2.as_str(),
        ),
        ("assistant", answer)
    );
}

fn seed_crash_history(
    fixture: &Fixture,
    session: &str,
) -> (Vec<(String, String, String)>, Vec<RequestedRange>) {
    let db = Db::open(&fixture.data()).expect("seed crash database");
    db.create_session(session).expect("create crash session");
    db.set_pref(
        &format!("{}{session}", oc_adapters::runtime::SESSION_LOCATION_PREFIX),
        &fixture
            .project
            .canonicalize()
            .expect("canonical fixture project")
            .to_string_lossy(),
    )
    .expect("bind crash session to fixture Location");
    let padding = "x".repeat(512);
    let mut ranges = Vec::with_capacity(CRASH_RANGES);
    for range_index in 0..CRASH_RANGES {
        let mut members = Vec::with_capacity(CRASH_MEMBERS_PER_RANGE);
        for member_index in 0..CRASH_MEMBERS_PER_RANGE {
            let ordinal = range_index * CRASH_MEMBERS_PER_RANGE + member_index;
            let role = if ordinal.is_multiple_of(2) {
                "user"
            } else {
                "assistant"
            };
            let text =
                format!("{CRASH_FILLER} range={range_index} member={member_index} {padding}");
            members.push(
                db.append_message(session, role, &text)
                    .expect("seed covered message"),
            );
        }
        ranges.push(RequestedRange {
            start: members.first().expect("range start").clone(),
            end: members.last().expect("range end").clone(),
            summary: format!("AUD21_CRASH_FACT range={range_index}"),
            members,
        });
        db.append_message(
            session,
            "assistant",
            &format!("uncovered separator {range_index}"),
        )
        .expect("seed range separator");
    }
    db.append_message(session, "assistant", "uncovered live seed tail")
        .expect("seed tail");
    let raw = db.read_history_full(session).expect("seeded raw history");
    drop(db);
    (raw, ranges)
}

fn drain_pending_requests(listener: &TcpListener) {
    loop {
        match listener.accept() {
            Ok((socket, _)) => drop(socket),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) => panic!("drain killed process provider request: {error}"),
        }
    }
}

#[derive(Debug)]
struct DirectOperation {
    id: String,
    turn: String,
    state: String,
    output: Option<String>,
}

// Minimal read-only SQLite probe for observing committed rows while Db's
// product ownership lock is held by another process. The symbols are already
// linked by oc-adapters' bundled rusqlite dependency.
struct DirectSqlite {
    raw: *mut sqlite3,
}

impl DirectSqlite {
    fn open(path: &Path) -> Self {
        let path = CString::new(path.as_os_str().as_bytes()).expect("SQLite path without NUL");
        let mut raw = ptr::null_mut();
        // SAFETY: path is NUL-terminated and raw is a valid out-pointer. The
        // returned handle is exclusively owned and closed by Drop below.
        let code =
            unsafe { sqlite3_open_v2(path.as_ptr(), &mut raw, SQLITE_OPEN_READONLY, ptr::null()) };
        assert_eq!(code, SQLITE_OK, "open SQLite crash probe");
        // SAFETY: successful sqlite3_open_v2 returned a live handle.
        assert_eq!(unsafe { sqlite3_busy_timeout(raw, 100) }, SQLITE_OK);
        Self { raw }
    }

    fn compress_operation(&self, session: &str) -> Option<DirectOperation> {
        let session = session.replace('\'', "''");
        let values = self.query(&format!(
            "SELECT id, turn_id, state, output FROM tool_operations \
             WHERE session_id = '{session}' AND name = 'compress' \
             ORDER BY rowid DESC LIMIT 1"
        ))?;
        Some(DirectOperation {
            id: values[0].clone().expect("operation id"),
            turn: values[1].clone().expect("operation turn"),
            state: values[2].clone().expect("operation state"),
            output: values[3].clone(),
        })
    }

    fn compression_counts(&self) -> (usize, usize) {
        let values = self
            .query(
                "SELECT (SELECT COUNT(*) FROM compression_blocks), \
                        (SELECT COUNT(*) FROM compression_members)",
            )
            .expect("compression count row");
        (
            values[0]
                .as_deref()
                .expect("block count")
                .parse()
                .expect("numeric block count"),
            values[1]
                .as_deref()
                .expect("member count")
                .parse()
                .expect("numeric member count"),
        )
    }

    fn query(&self, sql: &str) -> Option<Vec<Option<String>>> {
        let sql = CString::new(sql).expect("probe SQL without NUL");
        let mut statement = ptr::null_mut();
        // SAFETY: self.raw is live, sql is NUL-terminated, and statement is a
        // valid out-pointer retained only until sqlite3_finalize below.
        let prepared = unsafe {
            sqlite3_prepare_v2(self.raw, sql.as_ptr(), -1, &mut statement, ptr::null_mut())
        };
        self.assert_ok(prepared, "prepare SQLite crash probe");
        // SAFETY: statement was successfully prepared and is not shared.
        let stepped = unsafe { sqlite3_step(statement) };
        let row = if stepped == SQLITE_ROW {
            // SAFETY: statement currently points at a SQLITE_ROW. SQLite owns
            // each text pointer until the next step/finalize; copy it now.
            let columns = unsafe { sqlite3_column_count(statement) };
            let mut values = Vec::with_capacity(columns as usize);
            for column in 0..columns {
                // SAFETY: statement remains positioned on the same live row.
                values.push(unsafe { sqlite_column(statement, column) });
            }
            Some(values)
        } else {
            assert_eq!(stepped, SQLITE_DONE, "step SQLite crash probe");
            None
        };
        // SAFETY: statement was initialized by sqlite3_prepare_v2 and has not
        // previously been finalized.
        self.assert_ok(
            unsafe { sqlite3_finalize(statement) },
            "finalize SQLite probe",
        );
        row
    }

    fn assert_ok(&self, code: c_int, action: &str) {
        if code == SQLITE_OK {
            return;
        }
        // SAFETY: self.raw is live and sqlite3_errmsg returns a stable
        // NUL-terminated pointer owned by that connection.
        let error = unsafe { sqlite3_errmsg(self.raw) };
        // SAFETY: error is the live connection's NUL-terminated error string.
        let message = unsafe { CStr::from_ptr(error) }.to_string_lossy();
        panic!("{action}: SQLite code {code}: {message}");
    }
}

impl Drop for DirectSqlite {
    fn drop(&mut self) {
        // SAFETY: raw is exclusively owned and all statements are finalized.
        let code = unsafe { sqlite3_close(self.raw) };
        if !std::thread::panicking() {
            assert_eq!(code, SQLITE_OK, "close SQLite crash probe");
        }
    }
}

unsafe fn sqlite_column(statement: *mut sqlite3_stmt, column: c_int) -> Option<String> {
    // SAFETY: caller guarantees statement is positioned on SQLITE_ROW.
    let text = unsafe { sqlite3_column_text(statement, column) };
    if text.is_null() {
        return None;
    }
    // SAFETY: same live row as above; SQLite reports the exact byte length.
    let length = unsafe { sqlite3_column_bytes(statement, column) };
    let length = usize::try_from(length).expect("nonnegative SQLite text length");
    // SAFETY: SQLite guarantees at least `length` readable bytes for text.
    let bytes = unsafe { std::slice::from_raw_parts(text, length) };
    Some(
        std::str::from_utf8(bytes)
            .expect("SQLite probe text is UTF-8")
            .to_string(),
    )
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct sqlite3 {
    _private: [u8; 0],
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct sqlite3_stmt {
    _private: [u8; 0],
}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;

unsafe extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        database: *mut *mut sqlite3,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_busy_timeout(database: *mut sqlite3, milliseconds: c_int) -> c_int;
    fn sqlite3_prepare_v2(
        database: *mut sqlite3,
        sql: *const c_char,
        bytes: c_int,
        statement: *mut *mut sqlite3_stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_step(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_column_count(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_column_text(statement: *mut sqlite3_stmt, column: c_int) -> *const u8;
    fn sqlite3_column_bytes(statement: *mut sqlite3_stmt, column: c_int) -> c_int;
    fn sqlite3_finalize(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_errmsg(database: *mut sqlite3) -> *const c_char;
    fn sqlite3_close(database: *mut sqlite3) -> c_int;
}

fn assert_dcp_tool_schemas(request: &Value) {
    let compress = tool_definition(request, "compress");
    assert_object_schema(compress, &["topic", "content"]);
    let content = &compress["parameters"]["properties"]["content"];
    assert_eq!(content["type"], "array", "compress content schema");
    let range = &content["items"];
    assert_eq!(range["type"], "object", "compress range schema");
    for field in ["startId", "endId", "summary"] {
        assert_eq!(
            range["properties"][field]["type"], "string",
            "compress range field {field}"
        );
        assert!(required(range, field), "compress range requires {field}");
    }

    let glob = tool_definition(request, "glob");
    assert_object_schema(glob, &["pattern"]);
    assert_eq!(
        glob["parameters"]["properties"]["pattern"]["type"],
        "string"
    );
    assert_eq!(
        glob["parameters"]["properties"]["offset"]["type"],
        "integer"
    );
    assert_eq!(glob["parameters"]["properties"]["limit"]["type"], "integer");

    let grep = tool_definition(request, "grep");
    assert_object_schema(grep, &["pattern"]);
    assert_eq!(
        grep["parameters"]["properties"]["pattern"]["type"],
        "string"
    );
    assert_eq!(
        grep["parameters"]["properties"]["literal"]["type"],
        "boolean"
    );
    assert_eq!(
        grep["parameters"]["properties"]["offset"]["type"],
        "integer"
    );
    assert_eq!(grep["parameters"]["properties"]["limit"]["type"], "integer");
}

fn tool_definition<'a>(request: &'a Value, name: &str) -> &'a Value {
    let tools = request["tools"].as_array().expect("Responses tools array");
    tools
        .iter()
        .find(|tool| tool["type"] == "function" && tool["name"] == name)
        .unwrap_or_else(|| {
            let visible: Vec<_> = tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect();
            panic!("AUD19 missing model-visible {name} function schema; visible tools: {visible:?}")
        })
}

fn assert_object_schema(tool: &Value, required_fields: &[&str]) {
    assert!(
        tool["description"]
            .as_str()
            .is_some_and(|description| !description.is_empty()),
        "tool needs a model-visible description: {tool}"
    );
    let schema = &tool["parameters"];
    assert_eq!(schema["type"], "object", "ordinary function schema");
    assert!(
        schema["properties"].is_object(),
        "schema properties: {tool}"
    );
    for field in required_fields {
        assert!(required(schema, field), "schema requires {field}: {tool}");
    }
}

fn required(schema: &Value, field: &str) -> bool {
    schema["required"]
        .as_array()
        .is_some_and(|fields| fields.iter().any(|value| value == field))
}

fn assert_visible_anchor(request: &Value, anchor: &str) {
    assert!(
        request["input"].to_string().contains(anchor),
        "DCP anchor {anchor} is not visible in the model input projection"
    );
}

fn assert_nudges(request: &Value, expected: usize, stage: &str) {
    let count = message_text(request).matches(NUDGE_FRAGMENT).count();
    assert_eq!(
        count, expected,
        "wrong model-visible DCP nudge count at {stage}"
    );
}

fn input_bytes(request: &Value) -> usize {
    serde_json::to_vec(&request["input"])
        .expect("serialize captured input")
        .len()
}

fn message_text(request: &Value) -> String {
    request["input"]
        .as_array()
        .expect("typed Responses input")
        .iter()
        .filter(|item| item["type"] == "message")
        .filter_map(|item| item["content"].as_array())
        .flat_map(|content| content.iter())
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn function_output<'a>(request: &'a Value, call_id: &str, name: &str) -> &'a str {
    let input = request["input"].as_array().expect("typed Responses input");
    let call = input
        .iter()
        .position(|item| {
            item["type"] == "function_call" && item["call_id"] == call_id && item["name"] == name
        })
        .unwrap_or_else(|| panic!("missing structured {name} call for call_id={call_id}"));
    let output = input
        .iter()
        .position(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .unwrap_or_else(|| panic!("missing structured function output for call_id={call_id}"));
    assert!(call < output, "function output precedes call_id={call_id}");
    let item = &input[output];
    assert_eq!(item["type"], "function_call_output");
    assert_eq!(item["call_id"], call_id);
    item["output"]
        .as_str()
        .unwrap_or_else(|| panic!("non-string Responses function output for {call_id}: {item}"))
}

fn read_request(mut socket: TcpStream) -> (TcpStream, Value) {
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("request read timeout");
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("response write timeout");
    let deadline = Instant::now() + TIMEOUT;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        assert!(Instant::now() < deadline, "HTTP header deadline");
        let read = socket.read(&mut chunk).expect("request headers");
        assert_ne!(read, 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(bytes.len() < 65_536, "bounded request headers");
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).expect("HTTP headers");
    assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().expect("content length"))
        })
        .expect("content-length");
    assert!(length < 2 * 1024 * 1024, "bounded request body");
    while bytes.len() < header_end + length {
        assert!(Instant::now() < deadline, "HTTP body deadline");
        let read = socket.read(&mut chunk).expect("request body");
        assert_ne!(read, 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body: Value = serde_json::from_slice(&bytes[header_end..header_end + length])
        .expect("typed Responses request JSON");
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["stream"], true);
    (socket, body)
}

fn respond_tool(socket: &mut TcpStream, name: &str, arguments: Value, call_id: &str) {
    let item_id = format!("item-{call_id}");
    let arguments = arguments.to_string();
    respond_events(
        socket,
        &[
            json!({"type": "response.output_item.added", "item": {
                "type": "function_call",
                "id": item_id,
                "call_id": call_id,
                "name": name,
                "arguments": "",
                "status": "in_progress"
            }}),
            json!({
                "type": "response.function_call_arguments.delta",
                "item_id": item_id,
                "delta": arguments
            }),
            json!({"type": "response.completed", "response": {
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": item_id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments,
                    "status": "completed"
                }]
            }}),
        ],
    );
}

fn respond_text(socket: &mut TcpStream, text: &str) {
    respond_events(
        socket,
        &[
            json!({"type": "response.output_text.delta", "delta": text}),
            json!({"type": "response.completed", "response": {
                "status": "completed",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": text}]
                }]
            }}),
        ],
    );
}

fn respond_events(socket: &mut TcpStream, events: &[Value]) {
    let sse: String = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
        sse.len()
    )
    .expect("fake Responses reply");
}
