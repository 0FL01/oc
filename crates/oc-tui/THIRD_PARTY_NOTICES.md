# Third-party notices

Original project code is MIT; see LICENSE. The DCP presentation in src/dcp_view.rs
retains source-derived text/layout from Dynamic Context Pruning 3.1.15,
commit `11f6517780a502512a3467645074be447cb0369e`, lib/ui/notification.ts and
lib/ui/utils.ts, under AGPL-3.0-or-later. See LICENSE-AGPL-3.0-or-later.

Source: https://github.com/Opencode-DCP/opencode-dynamic-context-pruning
Project provenance: https://github.com/0FL01/oc/blob/agent/oc-rust-port/docs/DCP_VIS38_PROVENANCE.md

Additional bundled source/data retains its original licenses and attribution:

- Theme: assets/upstream/PROVENANCE.md.
- Fuzzysort port: assets/fuzzysort-LICENSE and src/fuzzy.rs.
- All 39 syntax grammars and queries: assets/syntax/PROVENANCE.md,
  manifest.json, license-* files and license/COPYING files inside source archives.

These notices do not replace original notices.

The MCP sidebar presentation follows the behavior of OpenCode 2.0.31's
`packages/tui/src/feature-plugins/sidebar/mcp.tsx` and session sidebar at supplied
commit `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6` (MIT, Copyright (c) 2025 opencode).
The MIT permission notice in LICENSE also applies to that upstream copyright.
No upstream TypeScript runtime or plugin host is bundled.
