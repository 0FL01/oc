# T53/R3 GO03 — protocol enum and chronological system/effort seam (first atomic)

Not a GO03 PASS: Chat/Messages adapters, Go metadata lanes and compatibility
facts are later slices. This atomic delivers the shared lowering seam and the
Responses consumer that T45/R10 (spec line 138) is waiting for.

## Pinned donor (OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`)

- `packages/ai/AGENTS.md` "Chronological System Updates" / "Effort Updates".
- `packages/ai/src/protocols/shared.ts::wrapSystemUpdate` (escaped
  `<system-update>` user-text fallback).
- `packages/ai/src/protocols/open-responses.ts::lowerMessages` (system →
  `developer`, effort markers → coalesced `configuration_update`, default
  `medium`).
- `packages/ai/src/effort-updates.ts` (`resolveEffortUpdates`: drift strips
  markers and uses the current effort; otherwise top-level effort freezes at the
  first marker's `previous`).
- Native difference (spec): donor enables effort updates by model-name regex
  (`gpt-6-astra`, Claude Opus 4.8); native enables them only through a declared
  capability, never a guessed allowlist.

## Native behavior

- `provider/protocol.rs`: private finite `Protocol {Responses, Chat, Messages}`;
  `lower_chronological_system` (Responses `developer`; Chat and undeclared
  Messages escaped `<system-update>` user text in place; declared Messages keeps
  its native update); `resolve_effort_updates` and `lower_responses_effort`
  (unsupported → strip + current effort; supported → frozen top-level effort and
  in-place coalesced `configuration_update`).
- `provider.rs::stream_input_overlaid` lowers every Responses request lane
  (main, tool follow-up, title, summary compaction, child, retry): chronological
  `System` history items (plan enter/leave reminders and other runtime system
  rows) were previously sent as raw mid-conversation `role:"system"`; they now go
  as `developer` in the same position. The initial prompt was already
  `developer`.
- The effort-marker path has no production capability yet (no declared
  `supportsEffortUpdates` source until compatibility facts land, slice 3a), so
  Responses keeps the captured current top-level effort; the functions are
  marked as awaiting that consumer.

## Checks

- `provider::protocol::tests::go03_chronological_system_lowers_per_protocol_in_place`
- `provider::protocol::tests::go03_effort_markers_follow_donor_resolution`
  (drift, unsupported strip, frozen previous, coalescing, model default /
  `medium`, trailing marker).
- `provider::tests::aud09_aud10_canonical_wire_and_output`: captured HTTP body
  carries `developer` at the system item's position and no raw `system` role.
- Full workspace **1570 / 0 / 10**; fmt and strict workspace Clippy PASS.
