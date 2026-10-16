# R2 / T43 — resource-aware backend permissions

Date: 2026-09-22. Base HEAD: `15719e976030a8fdaadd843ac13556de237f559d`.
Uncommitted additive backend slice under
`docs/goals/2026-09-22-backend-blockers.md`. No T43/T45 completion claim.

## Reference and resolved contract

Inspected local upstream v2.0.12 commit
`2670273ff17da96f85c5826ced57aa1b368754fa`, specifically:

- `packages/core/src/permission.ts`: last matching action/resource rule wins;
  unmatched resource defaults to ask; every requested resource is checked.
- `packages/core/src/v1/config/permission.ts`, `config/normalize.ts`, and
  `v1/config/migrate.ts`: preserve input property order; legacy scalar/resource
  maps and native action/resource/effect arrays; renamed tool actions.
- `packages/core/src/util/wildcard.ts`: `*`, `?`, separator normalization and
  optional arguments for a trailing ` *`.
- `packages/core/src/file-access.ts`, `tool/plugin/{patch,shell,subagent,glob,grep,webfetch,skill}.ts`,
  `tool/mcp.ts`, and `config/plugin/agent.ts`: actual resource values and
  path-only home expansion.

Native authority boundaries remain stricter than upstream's flattened merge:
rules within one source are ordered, but overlapping central sources are
intersected; selected-agent and each child-agent layer only narrow the caller.
An absent central action denies. An action with no matching resource asks.
`ask` returns a distinct approval-required error and never dispatches; this
runtime still has no interactive approval channel.

## Implementation

- Added `permissions::PermissionRules` with ordered rules and independently
  intersected constraints. Supports `permission` scalar/maps, native
  `permissions` arrays, and existing native-profile `permissions` maps.
- Enabled `serde_json/preserve_order`; Cargo.lock adds the already-locked
  `indexmap` dependency edge. No new package/version was introduced.
- Kept the old scalar permission map as a compatibility summary. The runtime
  uses `permission_rules` for parsed configuration, not the folded summary.
- `tools: false` is an independent denial; `tools: true` does not create a grant.
  `write/edit/patch`, `shell`, and `task` map to native `apply_patch`, `bash`,
  and `subagent` actions respectively.
- Home expansion uses the snapshotted HOME for `read`, `apply_patch`, and
  `external_directory` rules; shell resources remain literal command text.
- Resource checks run at the durable runtime dispatch boundary, in the common
  built-in executor, and for each patch preflight path/rename destination.
- Child lanes inherit the immediate caller's complete rules, including earlier
  child constraints. They no longer rebuild authority from the root generation.
- Agent metadata accepts typed `hidden`, legacy `disable`, and native
  `disabled`. Disabled definitions are removed, including a shadowed earlier
  definition. Hidden agents remain explicitly addressable but are omitted from
  the automatic catalog. Catalog entries are also filtered by the specific
  `subagent` resource permission (ask remains discoverable, like upstream).
- Agent digests bind ordered rules and hidden metadata.

### Actual resources

| Native tool | Permission resource |
| --- | --- |
| `read` | Normalized file path relative to the Location for internal paths |
| `apply_patch` | Every affected path, including both sides of rename |
| `bash` | Command represented from argv, preserving argument boundaries with quoting |
| `subagent` | Requested agent ID |
| MCP | Literal `*`; registered upstream `server_tool` identity and native wire identity match the same policy |
| `glob`, `grep` | Search pattern, not the directory |
| `webfetch` | Requested URL |
| `skill` | Skill ID |
| `compress` | `*` |

Native shell remains argv-only: a `sh -c` invocation must itself be authorized;
its payload is not mistaken for an allowed bare command. This slice does not
add upstream shell parsing or widen the existing trusted-root filesystem API.
The native file executor still requires its admitted relative path form and
refuses outside-root access even if an external-directory rule allows it.

## Evidence

New `tests/permissions.rs` covers ordered maps and native rules, unmatched
resources, Ask diagnostics, source/agent/tool non-widening, patch rename,
shell/subagent/MCP identities, home expansion/wildcards, and JSON/markdown
metadata admission. JSON and markdown order tests deliberately place a later
`*` after an earlier lexically-later key, so sorted-key parsing would fail.

New fake-provider runtime regression
`resource_permissions_gate_real_dispatch_before_side_effects` proves allowed
read/patch/shell execution, blocked private reads, refused rename destinations,
approval-required unmatched commands, unchanged source files, absent marker
files, and no private read contents in the continuation request.

New fake-provider subagent regression
`nested_resource_constraints_and_filtered_catalog_remain_inherited` proves a
permissive grandchild cannot regain the parent's denied command, while an
explicit allowed command still executes. The advertised catalog omits both
hidden and permission-denied agents.

Checks (all final listed results exit 0):

- `cargo test --locked -p oc-adapters`: 292 passed, 3 existing live ignores.
  This package-wide run preceded the final path-home expansion addition.
