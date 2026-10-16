# Zen removed from the product (owner decision)

## Result

The owner decided to drop OpenCode Zen from the product entirely. Removed in this slice: `crates/oc-adapters/src/zen_catalog.rs`, `crates/oc-adapters/src/zen_chat.rs`, `crates/oc/tests/zen_free.rs`, the Zen wiring in `config.rs`, `composition.rs`, `runtime.rs`, `application.rs`, `provider.rs` and `lib.rs`, and the scope document `docs/goals/2026-09-23-zen-free-chat.md`. `GOAL.md`, `docs/TEST_PLAN.md`, `planning/tasks.json` and `planning/acceptance.json` are back to their pre-Zen state (no `opencode` provider, no ZEN01–ZEN03, no T48 registration). Decision D18 in `docs/DECISIONS.md` records the removal and keeps the general principle that `oc` never mimics another client or bypasses a provider's access control. T48 journal leaves and `evidence/T48/*` stay on disk as history only.

## Checks

`python3 scripts/check_docs.py`, `python3 scripts/progress.py reindex` and `check` exit 0 after the removal; the generated handoff no longer lists T48. `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test -q --locked --workspace` and `cargo build --locked` are run in the same slice. `rg -i zen` over tracked code, GOAL, TEST_PLAN and live planning registries reports no remaining product references; only the archived journal/evidence files keep the name.

## Risks

The removed code was self-contained and is restored from Git history if needed. The archived T48 files are no longer validated by `progress.py` (no registry entry), so they are documentation only. Nothing in the remaining product depends on Zen; OpenProxy remains the native live path.

## Next

Resume the pre-existing backlog: T44 (owner-paused pixel parity) and the ready backend tasks T45–T47. Any future real-provider work (paid Zen key, OpenCode Go, local OpenAI-compatible server) starts as a new, separately approved scope with its own acceptance IDs.
