# T44 — paired Rename session and durable restart

Pinned OpenCode v2.0.12 registers `session.rename` with `ctrl+r`, the Commands entry **Rename session** and a focused `DialogPrompt` (`packages/tui/src/routes/session/index.tsx:819-832`, `component/dialog-session-rename.tsx:7-37`, `ui/dialog-prompt.tsx:52-140`). Native `oc` now has a real, owner-acknowledged rename: the owner checks the current Location and root, writes the manual title and event atomically, and the tab updates only after acknowledgement. Generated titles cannot overwrite a manual one. The 60-cell modal has a separate grapheme-aware editor, preserves the composer, and retains the edit on owner refusal. An unchanged oversized generated title cannot silently be truncated and persisted; manual titles are limited to 256 UTF-8 bytes with invisible/control/bidi input rejected (valid joined emoji retained). A native `/rename <title>` also directly submits an explicit title. Code and test-only runner: `5a37756`.

## Independent PTY evidence

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --rename-session true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-rename-20260924-qualified
```

Immutable `-01`, `-02`, `-03` and `-qualified` attempts retain earlier unequal and final captures. The original executable is SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`, pinned source `2670273ff17da96f85c5826ced57aa1b368754fa`; qualified native binary is `989b0242da7eb152bb376aff7426cb3b022aac48a88f6ba28eaaaa91edf0d35a`. Fixture hash `a816400f909a6b252465ece82967a72b0e41c42eec01e27ad72423d2f1557fbf`; terminal profile ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. The capture lock identifies the pre-commit HEAD plus dirty source manifest, executable, independent PTY inputs/protocol/VT/cells/PNGs and comparisons. Both sides report `RENAME_CHECKS_PASS` and `provider_contract=true`: Ctrl+R opens a prefilled dialog, the real keyboard replaces its value, Enter changes the tab, a natural Ctrl+D exit and bare restart preserve the renamed title without extra provider work. Each side has two transcript requests and one title request/completion before and after rename/restart.

`-01` prefilled dialog differed by 223/4800 styled cells and cursor; `-02` retained 171 styled-only differences after geometry/foreground correction. In `-03` and final `-qualified`, **both prefilled and edited dialogs are identical across the independent captures: 0/4800 differing styled cells, matching cursor and 0/647040 differing PNG pixels**. Whole-session and restored-session frames still differ (final `rename-after` and `rename-restored-session`: 3/4800 cells each, real elapsed-time digits); restored Home differs 6/4800 cells from the real app versions. The runner exits 1 for `DIFFERENT` whole-frame comparisons; this is not VIS01–24, VIS08 or VIS24 acceptance.

## Verification and limits

Final serialized `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet` passed with zero failures (235 TUI tests, 29 real `pty_t42` tests, 35 binary unit tests, existing opt-in live ignores). First serialized pass saw `ui05_interrupted_exit_is_nonsuccess` exit 0 instead of 130 while the suite ran concurrently with Clippy/build; isolated and full PTY reruns passed and a subsequent serialized full workspace rerun passed without changing assertions. Workspace fmt, all-target Clippy `-D warnings`, locked build, Node syntax, docs/progress structural checks and diff check passed. Owner integration tests cover unknown/child/foreign Location refusal, Unicode/length/invisible validation, SQLite-trigger rollback, durable reopen and generated-title precedence. Real PTY tests cover shortcut, palette, slash with argument, modal cancel/retained input, storage failure retry, Home/child/foreign/busy refusal and restart.

Remaining mismatch: upstream `/rename` with **no** argument requests title regeneration; native explicitly reports this path unavailable rather than faking a rename or silently issuing a provider call. Other Commands/Models entries, error/replay, width/PNG edge and S07/V08–V09/VIS gates remain open. Dynamic time, source-random Home examples and actual app version are not frozen or spoofed. The previously untracked `.opencode/` was not accessed.
