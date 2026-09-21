# T35 — configured workspace and discovery

Status: **PASS for T35 offline scope**, not whole-product READY.
Implementation: `320bf4c`; base `9daa55c`, no audit rollback.
Findings: **F08, F09, F17**. Acceptance: **AUD14–AUD18**.
Initial red: [regression.md](regression.md). Gates: [checks.md](checks.md).

## Delivered production behavior

The shared application composition now discovers the admitted global XDG/config
root and current canonical Location itself. JSON precedes JSONC in each layer;
global, direct project and project `.opencode` sources merge low-to-high. Sources
are immutable and trusted only after canonical boundary admission. Relative
`{file:...}` reads use source-directory `openat` traversal with no-follow,
regular-file and 64 KiB checks; absolute/parent/symlink paths fail closed.

Global and project AGENTS become ordered typed developer input once per request.
Global and project `.opencode` skills/agents/commands plus top-level `agent` and
`command` config domains build one bounded application snapshot. Invented
`<kind>.json` filenames were removed. Primary-agent body, variant and permission
narrowing affect the actual request/executor; its digest covers behavior and is
stored in the turn wire journal. A changed digest starts a fresh causality lane
from immutable raw messages instead of replaying old opaque/tool state.

Skills expose bounded metadata only; their bodies are snapshotted once and appear
only after the native skill call. Invalid selected skill IDs return their pinned
diagnostic. Commands are Markdown/literal templates, expanded once without shell,
recursive substitution or loader execution; original invocation remains durable.
Legacy write/edit normalize to apply_patch before restrictive merge and winning
provenance follows the effective source. Provider/MCP typed domains reject unknown
fields; disabled MCP entries do not resolve secrets or read files.

## Acceptance evidence

| ID | Executed evidence |
|---|---|
| AUD14 | `configured_workspace` launches separate actual `oc` processes in A/B sharing one global root/data. Captured typed requests contain global then correct local AGENTS exactly once; A content absent in B; global/local skill metadata follows Location. Reusing A session from B fails `belongs to location` before provider contact. |
| AUD15 | A/B primary profiles have equal descriptions and different bodies; captured fixed request lanes differ and contain the selected body. Digest unit changes with body/permission. Actual `/literal value$2 second` request retains Markdown backticks/code fence, expands `$ARGUMENTS/$1/$2` once, and no trap cargo/node/bun/npx runs. Missing positionals remain literal. |
| AUD16 | Actual model-generated apply_patch against temp projects is denied both by central allow + legacy write/edit deny and by selected-agent narrowing; no file is created. Source-relative trusted substitution tests pass, outside/symlink/untrusted reads fail. Unknown provider/MCP/agent/command capability fields diagnose before side effects; source bytes remain unchanged. |
| AUD17 | Actual binary admits duplicate bare/pinned/user-required DCP aliases without executing JS; one compiled identity is deduplicated internally. Skill source mutated after startup still returns old pinned body. Unknown plugin URL yields source-qualified `UnsupportedPlugin`, no DB/network/process. Selected malformed agent blocks despite valid sibling; malformed skill is warned and gives precise call result while valid sibling remains visible. |
| AUD18 | Locked user oracle regressions execute safe-integer max/reject 2^53 before clamp, first-slash multi-component names, standard URL validation, case-insensitive header replacement, custom headers, missing-key zero request, bounded empty/retry, successful any 2xx, body cap, removed IDs, metadata-only remote rows and static ludka >500k unchanged. Composition loopback proves configured discovery headers reach `/models`. |

All application tests use temp files/loopback and assert no package executables.
The final full workspace and build gates pass; exact counts are in checks.md.

## Explicit plugin compatibility decision

The user's shared global config uses exact `@tarquinen/opencode-dcp@latest`.
This spelling now maps to the repository-pinned compiled DCP revision, exactly like
bare/3.1.15; it never resolves npm and cannot change version at runtime. This
latest explicit user requirement supersedes older docs that rejected `@latest`;
current docs/fixture were updated, historical evidence was not rewritten.

The same shared file includes exact
`@prevalentware/opencode-goal-plugin@0.1.49`. Because the product goal excludes an
arbitrary plugin host and goal orchestration, this marker is recognized only as
authoring-only: visible warning, zero capability, no import/process/network.
Other packages/versions/ranges/URLs remain hard `UnsupportedPlugin` errors.

## Bounds and remaining scope

Definition limits remain: 8 roots, 256 entries/kind, 16 KiB agent/command body,
64 KiB skill/instruction file, 1 MiB total definitions, 256 KiB instructions.
Malformed siblings do not become defaults. Diagnostics contain paths/rules, never
secret values or bodies. Source configs are never written.

This does not claim full Location walk beyond the supplied canonical current
Location, config-authoring UI, subagents, executable command files or arbitrary
plugins. Model-visible compress/nudge and DCP graph behavior are T36; generation-
lifetime MCP/auth is T37; actual TUI pickers/Location controls/custom slash routing
are T39. No live provider call was made. T27 remains blocked until T42.

Next: start **T36** immediately via the existing progress journal. No push yet.
