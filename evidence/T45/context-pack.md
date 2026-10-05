# T45/R8 — context_message_ids quoted parent context (first atomic)

Native extension (R8); omission keeps OC2 fresh-context delegation. Not an R8
closeout.

## Behavior

- `subagent` schema gains optional `context_message_ids` (≤64 non-empty
  strings), validated by `tools.rs::validate_subagent_args`.
- `storage_conversation.rs::parent_context_messages` resolves IDs in one read
  transaction, before any child row/reservation: only the calling session's
  active-branch (`conversation_messages`/`conversation_turns`) canonical user
  messages (an accepted turn message) and committed assistant answers, at or
  before the invoking turn's accepted message. Unknown, foreign-session,
  reverted/excluded, notice/reminder and not-yet-visible rows are refused with
  an actionable tool error; nothing is created. Repeats deduplicate and output
  follows parent chronology, not argument order. Stored text is used as-is
  (a command keeps its invocation text, not the expanded prompt).
- `runtime/turn.rs::TurnSubagent::context_pack` renders a provenance-labelled
  `<parent_context>` block of `<message id role>` entries with `& < >`
  escaped, so quoted text cannot forge runtime delimiters, and prepends it to
  the delegated task in the child's user message (never system/developer or
  assistant/tool items). Fresh and continued children both receive it once
  with their new task. The pack is part of the accepted child user message and
  turn log, so first delivery, background recovery and restart replay the same
  snapshot; later parent Revert/compaction cannot change it.
- The pack is bounded to 256 KiB before admission.
- Pre-admission model-context cost (second atomic): when a pack is present,
  `TurnSubagent::admit_quoted_child` runs the same schema-inclusive
  `run_turn_admitted(..., probe=true)` as native command children against the
  resolved child model, child lane fixed profile/environment/AGENTS/MCP/tool
  inputs, retained continuation history and the output/safety reserve, before
  `Jobs::launch` writes any child row/job/input. Refusal is an actionable tool
  error ("does not fit … Select fewer messages or summarize the findings").

## Remaining R8 work (pending)

- Bounded visible ID index that omits notices/system rows (the existing
  "Stable text-message IDs" rendering is reused unchanged).
- Explicit selection of DCP/compaction-covered raw messages is admitted by the
  resolver, but no dedicated test yet; guidance/preview wording for automatic
  profile/AGENTS/tools assembly; attachment diagnostics.

## Checks

- `tests/subagent.rs::r8_context_message_ids_quote_exact_parent_messages_in_chronology`
  (fake provider: unknown and foreign IDs refused without a child; selected
  user+assistant quoted escaped, chronological, deduplicated; unselected
  current-turn text absent; durable child task contains the pack).
- fmt check, strict workspace all-target Clippy: PASS.
- Full workspace test (3 jobs / 2 threads, bench TMPDIR): **1557 passed /
  0 failed / 10 ignored**.
- `tests/subagent.rs::r8_oversized_quoted_context_is_refused_before_child_creation`
  (tiny-context child model: quoted 36 KB message refused with no child;
  the same agent without a pack runs). RED confirmed by disabling the probe.
  Full workspace after this atomic: **1558 / 0 / 10**.
