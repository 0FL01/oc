//! Real CoreApp controls over the existing native resource owner (MCP08/MCP10).
use super::*;
use oc_core::queries::{McpAction, McpControl, McpServerSnapshot, McpSnapshot, McpStatus};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[path = "mcp_tests/lookups.rs"]
mod lookups;

struct Fixture {
    _root: tempfile::TempDir,
    project: PathBuf,
    global: PathBuf,
    data: PathBuf,
    env: BTreeMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let global = root.path().join("home/config/opencode");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&global).unwrap();
        let env = BTreeMap::from([
            (
                "HOME".into(),
                root.path().join("home").display().to_string(),
            ),
            (
                "XDG_CONFIG_HOME".into(),
                root.path().join("home/config").display().to_string(),
            ),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ]);
        Self {
            data: root.path().join("data"),
            _root: root,
            project,
            global,
            env,
        }
    }

    fn config(&self, mcp: Value) {
        fs::write(self.global.join("opencode.json"), json!({
            "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"apiKey":"fixture-key","baseURL":"https://example.invalid/v1"},
                "models":{"fixture":{"name":"Fixture","limit":{"context":65536,"output":4096}}}}},
            "model":"fixture/fixture", "mcp":mcp
        }).to_string()).unwrap();
    }

    fn entry(
        &self,
        label: &str,
        report: &Path,
        initialize: &str,
        catalog: &str,
        disabled: bool,
    ) -> Value {
        json!({"type":"local","disabled":disabled,
            "command":["/usr/bin/python3", Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mcp10-lifecycle.py"),
                report,label,initialize,catalog,&self.project],
            "timeout":{"startup":3000,"catalog":3000,"execution":3000}})
    }

    async fn spawn(&self) -> (CoreApp, WorkerGuard) {
        let (app, guard, _) = spawn_with_env(&self.project, &self.data, self.env.clone())
            .await
            .unwrap();
        (app, guard)
    }
}

fn control(snapshot: &McpSnapshot, name: &str, action: McpAction) -> McpControl {
    McpControl {
        binding: snapshot.binding.clone(),
        server: snapshot
            .servers
            .iter()
            .find(|row| row.name == name)
            .unwrap()
            .id
            .clone(),
        action,
    }
}

