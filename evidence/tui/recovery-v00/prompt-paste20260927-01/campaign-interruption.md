# Serial campaign interruption

The initial `run_prompt_paste.mjs` invocation completed, in order:
`chip-120`, `extra-120`, `repeat-nav-120`, `chip-79`, `extra-79`,
`chip-80`, `extra-80`, `chip-121`. Each capture runner exited 1 and kept
its original files and full comparator output.

Before starting `extra-121`, the exclusivity preflight observed a separate
`cargo build --release` (PID 1291315) and stopped without launching Cargo.
The initial campaign helper threw before writing its final `campaign.json`;
per-attempt commands/exit codes/locks and runner logs remain authoritative.
The helper now records a preflight blocker before exiting for future campaigns.
This file records the original interruption; it does not change the captures.

The first `repeat-nav-120` also exposed a fixture bug: native's actual outgoing
user message contained the exact typed raw multiline draft, but the bridge searched
for literal newlines in a JSON-serialized request. JSON uses escaped newlines,
so it wrongly rejected the main request with HTTP 400. Its actual failure frames,
request input, timeout and comparator results are retained. A new
`repeat-nav-120-02` will use a structured input-text comparison. No Rust change
is involved in that fixture correction.

The second attempt succeeded natively but retained an original failure: the
original actually sends one extra trailing space, inserted by `pasteText` outside
the chip extmark (`opencode/packages/tui/src/component/prompt/index.tsx:1393`)
and preserved by expansion (`:1422–1424`). This was already present on its first
wire; JSON-search failure initially obscured the separate exact-text mismatch.
Both first and second original failures remain. The final `repeat-nav-120-03`
fixture accepts that observed source-backed original spacer while retaining a
separate exact donor/native wire-difference result. It does not alter either
binary's input or waive the difference for VIS07.
