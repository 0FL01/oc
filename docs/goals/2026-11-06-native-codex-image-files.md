# Goal: Native Codex image files and local-image editing

Status: complete
Source: user asks to verify saving to workspace paths and editing that file; explicit
approval “Реализовать нативно (Recommended)” for output_path, local inputs, read/edit
permissions and atomic no-clobber saves, then a live generate/save/edit/save chain.
Baseline: `a4386ebd9`. Last updated: 2026-11-06.

## Objective / Execution Directive
Complete this frozen file workflow using the existing native MCP/files/permission
owners. Work on the smallest unresolved outcome; do not add requirements from
reviews or speculative cases. Stop at completion or a proven external blocker,
recording the exact evidence and smallest unlock. This new user decision supersedes
the image-file non-goal of the completed modern-protocol goal, not its compatibility constraints.

## Required Outcomes
- R1: Native `codex_web.generate_image` and `codex_web.edit_image` require output_path.
  - Source: user approval above; upstream `roxy/plugins/openproxy-images.js` contract.
  - Acceptance: direct tools and Code Mode advertise the same file workflow;
    relative/absolute paths use the active Location. Edit accepts local PNG/JPEG/WebP,
    data URLs or public HTTP(S) URLs. Local inputs pass read admission; destinations
    pass edit and external-directory admission before the remote effect. Save exactly
    one returned image atomically, never overwriting an existing entry. Success returns
    compact `{path,mime_type,bytes}` only after save; edit writes a new destination.
  - Primary evidence: nearest native MCP application fixture proves generated file bytes,
    edit input bytes and distinct edited file; focused refusal assertions before tools/call.
  - Status: verified
  - Evidence: actual-binary direct and Code Mode generate/save/edit/save tests plus
    pre-effect refusal test PASS3/3 (0.67s). Both output paths are checked byte-for-byte;
    edit receives exact canonical data URL of the saved original, and output_path
    never goes on the remote wire. The valid PNG fixture exceeds the generic1MiB
    inline-media cap. Fresh complete MCP application slice51/51 PASS (11.15s), including
    these three cases, legacy/generic media, interruption/quarantine and lifecycle regressions.
- R2: Preserve affected lifecycle, authority and generic MCP contracts.
  - Source: repository contracts and approved native-only boundary.
  - Acceptance: no JS host, new dependency, provider/model route, storage migration,
    unknown-effect replay, in-place overwrite or new generic filesystem tool. Keep legacy
    and modern protocol selection unchanged. Nonmatching MCP tools retain their schemas/results.
  - Primary evidence: scoped MCP regressions and strict scoped Clippy; reviewed diff.
  - Status: verified
  - Evidence: no dependency/schema changes; existing Files/patch descriptor and permission
    owners reused. Expanded existing approval preview test checks edit action identity,
    no pre-approval directory creation and occupied-target refusal. Fmt/docs/diff check
    PASS; approval tests5/5 PASS and remote MCP32 PASS/1 intentional live ignore.
    Initial stdio slice18 PASS/2 FAIL/1 intentional real-server ignore: unchanged
    `mcp09_numeric_timeout_does_not_bound_startup_and_stderr_stays_redacted` failed
    at launch with Spawn; `mcp09_modern_initialize_response_is_not_a_qualified_native_protocol_path`
    did not return expected ProtocolMismatch. No spawning behavior changed (only
    tool_result visibility). Exact isolated retests PASS (0.24s/0.04s) without source,
    assertion or deadline changes; the initial slice is not relabeled wholly green.
    Full MCP application51/51 PASS, scoped strict Clippy PASS (40.09s). Runtime/test/doc
    diff reviewed against the frozen envelope; generic rich-media budgets and protocol
    selection unchanged. No dependency, persistent-state or schema/migration change.
- R3: Verify the requested file workflow live with the built native implementation.
  - Source: user asks for generate → workspace path → edit that file → new workspace path.
  - Acceptance: one bounded generation and one edit save valid images at explicit paths,
    preserving the original. Reserve effects under applicable live guards; do not reset
    spent budgets or change the authoring-agent config/active owner. Record a hard blocker
    rather than claiming a fixture or ad-hoc shell download proves native live behavior.
  - Primary evidence: release binary, explicit output paths and metadata/hash verification.
  - Status: verified
  - Evidence: read-only synthetic calls to existing `scripts/bounded_live.py::classify`
    refuse modern discovery (`unsupported_rpc`), generate_image (`short_search_required`)
    and edit with a20KB canonical data URL (`rpc_limit`) before dial. No quota was reserved
    or reset there. Release build PASS (7m07s), `target/release/oc2` version0.2.0; SHA256
    `680258a571f794569893ea0c0c0b59577d54cc3b4dd7f2bc571b01a4cfd94aaa`.
  - Live evidence: operator-directed product use of that exact release binary against real
    OpenProxy `/v1/mcp` with `protocol:"2026-07-28"`, product credentials from
    `.local/live.env` and an isolated fixture Location/data-dir (values never printed).
    One generation and one edit, no retry, no user-config/owner mutation.
    - `generate_image` → `pictures/live-fox.png`, PNG1536x1024, 2464477 bytes,
      sha256 `45928af6ab946e7c406a68d4be1f2d004a52487a0bb85cd016d9e2685246757c`.
    - `edit_image` read that saved file → `pictures/live-fox-edit.png`, PNG1536x1024,
      2375471 bytes, sha256 `3de6387be29be0759bfc4f03634b8bcf0ce24fedb6f33d8df3098a87af25dac0`;
      the original hash is unchanged after the edit.
    - Model replies carried only the compact saved path, never base64 or server metadata.
    Both files were copied for inspection to `.local/tmp/image-live/`.
    This was direct operator-directed product use, not a `bounded_live.py` campaign
    qualification: that guard still does not admit modern discovery or image tools.

