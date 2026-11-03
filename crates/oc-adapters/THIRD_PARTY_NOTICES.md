# Third-party notices

Original project code is MIT; see LICENSE. This package also contains
source-derived Dynamic Context Pruning code under AGPL-3.0-or-later; see
LICENSE-AGPL-3.0-or-later and the source attribution comments.

Source: https://github.com/Opencode-DCP/opencode-dynamic-context-pruning

- DCP 3.1.15: commit `11f6517780a502512a3467645074be447cb0369e`.
- DCP 3.2.0: commit `d637981555a18c3992472268a0657a948925d5fa`.
- Native request cadence, compression/accounting and display mappings retain
  their recorded source provenance. The implementation uses native Rust/SQLite,
  not the upstream JS plugin runtime.

Full project provenance and corresponding source:
https://github.com/0FL01/oc/tree/agent/oc-rust-port/docs/DCP.md
and docs/DCP_VIS38_PROVENANCE.md in that repository.

The embedded OpenCode review prompt retains its MIT license and attribution in
assets/upstream/PROVENANCE.md. These notices do not replace original notices.

W4's native WebSocket proxy/NO_PROXY selection is source-derived from OpenCode
`packages/core/src/effect/websocket-constructor.ts`, MIT, revision
`ff90ed91607cabf420a369e9009539c1aa0932af`. The complete original MIT notice is
retained at `fixtures/OpenCode-MIT.txt`; source/test mapping is in docs/SOURCES.md.