async fn wait_status(
    app: &CoreApp,
    name: &str,
    status: McpStatus,
) -> (McpSnapshot, McpServerSnapshot) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = tokio::time::timeout(Duration::from_millis(500), app.mcp_status())
            .await
            .expect("status blocked by network")
            .unwrap();
        let row = snapshot
            .servers
            .iter()
            .find(|row| row.name == name)
            .unwrap();
        if row.status == status {
            return (snapshot.clone(), row.clone());
        }
        assert!(
            Instant::now() < deadline,
            "MCP status did not settle: {snapshot:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn events(report: &Path, stage: &str) -> usize {
    fs::read_to_string(report.with_extension("json.events"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with(&format!("{stage} ")))
        .count()
}

fn counts(report: &Path) -> Value {
    serde_json::from_slice(&fs::read(report).unwrap()).unwrap()
}

fn assert_reaped(report: &Path) {
    let pid = counts(report)["pid"].as_i64().unwrap() as libc::pid_t;
    // SAFETY: signal zero only probes the child PID recorded by this fixture.
    let alive = unsafe { libc::kill(pid, 0) };
    assert_ne!(alive, 0, "owned MCP child survived close");
    assert_eq!(events(report, "closed"), events(report, "spawn"));
}

#[tokio::test]
async fn mcp08_controls_coalesce_before_activation_and_reuse_frozen_source() {
    let fixture = Fixture::new();
    let report = fixture.project.join("dormant.json");
    let gate = fixture.project.join("catalog-release");
    let mut entry = fixture.entry("dormant", &report, "-", gate.to_str().unwrap(), true);
    entry["command"][0] = json!("{file:program}");
    entry["environment"] = json!({"MCP10_CANARY":"{file:token}"});
    fixture.config(json!({"servers":{"dormant":entry}}));
    fs::write(fixture.global.join("program"), "/usr/bin/python3").unwrap();
    fs::write(fixture.global.join("token"), "mcp10-activated-canary").unwrap();
    let (app, guard) = fixture.spawn().await;
    let initial = app.mcp_status().await.unwrap();
    assert_eq!(initial.servers[0].status, McpStatus::Disabled);
    assert!(!report.exists(), "disabled entry spawned");
    // Disk edits alone cannot mutate this immutable source generation.
    fixture.config(json!({"servers":{"dormant":{"type":"local","disabled":true,"command":["never-execute-new-source"]}}}));
    let disk = fs::read(fixture.global.join("opencode.json")).unwrap();
    let connect = control(&initial, "dormant", McpAction::Connect);
    app.mcp_control(connect.clone()).await.unwrap();
    let (pending, _) = wait_status(&app, "dormant", McpStatus::Pending).await;
    let deadline = Instant::now() + Duration::from_secs(3);
    while !report.exists() || counts(&report)["catalog"] != 1 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // A duplicate must not re-read activation files or start a second job.
    fs::write(fixture.global.join("program"), "never-execute-duplicate").unwrap();
    let (one, two) = tokio::join!(app.mcp_control(connect.clone()), app.mcp_control(connect));
    assert_eq!(one.unwrap().binding, pending.binding);
    assert_eq!(
        two.unwrap().servers[0].pending_action,
        Some(McpAction::Connect)
    );
    assert_eq!(events(&report, "spawn"), 1);
    fs::write(&gate, "release").unwrap();
    let (connected, row) = wait_status(&app, "dormant", McpStatus::Connected).await;
    assert_eq!(row.tools, 1);
    assert!(
        !row.configured_enabled,
        "manual connect changed configured activation"
    );
    assert_eq!(counts(&report)["activated"], true);
    assert_eq!(counts(&report)["cwd"], true);
    app.mcp_control(control(&connected, "dormant", McpAction::Disconnect))
        .await
        .unwrap();
    wait_status(&app, "dormant", McpStatus::Disabled).await;
    assert_reaped(&report);
    assert_eq!(events(&report, "initialize"), 1);
    assert_eq!(events(&report, "catalog"), 1);
    assert_eq!(
        fs::read(fixture.global.join("opencode.json")).unwrap(),
        disk
    );
    assert!(!format!("{connected:?}").contains("mcp10-activated-canary"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (restart, guard) = fixture.spawn().await;
    assert_eq!(
        restart.mcp_status().await.unwrap().servers[0].status,
        McpStatus::Disabled
    );
    restart.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(events(&report, "spawn"), 1);
}

#[tokio::test]
async fn mcp08_manual_activation_uses_pinned_nofollow_root_not_replaced_path() {
    let fixture = Fixture::new();
    let report = fixture.project.join("pin.json");
    let mut entry = fixture.entry("pin", &report, "-", "-", true);
    entry["command"][0] = json!("{file:program}");
    fixture.config(json!({"servers":{"pin":entry}}));
    fs::write(fixture.global.join("program"), "/usr/bin/python3").unwrap();
    let (app, guard) = fixture.spawn().await;
    let disabled = app.mcp_status().await.unwrap();
    let pinned = fixture.global.with_file_name("pinned-old");
    fs::rename(&fixture.global, &pinned).unwrap();
    fs::create_dir(&fixture.global).unwrap();
    fs::write(
        fixture.global.join("program"),
        "never-execute-replaced-root",
    )
    .unwrap();
    app.mcp_control(control(&disabled, "pin", McpAction::Connect))
        .await
        .unwrap();
    let (connected, _) = wait_status(&app, "pin", McpStatus::Connected).await;
    app.mcp_control(control(&connected, "pin", McpAction::Disconnect))
        .await
        .unwrap();
    wait_status(&app, "pin", McpStatus::Disabled).await;
    assert_reaped(&report);
    // The same source reader refuses a symlink in the pinned root.
    fs::remove_file(pinned.join("program")).unwrap();
    std::os::unix::fs::symlink(fixture.global.join("program"), pinned.join("program")).unwrap();
    let disabled = app.mcp_status().await.unwrap();
    assert!(
        app.mcp_control(control(&disabled, "pin", McpAction::Connect))
            .await
            .is_err()
    );
    assert_eq!(events(&report, "spawn"), 1);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn mcp08_disabled_project_activation_cannot_reintroduce_orphan_product_credential() {
    let mut fixture = Fixture::new();
    fixture
        .env
        .insert("ORPHAN_PASSWORD".into(), "mcp10-activated-canary".into());
    let healthy = fixture.project.join("healthy.json");
    let rejected = fixture.project.join("rejected.json");
    fixture
        .config(json!({"servers":{"healthy":fixture.entry("healthy", &healthy, "-", "-", false)}}));
    let mut entry = fixture.entry("rejected", &rejected, "-", "-", true);
    entry["environment"] = json!({"MCP10_CANARY":"{env:ORPHAN_PASSWORD}"});
    fs::write(
        fixture.project.join("opencode.json"),
        json!({"mcp":{"servers":{"rejected":entry}}}).to_string(),
    )
    .unwrap();
    let (app, guard) = fixture.spawn().await;
    wait_status(&app, "healthy", McpStatus::Connected).await;
    let disabled = app.mcp_status().await.unwrap();
    assert!(
        app.mcp_control(control(&disabled, "rejected", McpAction::Connect))
            .await
            .is_err()
    );
    assert!(
        !rejected.exists(),
        "withheld credential entered the command environment"
    );
    let after = app.mcp_status().await.unwrap();
    assert_eq!(
        after
            .servers
            .iter()
            .find(|r| r.name == "healthy")
            .unwrap()
            .status,
        McpStatus::Connected
    );
    assert!(!format!("{after:?}").contains("mcp10-activated-canary"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_reaped(&healthy);
}

#[tokio::test]
async fn mcp10_reload_and_location_join_old_pending_jobs_and_reject_stale_actions() {
    let fixture = Fixture::new();
    let old = fixture.project.join("old.json");
    let gate = fixture.project.join("old-initialize-release");
    fixture.config(json!({}));
    fs::write(fixture.project.join("opencode.json"), json!({"mcp":{"servers":{"same":fixture.entry("same", &old, gate.to_str().unwrap(), "-", false)}}}).to_string()).unwrap();
    let (app, guard) = fixture.spawn().await;
    let previous = app.mcp_status().await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !old.exists() || counts(&old)["initialize"] != 1 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let new = fixture.project.join("new.json");
    fs::write(
        fixture.project.join("opencode.json"),
        json!({"mcp":{"servers":{"same":fixture.entry("same", &new, "-", "-", false)}}})
            .to_string(),
    )
    .unwrap();
    app.reload_location().await.unwrap();
    assert_reaped(&old);
    fs::write(&gate, "late-old-release").unwrap();
    assert!(
        app.mcp_control(control(&previous, "same", McpAction::Connect))
            .await
            .is_err()
    );
    let (reloaded, _) = wait_status(&app, "same", McpStatus::Connected).await;
    assert_ne!(previous.binding, reloaded.binding);
    assert_eq!(events(&old, "catalog"), 0);
    let other = fixture.project.with_file_name("other");
    fs::create_dir(&other).unwrap();
    app.switch_location_home(other.display().to_string())
        .await
        .unwrap();
    assert_reaped(&new);
    assert!(
        app.mcp_status().await.unwrap().servers.is_empty(),
        "old project state survived Location switch"
    );
    assert!(
        app.mcp_control(control(&reloaded, "same", McpAction::Disconnect))
            .await
            .is_err()
    );
    assert_eq!(events(&old, "spawn"), 1);
    assert_eq!(events(&new, "spawn"), 1);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
