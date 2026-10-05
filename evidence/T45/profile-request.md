# T45/R6 — profile request headers/body overlays

Atomic slice of R6 ("request settings/headers/body … have actual execution
semantics, not silent acceptance"). Not a T45 closeout; color presentation is
the separate next R6 atomic.

## Donor reading (pinned `2670273ff17da96f85c5826ced57aa1b368754fa`)

- `packages/schema/src/config/agent.ts`: `request: ConfigProvider.Request`.
- `packages/schema/src/config/provider.ts:42–45`: config `Request` admits only
  `headers` (string map) and `body` (JSON record); there is no config
  `request.settings`.
- `packages/core/src/config/plugin/agent.ts:111–114`: later definitions merge
  headers/body by key (`Object.assign`).
- `packages/core/src/v1/config/migrate.ts:141–160`: V1 `options`, then
  `temperature`, then `top_p` become `request.body`.
- The pinned core request path stores `agent.request` but never applies it to
  the outgoing model request. R6's frozen acceptance requires actual
  execution semantics, so applying the overlay is a native difference.

## Native behavior

- `defs.rs::agent_request`: JSON `agent`/`agents` and Markdown frontmatter admit
  `request.{headers,body}` plus V1 `options`/`temperature`/`top_p`; native
  body fields win. Header names are lowercased. Supplied-field merge is
  key-level across sources.
- `provider.rs::RequestOverlay::validate` refuses, at load time with a
  diagnostic and no profile registration: `request.settings`/other request
  keys, non-object shapes, non-string header values, invalid header names or
  values, routing/auth/framing headers (`authorization`, `accept`,
  `content-type`, `host`, `content-length`, …) and runtime-owned body fields
  (`model`, `store`, `stream`, `input`, `include`, `max_output_tokens`,
  `tools`, `reasoning`, `prompt_cache_key`). Overlay is bounded to 64 KiB.
- Root lane uses the selected primary profile's overlay; a child lane uses only
  its own profile's overlay (no parent leak). Title/compaction requests are
  unchanged. Overlay header values are sensitive, excluded from `Debug`, never
  persisted with the lane, and participate in the existing error redaction.
- `agent_digest` covers the overlay, so profile changes are visible to the
  existing lane/digest reconciliation.

## Checks

- `defs::profile_tests::r6_request_overlays_merge_by_key_with_legacy_migration_and_explicit_refusals`
- `application::profile_tests::r6_loaded_profile_request_overlay_reaches_child_catalog`
- `tests/subagent.rs::r6_profile_request_overlays_reach_root_and_child_requests_only`
  (fake provider: captured root/child/root bodies and headers).
- `cargo fmt --all --check`, strict `cargo clippy --workspace --all-targets
  --locked -- -D warnings`: PASS.
- Full `cargo test --workspace --locked --no-fail-fast` (3 jobs / 2 threads,
  bench TMPDIR outside the worktree): **1551 passed / 1 failed / 10 ignored**
  (baseline 1549 + 3 new owner tests). The single failure is
  `webfetch::format_tests::tool17_held_body_deadline_cancel_and_conversion_join_keep_executor_responsive`;
  it fails identically 3/3 on a clean detached HEAD `7ba015dca` worktree with
  `cargo test -p oc-adapters --lib webfetch` (shared static `ACTIVE` counter
  raced by concurrent webfetch conversion tests). Pre-existing, outside this
  slice; not disabled, retained as an open test-isolation risk.
- Environment note: a TMPDIR inside the worktree breaks git/Cargo fixtures, and
  a short TMPDIR path makes `pty_t39::inherited_selection` path-privacy
  assertions match visible text; both reruns were discarded as environmental.
