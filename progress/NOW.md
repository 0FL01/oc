# NOW — актуальный handoff

State updated: 2026-10-16T00:55:21+00:00
Active: T65

Сверить Git status/diff до выполнения команд.
Task: T65 — Native V2 provider/cache/transport and ordered permission semantics
Spec: docs/goals/2026-10-15-v2-config-semantics.md
Evidence target: evidence/T65/report.md

Owner plan 2026-10-15 pauses T44 until this configuration slice is implemented, verified and normally committed/pushed; subsequent explicit authorization also requires a coherent version bump, crates.io publication using existing Cargo credentials without printing them, and isolated registry install verification before returning to full TUI parity. Configuration oracle: TS V2 at b4f19bfe74a972c2919aa4c9d7445fa61f41b226 (before the upstream fifteen-minute chunk default), separate from frozen TUI2670273. Separately merge provider/model documents then effective compatibility provider defaults→model overrides preserving explicit false across protocols; remove settings.setCacheKey runtime gate, preserve tri-state/protocol cache-key/cache:none/header-affinity/Anthropic policies; keep timeout/headerTimeout/chunkTimeout/transport provider-only and separate HTTP300000ms header/chunk, false-disabled and optional total deadlines, with WS-specific behavior and cancellation. Ordinary permissions use TS ordered overrides (agent defaults→top-level docs→agent-specific docs→session handling), while saved allow cannot bypass final configured deny and authority/sandbox/delegation ceilings remain non-widenable. V1 remains legacy_config before effects; valid unsupported V2 capability and malformed V2 value diagnostics stay distinct, arbitrary package settings are not legacy by name. Shared synthetic V2 fixtures/expected results for covered TS/Rust subset; existing redaction, admission, cancellation, MCP/package/Markdown/authority regressions and affected/full Cargo gates. No real credentials in fixtures/user config/secrets.env changes, V1 migration, plugin host, shared service or full application/built-in-agent parity. Preserve T44 stash1087067b3025322885c6a964579d80cea4931b06 for the explicit return after delivery.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T65 — actual0.2.0 publication receipt

## Result

**All four0.2.0 crates are published.** Verified source/checkpoint commit
`feeea77fc` was normally pushed before the single actual publication command.
No release/tag or default-user installation was created. Configuration gates
are green; isolated installed-binary verification is still running, so R8/T65
completion is not claimed yet.

## Checks

`cargo publish --workspace --no-verify --locked`: PASS. Prior latest dry-run
verification built every extracted package against staged registry dependencies;
production source, manifests, tests/assets and packaged README were unchanged
between verification and upload; Cargo's VCS metadata now names the clean checkpoint.
Cargo uploaded, waited for availability and reported Published in dependency order:
core → adapters → TUI → oc2. Log: `.local/evidence/t65-publish-actual-01.log`.

Independent public crates.io version API reads returned200 for every package,
each version0.2.0, unyanked and published_by0FL01, rust_version1.93:

| Crate | Published UTC,2026-10-16 | Public archive bytes | License | Binaries |
| --- | --- | ---: | --- | --- |
| oc-core | 00:49:24.525624 | 72543 | MIT | none |
| oc-adapters | 00:49:27.871314 | 1477829 | MIT AND AGPL-3.0-or-later | none |
| oc-tui | 00:49:31.732725 | 8057839 | MIT AND AGPL-3.0-or-later | none |
| oc2 | 00:49:35.316828 | 457551 | MIT AND AGPL-3.0-or-later | oc2 only |

Fresh approved scratch work/HOME/install roots were created without replacing
an existing installation. Registry-only installation is running from outside the
checkout with `cargo install oc2 --version 0.2.0 --locked --registry crates-io`,
explicit isolated `--root` and the existing compilation cache reused solely as
`--target-dir`. No path/git package selection; environment excludes provider
credentials. Log: `.local/evidence/t65-registry-install-01.log`.

## Risks

Publication is an observed completed external effect: **do not repeat it**.
No credentials were printed; existing Cargo authentication was used unchanged.
The old locked yanked-dependency warning remained visible. This is not full
product, built-in-agent or TUI parity. T44's stash remains intact and unapplied.

## Next

Wait for the managed registry-install completion; inspect its actual result,
then run the bounded installed-ELF check in a fresh HOME/project/data root and
empty PATH. Require actual version/help/smoke, a local synthetic headless turn
with canonical V2 cache configuration, and persisted-session listing with no
new generation. Record the receipt, finish T65 and resume T44/restore its parked
WIP once after inspecting Git. No further upload is needed.


Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
