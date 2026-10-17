# Portable native-binary fixtures (2026-10-17)

## Result

Owner requested fixing the seven old-account temp-path failures and commit/push.
Base: `ef09abb2088aa907b11008ba2e3f0d5e8b6cd1ab`.

Rust fixtures use the standard temporary-directory API. Python harnesses receive
the caller's selected temp root explicitly after `env_clear()` and resolve it
with `tempfile.gettempdir()`. The same one-line fix covers the feature-gated fake
OAuth harness. No new helper/framework, production code, credentials, schema or
user configuration changes. The optional frozen-reference binary path is not a
temporary directory and is deliberately outside this correction.

Two newly reachable fixture defects were reproduced and fixed without changing
their expected outcomes:

- Initial PTY collection used a fixed 550/650 ms sleep. A bounded diagnostic saw
  the first frame around 618 ms (`runtime.ready` at 542 ms). Startup/discovery now
  wait for the actual expected frame within five seconds, then retain their
  original steady observation windows. Shutdown/Settings deadlines and every
  secret/error/process/terminal-restoration assertion remain intact.
- Python's default ASCII escaping produced different Unicode preference-key
  bytes from Rust's `serde_json`. The two seeded tuple keys now use UTF-8 JSON;
  the same corrupt-selection assertions then exercise the intended Location.

## Checks

- All seven affected tests PASS under a new temp root containing a space and
  Unicode: auth CLI/TUI, four recovery/startup cases, and real terminal lifecycle.
- Fake OAuth `--features auth-fixture --test auth_oauth`: 1 PASS in the same root.
  Only owned local fake issuers/providers are used, no real keys or paid calls.
- Python AST, workspace fmt and strict workspace/all-target Clippy: PASS.
- First full run used a longer private temp root and two parallel cases:
  2038 passed / 3 failed / 12 unchanged opt-in ignored, 46 targets. Failures were
  shell flood-drain timeout, terminal cwd observation, and MCP routing request
  count. They are not omitted or attributed to this patch without evidence.
- Final `cargo test --locked --workspace --no-fail-fast`: **PASS**, 2041 passed,
  0 failed, 12 unchanged opt-in ignored cases across 46 targets. This run uses
  the standard approved temp root, one independent case at a time, all 16 CPUs
  available (no affinity cap), and 16 build jobs. It does not claim stability for
  every temp-root length or higher independent-case concurrency.
- Normal/default-feature `cargo build --locked`, `oc2 --help` and `--version`:
  PASS after the feature-gated fake OAuth test. Debug version is `oc2 0.2.0`.

Logs: `/home/stfu/.cache/opencode-tmp/opencode/portable-fixtures-TSIrCp/`.

## Risks

No test was skipped, no product performance threshold or validation was relaxed.
Only harness paths, readiness and fixture-key encoding changed. No whole visual
T44/READY claim; previous failed receipts remain historical rather than rewritten.

## Next

Deliver this reviewed correction with the green standard gate. Separate stress
or concurrency investigation must retain the failed run above; the temp-path fix
does not claim to explain every earlier failure. Full visual T44 remains open.