- Final `cargo test --locked -p oc-adapters --test permissions --test runtime --test subagent`:
  7 + 39 + 12 passed, zero ignored/failing.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`.
- `cargo build --locked -p oc`.
- Rustfmt of the edited backend files; `cargo fmt --all -- --check` and
  `git diff --check`.

Earlier failures were retained during implementation: initial compilation
intersected incomplete concurrent MCP edits; `--locked` identified the required
preserve-order lock edge (updated offline); integration exposed the patch
bridge's second action-only check, now corrected. The existing malformed
`tools: true` definition test now asserts an invalid-shape diagnostic rather
than an unsupported-field diagnostic; invalid input is still rejected. The
existing missing-read-path diagnostic was preserved after the resource hook
initially changed it.

## Interfaces and handoff

New fields: `Generation.permission_rules`, `AgentDef.permission_rules`,
`AgentDef.hidden`, `SubagentAgent.permission_rules`, `SubagentAgent.hidden`.
Programmatically constructed scalar-only generations can use default rules.
Existing fixtures in context_bounds/e2e_live/e2e_offline/soak/runtime/subagent
receive these defaults without changing their assertions.

`ToolPolicy` adds default `check_resource` and `check_call` methods.
`ToolError` adds `ApprovalRequired`. Resource-aware runtime consumers must use
`RuntimePolicy::with_rules`, with Location and MCP registry bindings as needed;
`RuntimePolicy::new` remains scalar-only compatibility. Concurrent MCP guidance
filtering takes this same policy object and therefore observes resource-aware
MCP authorization. Other agents' model/MCP edits were preserved.

Remaining: interactive approval, broader filesystem/shell API parity, and the
separate T45 orchestration scope. This report does not qualify those features
or close the full backend milestone. Parent owns registry/decision records and
final combined qualification. Protected T44 files/state/evidence were not
edited; no commit or push was made.

## Review follow-up — effective primary policy and literal wildcard resources

Same uncommitted base HEAD; 2026-09-22. Two review defects were reproduced before
the fix:

- `literal_stars_in_resources_cannot_bypass_wildcard_denials` failed because
  `secret*` did not match the actual resource `secret*keys` (exit 101).
- The extended `resource_permissions_gate_real_dispatch_before_side_effects`
  actually read and deleted `safe/secret*keys` despite its deny rule (exit 101).
- `primary_selection_and_restore_narrow_dispatch_guidance_and_children` failed
  because startup `review` constraints remained after API selection of `build`
  (exit 101). The fixture also exercises selection back to `review` and restart
  with persisted `review` while the configured default has changed to `build`.

Minimal corrections:

- Composition now retains central configuration authority independently of the
  startup agent. Its former scalar and resource-aware startup narrowing were
  removed; explicit DCP module authority is still intersected as before.
- Application workspace publication resolves the effective selected agent from
  the immutable composition and snapshots its scalar/rule constraints together
  with the prompt, ID and digest. This path serves startup, API selection,
  persisted restoration and Location publication.
- Runtime workspace stores those constraints, expanding path HOME references
  using the runtime's snapshotted environment. Every primary lane intersects
  central rules with this snapshot. Existing MCP guidance filtering and child
  inheritance consume that same lane. Manual `run_compress` now uses it too.
- The wildcard matcher handles the pattern's `*` before literal equality, so an
  input `*` cannot consume the wildcard as if it were a literal and bypass its
  backtracking behavior.

Final regression observations:

- The application test proves startup denial; `review` → `build` restores only
  centrally admitted bash/MCP use; API `build` → `review` denies both tools;
  a permissive child inherits both denials; and restored non-default `review`
  denies tools even with no spawnable agents/catalog. Only the `build` requests
  contain MCP initialize guidance. One actual MCP call and only the `build`
  shell marker are observed.
- Literal-star read and delete attempts now fail before side effects, retain
  the original file and keep its read canary out of the provider continuation.
- `primary_workspace_policy_also_bounds_manual_compress` proves denial stores
  no operation/block, then replacing the selected workspace with an unconstrained
  primary permits the same valid compression under central authority. Its first
  fixture run reached the existing unfinished-tail guard on the allowed case;
  adding the subsequent user message made the intended historical range valid.
  No production DCP validation or protection was changed.

Checks after the review corrections (final results exit 0):

- `cargo test --locked -p oc-adapters --test permissions --test runtime --test subagent`:
  **8 + 40 + 12 passed**, no failures/ignores. This run preceded the additional
  manual-compress regression below.
- `cargo test --locked -p oc-adapters --test runtime primary_workspace_policy_also_bounds_manual_compress -- --exact`:
  **1 passed**, 40 filtered out.
- `cargo test --locked -p oc-adapters --lib`: **161 passed**, no failures/ignores.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`.
- `cargo build --locked -p oc`.
- `cargo fmt --all -- --check`; `git diff --check`.

Follow-up files: `src/{application,composition,runtime,permissions}.rs`,
`tests/{permissions,runtime}.rs` under `crates/oc-adapters`, and this report.
Backend API delta: `Runtime::publish_workspace` appends
`agent_permissions: BTreeMap<String, Permission>` and
`agent_permission_rules: PermissionRules`; its application caller supplies the
effective agent's constraints. Core/TUI DTOs have no shape changes. Existing
concurrent model/MCP hunks are retained, including their newer title-budget work.
Review covered the combined permission parsing, metadata, tool resource mapping,
authority evaluation, selected-primary publication and child-policy paths.
No further correction is claimed for interactive approval, native argv/root API
limits or T45 orchestration. No commit or push was made.
