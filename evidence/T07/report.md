# T07 — User config и permissions

Status: PASS. Implementation commit: `c8c97b060562dda76ba0e02f139e92bc5a2da40f`. Method: offline `cargo` unit execution with inline JSONC/TOML fixtures; no network, no live requests, no Docker, no command/plugin execution.

## CFG01 User forms — PASS

- JSONC comments/trailing commas stripped outside strings (`strip_jsonc`, port of `check_docs.py` semantics); `timeout:false` and `chunkTimeout:6000000` preserved as `Some(false)`/`Some(6000000)` (never defaulted away); `ludka` static models map vs `ludka2` discovery absence distinguished; native `oc-rs.toml` subset (`profile`, `tool_exposure`) parsed via `toml` without activating future modules.

## CFG02 Substitution/merge — PASS

- `{env:VAR}` expands (missing → empty); `{file:path}` requires trusted source with no-follow + 64 KiB bound, else `Untrusted`. Later source wins per provider/MCP id with per-section provenance; permissions merge most-restrictive (`Deny > Ask > Allow`); inputs verified byte-unchanged; selected-provider empty key → `MissingCredential` (test uses `{env:ABSENT_KEY}` template).

## CFG03 Capability failures — PASS

- Unknown `npm`, explicit `codemode:true`, `oauth:true`, DCP `allowSubAgents`/`customPrompts:true`, non-`range` compress mode all give typed `UnsupportedCapability` with field + reason before any side effect.

## CFG04 Trust/secrets/disabled — PASS

- Untrusted `{file:}` refused pre-read; disabled `chrome-devtools` needs no credential and launches nothing; `explain_redacted` replaces `apiKey`/headers with `***` while keeping provenance; no secret values in error strings.

## CFG05 Config roots — PASS

- Order fixture cross-checked; later file wins per provider id with provenance; JSON-before-JSONC and single explicit `--config` slot documented.

## CFG06 Instructions — PASS

- Ordered `G → Location` sentinel-once rule asserted; generation provenance mirrors per-section source tracking.

## CFG07 Definitions — PASS

- `SKILL.md` frontmatter parsed without YAML engine (bounded `name`/`description`, size caps, visible errors); `legacy_key` maps `write`/`edit` → `apply_patch`; permission merge deny-wins proven.

## CFG08 Native plugins — PASS

- Exact `classify_plugin`: bare + pinned DCP → `dcp`, canonical `<root>/{plugin,plugins}/openproxy-models.js` → `discovery`, no JS execution; `@latest`, foreign roots, URLs all `UnsupportedPlugin`.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0 (fixed `derivable_impls`, `op_ref`, `collapsible_if`; renamed `gen` binding — reserved in edition 2024; renamed thiserror `source` field → `origin`).
- `cargo test -p oc-adapters config` 8/8; workspace 44 total (oc 4 + adapters 20 + core 13 + tui 7); `cargo build --locked`, `oc --help`, `check_docs.py` exit 0.
- New dep `toml 0.8.23 MIT OR Apache-2.0`; no command/plugin execution, no future-module activation.

## Scope and limitations

- File-backed root discovery (walk `G` + Location `.opencode`), AGENTS body loading, and full skill/agent/command catalog Listing remain for dedicated follow-ups reusing this parser/policy core; T07 covers parsing, trust, merge, validation, and classification.
- Real `ENOSPC`/credential-store integration deferred; quota/provenance/redaction proxies proven here.
