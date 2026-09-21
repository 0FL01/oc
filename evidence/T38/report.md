# T38 report — shell supervision и webfetch egress

Findings: F12 (shell hangs/orphan pipes/loose child env/symlink cwd), F13
(webfetch Unicode panic, hand-rolled URL splits, unbound egress, per-request
deadline). Audit base `fc796830…`, repaired on top of T37 closeout `4574a2d`.
Contract: `audit/repairs/T38.md` (AUD25–AUD28).

## What changed

`crates/oc-adapters/src/shell.rs` (supervisor rewrite):

- `execute` starts the deadline **before** `spawn`; every wait (stdin writer,
  drains, child exit, teardown) is deadline- and cancel-aware.
- stdin is written on its own thread; stdout/stderr are drained concurrently
  through a shared `Drain` state (`bytes`, `truncated`, `done`). Past the retain
  cap the pipe keeps being read and discarded with a counter, so a flooding child
  never blocks on a full pipe.
- Leader exit is no longer treated as completion: drains get a bounded window,
  then `kill_group` (TERM → `kill_grace` → KILL, only the owned session group
  where pgid == leader pid) runs **before** `child.wait()`. Reader threads are
  never joined unboundedly; a partial outcome is returned instead.
- `ShellError::SpawnRefused` now carries the OS error text for a refused exec
  (`exec: <error>`), replacing the opaque `reap failed`.
- `resolve_cwd` canonicalises the candidate and requires `starts_with(root)`, so
  a symlink inside the root that points outside is `BadCwd`.
- `child_env` is a strict name allowlist (`PATH`, `HOME`, `TMPDIR`, `TERM`,
  `USER`, `LOGNAME`, `SHELL`, `CARGO_HOME`, `RUSTUP_HOME`, `XDG_*`, `LANG`,
  `LC_*`) with working defaults (`PATH=/usr/bin:/bin` fallback, `LANG=C.UTF-8`).
  Unlisted names are dropped even when harmless, so no substring heuristic is
  relied on for credential safety.

`crates/oc-adapters/src/webfetch.rs`:

- `html_to_text` scans only ASCII delimiters (`<`, `>`, `;`), so multi-byte UTF-8
  is never split; named/numeric entities decode; script/style/comments are
  removed case-insensitively; malformed and unclosed markup is tolerated.
- URL handling goes through `reqwest::Url` parse + `base.join()`: relative,
  protocol-relative, query-only, fragment-only, IPv6; fragment stripped, userinfo
  and non-http(s) rejected.
- `GuardedResolver` implements `reqwest::dns::Resolve` and is installed with
  `ClientBuilder::dns_resolver`, so the **checked address is the dialled
  address**: if any resolved address is non-public the request is refused before
  a socket is opened, closing the lookup→connect rebinding gap. `BlockedAddress`
  is carried through the error chain to `PrivateHost`. Loopback is reachable only
  with the explicit test flag; `fetch_with_resolver` lets tests inject a resolver.
- One total budget spans DNS, every redirect hop and the body (`budget_left`);
  redirects are re-validated and re-dialled through the guard; the bearer header
  is attached to the first hop only and never forwarded across origins.
- Byte quotas are independent of `Content-Length`.

Tests added: `crates/oc-adapters/tests/shell_watchdog.rs` (AUD27 driver + child
cases) and new unit tests in both modules.

## Acceptance mapping

| Item | Evidence |
| --- | --- |
| AUD25 | `aud25_html_unicode_entities_and_malformed`, `aud25_url_relative_query_ipv6`; RED: `byte index 4 is not a char boundary` on `<p>Привет 🦀</p>` |
| AUD26 | `aud26_dial_bound_private_answer_sends_no_request` (resolver flips public→loopback between lookup and connect; the loopback server records zero requests), `aud26_total_deadline_spans_redirects` (400 ms budget over 5 delayed hops; RED was 1.559 s `TooManyRedirects`) |
| AUD27 | `shell_watchdog`: stdin-unread, output flood, leader-exit with pipe-holding descendant, TERM-ignoring child — all bounded under a 30 s watchdog; descendant marker proves the owned group is killed; RED: `was not bounded: 30.003498348s` and `owned descendant survived the group teardown` |
| AUD28 | `aud28_strict_allowlist_drops_unlisted_names` (RED: `MYAPP_OK` leaked), `aud28_symlink_cwd_escape_refused_and_toolchain_resolves` (bare `rustc`/`cargo` resolve in the production path; RED: child ran in `…/outside`); `e2e_offline`/`e2e_live` no longer inject the test-only `RUSTC` hint or absolute cargo argv |

## Checks

Full detail in `evidence/T38/checks.md`. Summary: targeted suites green
(adapters unit 129, `shell_watchdog` 2, `e2e_offline` 3, `soak` 4, `runtime` 31);
`cargo test --workspace --locked` exit 0 with 308 passed / 0 failed; clippy
`-D warnings`, `cargo fmt --check`, `cargo build`, `oc --help`,
`scripts/progress.py check`, `scripts/check_docs.py`, `git diff --check` all
exit 0. Exactly the three pre-existing external harnesses remain ignored and were
not run.

## Remaining risk

- The four AUD27 cases bound behaviour and prove group teardown, but they run on
  this Linux host only (process groups, `/proc`-free checks); no other platform is
  exercised.
- `e2e01_seeded_coding_fix` produced one non-reproducible `reap failed` before the
  diagnostics change (three consecutive reruns and the full workspace run are
  green). The supervisor now surfaces the OS error if it recurs; this is recorded
  as a risk, not as a pass.
- HTML extraction is intentionally a small scanner, not a spec-complete parser:
  exotic constructs (CDATA, foreign content edge cases) degrade to text rather
  than erroring, which is the documented contract.

## Next step

T39 (TUI panels/switch/commands, F14/AUD29–AUD31) per `audit/repairs/T39.md`.
