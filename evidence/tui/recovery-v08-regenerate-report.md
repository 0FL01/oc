# T44 — genuine bare `/rename` regeneration

Pinned v2.0.12 dispatches bare `/rename` to a provider-backed title-generation request, distinct from the manual Ctrl+R dialog and `/rename <title>`. Native `oc` now admits an attached root in the current Location, reads bounded user/assistant text without tool results, resolves the owning session's selected model and configured title agent, validates the request budget, and issues a tool-free Responses request. The title update and event share a transaction. The update compares both the previously observed title and its title-event sequence: a concurrent manual A→B→A rename cannot be overwritten. A title-specific cancellation does not cancel a conversational turn; closing the initiating tab cancels the title request only after the tab close succeeds. Provider refusal, cancellation, invalid selection, budget failure and a concurrent rename retain the prior title and editable slash draft. No fabricated turn, turn usage or title is emitted. Code and test-only runner: `8901e1d`.

## Paired evidence

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --rename-session true --regenerate-title true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-regenerate-20260924-current
```

Four earlier immutable attempts (`-01`, `-02`, `-03`, `-qualified`) and the final current-code attempt are retained. The first two exposed an original-app autocomplete interaction in the runner; the runner now dismisses suggestions before submitting bare `/rename`. The final capture pins original v2.0.12 binary SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` (source `2670273ff17da96f85c5826ced57aa1b368754fa`) and native executable SHA-256 `079141e507aa0d73b767f5ce5a4aaa99abb313df5430366f51001db843292516`, built from recorded HEAD `f4282d4` plus dirty source manifest `84d9b6937c8f3ba51424eddbfa811e5a29f68501377961da3173dc8be7f5d51e`; fixture SHA-256 `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`, shared terminal profile `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. Each side has independent PTY/XDG and actual input/protocol/VT/cell/PNG artifacts.

Both sides report `REGENERATE_TITLE_CHECKS_PASS` and `provider_contract=true`. After manual rename, bare `/rename` made exactly **one additional completed title provider request** per side (two transcript and two title requests/completions total), changed the painted tab to the distinct provider title, and survived clean Ctrl+D exit/restart with neither new transcript nor title request. The prefilled and edited dialog frames remain **EQUAL** (0/4800 styled cells and 0/647040 pixels). The regenerated and restored session frames remain **DIFFERENT** by 2/4800 styled cells and 123/647040 pixels, localized to real elapsed-time digits at x38–39,y10. Restored Home differs by six real version glyph cells (native 0.1.0 versus original 2.0.12); no time/version/random example is spoofed. The runner exits 1 for these unmasked whole-frame differences. This is a bounded behavioral qualification, **not** a VIS01–24 or full T44 pixel-parity PASS.

## Checks and remaining scope

On the final code, the serialized locked full-workspace test suite passed (235 TUI, 30 real `pty_t42`, 37 binary and 193 adapter unit tests; existing opt-in live tests ignored). Workspace fmt, all-target Clippy `-D warnings`, locked build, Node syntax, docs/progress checks and diff check passed. Targeted tests verify SQL-trigger rollback, title-event ABA protection, current-Location/root/model validation, timeout/cancellation, no artificial turn, provider request content and prompt bounds, tab-close cancellation without cancelling the surviving tab's turn, and refusal to close without cancelling title work.

T44 remains active: Commands/Models capability breadth, VIS05 last-pixel PNG edge, error/replay, resource cases, and full VIS/V08–V09 qualification still require independent paired evidence. No `.opencode/` or live credentials were used.
