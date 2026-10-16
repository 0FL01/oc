# T02 — Минимальный source contract

Status: PASS. Implementation commit: `443555b148c9e14e9d3ec538a649d469901ba9af`. Method: documentary fetch of pinned upstream bytes to `.local` plus offline normalized fixtures and offline oracle execution. No paid requests, no credential reads, no Docker, no product runtime changes.

## Work — PASS

- Pinned sources verified: `anomalyco/opencode b8cedc1a7a5e2916bbb65dc1d4b620729c261638 MIT`, `Opencode-DCP/opencode-dynamic-context-pruning 11f6517780a502512a3467645074be447cb0369e AGPL-3.0-or-later (@tarquinen/opencode-dcp 3.1.15)`, `0FL01/openproxy 4ef76dbce2cdbb85206cbe5e59acbad9d96ae387 license unknown-no-copy`, user discovery `references/openproxy-models.user.mjs sha256 0f98ddad1660a991a400dbeba879df76ba2fc70b8e3fe530032976166939610c` (authority over repo plugin `28c7c8a60d17caec44c645a586c3568c847a649cb3e33cde4bd4881fd5d00cd7` with 10000ms timeout).
- Upstream bytes fetched with `curl -sL` to `.local/tmp/t02-upstream/` (gitignored): `config.test.ts (1611 lines)`, `config.ts (374)`, `skill.test.ts (416)`, `agent.test.ts (789)`, `command.test.ts (581)`, `command-subagent.test.ts (164)`, `instruction-discovery.test.ts (493, top-level test dir)`, `dcp-types.ts (108)`, `dcp-package.json`, `lean-proxy.md (106)`, `openproxy-mod.rs (2275)`, `codex_web_mcp.rs (256)`, `openproxy-models-repo.js (175)`, `LICENSE` files. `instruction-discovery.test.ts` under `test/config` is 404 (Not Found); correct path is `packages/core/test/instruction-discovery.test.ts` — recorded as executable baseline failure, not hidden.
- Fixtures created (8 + README): `config-roots.order.json`, `instructions.order.json`, `definitions.order.json`, `native-plugins.json`, `discovery-oracle.json`, `responses-wire.json`, `mcp-wire.json`, `dcp-range-schema.json`. Each carries `source_repo/commit/path/license/sha256/normalization`; OpenProxy entries use `unknown-no-copy` and copy no implementation; DCP entries record AGPL without porting code yet (full LICENSE acquisition deferred per D06 until first DCP-code push).
- Provenance manifest `planning/source-manifest.json` (schema_version 1) complements `planning/baseline.lock.json` with per-fixture revision/path/digest/license/normalization.

## Checks

- `python3 -c json-load all fixtures+manifest` — exit 0, `JSON_OK`.
- `node --test scripts/test_discovery_reference.mjs` — exit 0, 15/15 pass (new ID no-registry, 500k clamp, local override, atomic failure, 401 terminal, disabled no-request, last-wins, incomplete-limit delete, 503 bounded, credential-URL reject). This is an offline oracle test of the supplied JS reference, not a Rust implementation test.
- `python3 scripts/check_docs.py` — exit 0.
- `CARGO_BUILD_JOBS=2 cargo test --workspace --locked` — exit 0 (13 tests, unchanged from T01; T02 adds no Rust code).
- Fixture secret scan for live placeholders — no hits; `git diff --cached --check` exit 0.

## Scope and limitations

- T02 pins contracts and representative order/frontmatter/plugin fixtures only. No Responses adapter, discovery runtime, MCP client, DCP compressor, config loader or TUI behavior is claimed.
- All source-derived fixtures are marked `not-executed`; they are not differential parity. Executable baseline failure recorded: `test/config/instruction-discovery.test.ts` does not exist at pinned commit (correct path is top-level `test/instruction-discovery.test.ts`).
- OpenProxy license remains unknown; no OpenProxy implementation copied. DCP full LICENSE text stays in `.local` until code port; unrelated local work continues per D06.
