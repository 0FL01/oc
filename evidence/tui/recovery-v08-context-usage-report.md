# T44 — reported context separate from incomplete turn billing

## RECON and implementation

The pinned original v2.0.12 shows `6.8K (3%)` on the live and reopened read-tool session. In the isolated paired fixture (`scripts/tui_capture/bridge.py`), the tool-call Responses generation reports **no** usage, while the following text generation reports `(input_tokens=6000, output_tokens=763)`; the catalog context limit is 225,000. Native previously kept complete-turn `usage=None`, correctly declining to claim the missing first round's billed output, but displayed `shift+tab agents` instead of the known generation measurement. `2d22a81` records a separate `context_usage` containing only the latest *reported* generation pair, projects it through safe durable turn metadata, and uses it for the TUI context footer. Full-turn `usage` and its `tok/s` claim are unchanged and remain absent for incomplete billing. A missing later generation never erases an earlier reported pair; no estimate or hardcoded model/limit is introduced.

Adapter regressions cover a missing first round, missing final round, incomplete and cancelled turns, post-restart storage projection, and the absence of a fabricated billed total. TUI regressions cover the 225,000-token catalog limit, reported `(6000,763)` yielding `6.8K (3%)`, no fabricated `tok/s`, priority over an independently complete turn usage, and the legacy no-context fallback. In-flight turns without a separate reported context event retain the prior behavior; the completed live turn reloads its durable page and replay uses the same projection.

## Independent paired capture

Fresh immutable `recovery-v08-context-usage-01/` captured the installed pinned executable (`v2.0.12`, commit `2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`) and a rebuilt Rust binary under isolated per-side HOME/XDG/data, a common read-only fixture project and the same terminal profile. The lock records Rust binary SHA-256 `b6e9f8bb6304404c828a25a5904b0fb06be0999ec76f657cb3ba3d5b216b5552`, source HEAD `1b36413` plus dirty source hash `2b7e0714010aaf7f772cf0c449c9dab42d4597aab4f6ba467a2496d2c229a9be`, fixture SHA-256 `546109431a6bb5d0edfe5bf1d8884fa0471983985afbe4767b70579f560f1624`, and terminal profile `e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`. Both sides report `provider_contract=true`, `TAB_RESTART_CHECKS_PASS`, natural exit, two real histories reopened by mouse without new provider requests, exactly four transcript and two title requests/completions each before quit and none after.

Command from the repository root, with a **fresh** output path:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true --tab-restart true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-context-usage-01
```

For both reopened sessions the complete rendered row 39 reads the same on both sides, including `6.8K (3%)`. `scripts/tui_capture/check_region.py --rect 88 38 32 1` reports **0/32 styled-cell differences** in that context/control region for old and second history. The whole frames are still **DIFFERENT**: reopened old 114/4800 cells (51/647040 pixels), reopened second 115/4800 cells (113/647040 pixels), restored Home 6/4800 cells (independently selected same Home example, real app-version glyphs differ). On restored session frames 113 styled-only mismatches are blank foreground at (3–4,3) and (5–115,34); measured elapsed-time glyphs differ as measured, not frozen. The paired runner exit is 1 because whole-frame comparison fails; no VIS PASS is inferred from the matching footer rectangle. Input sequences, per-generation VT/protocol, styled cells, PNGs and full comparators are retained without masks.

## Checks and remaining scope

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet` PASS 0 failures (206 TUI, 191 adapter library, existing opt-in live ignores); `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo build --locked`, `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`, `git diff --check` PASS. T44 remains active. VIS01–VIS24, V08/V09 and outstanding S07 qualification remain open, notably resize/edge-width and dialog matrices; dynamic clocks, Home example randomness and real app version must not be spoofed. The pre-existing untracked `.opencode/` was not opened or modified.