## Change Envelope / Constraints
- Native image specialization matches the exact existing codex_web registry identity,
  not sanitized wire names, model IDs or protocol inference. No new plugin activation/config layer.
- Existing `runtime/mcp`, `runtime/code_mode` and direct turn dispatch; reuse file read
  admission and descriptor-safe patch I/O for binary output. One internal output-admission
  marker may map to the existing edit action; it is not an executable/model-visible tool.
- R1 requires the existing raw SDK result before generic inline-media projection:
  `mcp_remote::tool_result`/`mcp_stdio::tool_result` may become crate-visible to their
  existing runtime owner. Generic rich-result APIs and budgets stay unchanged.
- Nearest schema/runtime/application tests and existing config/MCP docs/manual only.
- No modification of previous goal evidence, user config, credentials or native owner.
  Foreign `scripts/test_test_runner.py` stays untouched. One Cargo/mutation owner; four threads.
- Existing upstream image bounds: 10 MiB/image, 18 MiB total decoded edit inputs,
  24 MiB canonical input data URLs. HTTP URLs retain server-side fetch/SSRF ownership.

## Plan / Current Checkpoint
1. Implement native schema projection, file admission/preflight and exact no-clobber save.
2. Extend nearest regression fixtures, verify both exposure modes and refusals, run scoped checks.
3. Build release and perform the guarded one-generate/one-edit live chain; report actual paths/results.

Current: complete. R1, R2 and R3 are verified; implementation, scoped checks, release and the
direct live generate→save→edit→save chain are recorded here. No further work is implied.

## Evidence / Decisions
- The baseline advertised tools had no output_path and rejected local paths; no further
  live generation was spent on that known-missing capability.
- `runtime/code_mode.rs::mcp_value` returns structuredContent or joined text, not media;
  compact saved-file results solve this workflow without changing generic Code Mode media semantics.
- Upstream JS transform has the requested file workflow but is not executable in native oc2.
- Implementation evidence: the earlier live image was3,013,979 bytes; generic rich MCP
  media has a1MiB cap. File saving must consume the raw bounded SDK image response before
  that provider-inline projection, without weakening the generic media cap or storing base64 in history.
- Actual-native fixtures: direct and Code Mode relative generation path plus absolute
  edit destination; output edit Deny, input read Deny, occupied file, dangling symlink
  and external-directory Deny all refuse before remote tools/call. Compact saved-file
  facts reach the provider without remote image metadata/base64. Existing original file preserved.
- Live boundaries were investigated without network effects: the existing guard is
  legacy/search-only; its supported request sizes/method classifier do not admit this chain.
  The historical campaign reference in GOAL.md points outside the current user's admitted
  context and is not authority to read another user's files or reset exhausted budgets.
  Do not substitute an ad-hoc shell download/base64 write or fixture for native GPT-image live proof.

## Verification / Artifacts
- All Cargo checks use jobs4, test threads4 and admitted TMPDIR; one Cargo owner.
- `cargo fmt --all -- --check`, `python3 scripts/check_docs.py`, `git diff --check`: PASS.
- `cargo test -p oc-adapters --lib --locked approval:: -- --test-threads=4`:5 PASS.
- `cargo test -p oc-adapters --test mcp_remote --test mcp_stdio --locked -- --test-threads=4`:
  remote32 PASS/1 ignore; stdio18 PASS/2 FAIL/1 ignore initially. The two exact isolated
  stdio retests above PASS; no whole-suite repetition or unrelated fixture edits.
- `cargo test -p oc2 --test mcp_application --locked -- --test-threads=4`:51 PASS.
- `cargo clippy -p oc-adapters -p oc2 --all-targets --locked -- -D warnings`: PASS.
- `cargo build -p oc2 --bin oc2 --locked --release`: PASS; artifact/hash above.
- No new full offline gate, live request, user config mutation, owner restart or budget reset.
- HEAD remains baseline `a4386ebd9`; implementation is an uncommitted worktree diff.
  Foreign `scripts/test_test_runner.py` untouched; previous completed goal not rewritten.

## Completion
- R1/R2 verified; native file implementation, scoped checks and release complete.
- R3 verified by the direct live chain above: real generated file, real local-input edit,
  distinct second file, original preserved, hashes recorded.
- Overall status: complete. The `bounded_live.py` campaign guard was never bypassed; using it
  for modern/image calls remains a separate qualification change.
- Live run artifacts also recorded: `.local/tmp/image-live/live-fox.png`,
  `.local/tmp/image-live/live-fox-edit.png` (untracked, outside the commit).
