# Independent sample PNG review — prompt-paste20260927-02

Result: sample review only; VIS07_NOT_PASS. Images were opened from this fresh
campaign after capture. The full comparator scope remains unmasked.

Opened full images:

- `chip-120/upstream/leader-chip-pending.png`
- `chip-120/oc/leader-chip-pending.png`
- `chip-120/oc/leader-chip-normal.png`
- `repeat-nav-120/oc/leader-repeat-expanded.png`
- `suffix-space-120/oc/leader-suffix-repeat-two-chips.png`
- `suffix-space-120/oc/leader-repeat-enter-next.png`
- `chip-80/upstream/leader-chip-normal.png`

Observed:

1. The 120-column chip wraps onto a second row. Both pending PNGs retain the
   bold dark chip text on its orange background while the raw draft and agent
   label become muted. Normal native draft/agent colors are restored in the
   separately opened normal image. The version difference remains visible.
2. Native raw-end repeat shows each original pasted line once, expanded into
   actual text. The two native suffix-space chips are visibly distinct before
   their separate mouse expansions. These are actual PTY interactions.
3. The native suffix-space Enter-next PNG is a terminal panic, not a completed
   conversation. Its full image contains `messages.rs:600:29`, `len is 8` and
   `index is 21`. Submission/completion therefore fails independently of the
   correct pre-submit false-repeat observation.
4. Original chip-80 normal PNG contains the painted chip, but its sealed cells
   snapshot predates that paint and does not. The lock explicitly records
   `UNSTABLE_CAPTURE`. This PNG/cells timing disagreement is retained and cannot
   qualify that normal state. The raw grid comparison remains DIFFERENT (288
   cells and a cursor difference), even though this diagnostic is not a stable
   paired-state observation.

No claim of reviewing every PNG, mandatory-case qualification, or VIS07 PASS.
