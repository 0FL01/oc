# Built-in DCP prompts

These files are the native default bodies for the five DCP prompt slots. An
admitted `dcp-prompts/overrides/<name>` file (with `experimental.customPrompts`)
still replaces the matching default.

Source: Opencode-DCP `@tarquinen/opencode-dcp` 3.2.0, revision
`d637981555a18c3992472268a0657a948925d5fa`, `lib/prompts/`:

| Asset | Source file | Source SHA-256 |
|---|---|---|
| `system.md` | `system.ts` (`systemPrompt("xml")`) | `2a79517e9607867281d658992152d17c1a4625120d04860b3d20297c4ea5ae36` |
| `compress-range.md` | `compress-range.ts` (`rangePrompt("xml")`) | `502c23810c67ebc955802a8ef797d6e9f23901f156451a37345d4fd3327aeb9e` |
| `context-limit-nudge.md` | `context-limit-nudge.ts` | `d8256e5194aa66d35a983520be3d3e9b1acc14baa841ea9153f0e49d9c72798c` |
| `turn-nudge.md` | `turn-nudge.ts` | `ea0f60cc80053880378f85be36c38a543664579b0bd20eb3f5d8ca5f5e6f96a5` |
| `iteration-nudge.md` | `iteration-nudge.ts` | `4923265082886153973b3d269ac32918847e4c7d9827de2cf3c4a5a5a5ea0a8c` |

Rendering: template literals were expanded for the XML ID format and escaped
backticks unescaped. One native adaptation: `compress-range.md` names the
native compressed-block header `[compressed bN]` instead of
`[Compressed conversation section]`. Native running-task step and renewal rules
are appended by the runtime, not edited into these bodies.

Copyright (c) Opencode-DCP contributors. Licensed under the GNU Affero General
Public License v3.0 or later; the complete license text is retained at
`evidence/tui/recovery-v00/dcp-oracle20260928-native01/sources/LICENSE`.
