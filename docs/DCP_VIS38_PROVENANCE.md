# VIS38 DCP presentation provenance

Source: [OpenCode Dynamic Context Pruning](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning),
version 3.1.15, pinned commit `11f6517780a502512a3467645074be447cb0369e`.
License: **AGPL-3.0-or-later**. Preserve this attribution and the
[original GNU Affero General Public License](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/LICENSE)
when redistributing the source-derived presentation. License-document SHA-256:
`d11d906d7e29af95d28b051cd89a064bb25ae27a5c407c9994626c47e13bdc8f`.

The native translation in `crates/oc-adapters/src/storage_dcp_view.rs`
implements the canonical categorical message-position mapping from
`lib/ui/utils.ts` (`formatProgressBar`, D06). It samples the final owner of
each of 50 cells using SQL positions instead of transferring the archive.
The original category precedence and empty fallback are preserved.

The run/block identity distinction and inherited-coverage accounting are
source-derived from `lib/compress/state.ts` (D08–D10); active summary usage is
separate from lifetime removed content. Native gross removal is measured
against actual content changes, includes effective hide/purge decisions,
and does not remove inherited summaries twice. No tokenizer is installed:
the exposed method is `utf16_round_quarter_fallback`, equivalent to
JavaScript `Math.round(text.length / 4)`, separately for each text, tool input
and tool output. Serialized provider envelopes are not token content.

Native persistence uses existing SQLite operations and ContextVersion row
tracking. Frozen operation snapshots contain no summary body or covered-ID
archive. Real saved summaries are read with operation-scoped, bounded UTF-8
byte paging (8 KiB maximum). Legacy metadata stays unavailable; no replay or
fabricated backfill is performed. Display controls are independently typed.
