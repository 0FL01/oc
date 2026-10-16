# Shared native syntax / current-profile user border — qualified slice

Whole frozen **R1–R6/VIS01–VIS45/T44 remains ACTIVE / NOT_PASS**. This receipt
qualifies the implemented grammar and current-profile presentation slice, not
global pixel parity or final R6/V09. Strict whole-frame differences remain visible.

## Implementation and preserved owners

- `oc-tui/src/syntax.rs` replaces the four-language lexical subset with one shared
  native Tree-sitter 0.25.10 capture/cache owner. Markdown, indexed live/replay
  fences and actual old/new patch-hunk images consume that owner. No production
  Node/Bun/WASM host, parser download, UI-specific execution or new storage/query
  owner is introduced.
- All **39** actual configured grammars are statically compiled from vetted C/C++
  sources: 34 external donor entries and the five OpenTUI built-ins (JavaScript,
  TypeScript, Markdown, Markdown inline, Zig). `build.rs` verifies source archives,
  query parts/combined queries, licenses and corpus digests before compilation.
  Complete revision/source/alias/resolver/license inventory is in
  `crates/oc-tui/assets/syntax/manifest.json` and `PROVENANCE.md`.
- One source/grammar-keyed token cache retains at most eight entries / 256 KiB.
  Untrusted parsing/query/injection work shares the existing new service's 30 ms
  budget, 64 KiB source bound and 8192-capture/match bounds. Trusted immutable query
  compilation is outside that deadline, including cold injected query setup.
  Resource and known-asset failures remain visible preview diagnostics; missing
  required assets are not silently qualified as an unknown language.
- Capture overlap/order, exact scope and first-component style fallback, injection
  containment, duplicate/empty captures and outer Markdown EOF normalization
  mirror the real pinned worker/converter. Code/diff concealment is disabled.
  Whole-fence context and revision survive indexed blank/4 KiB/128-row boundaries.
  CommonMark ASCII fence closures, malformed backtick info and tab-stop indentation
  preserve literal text. TAB stays in grammar input and paints as two styled cells;
  ordinary headings retain bold/H1 underline. No lexical keyword substitute remains.
- Patch rendering parses each bounded old/new hunk image separately, preserving
  actual effect coordinates, gutters, backgrounds, UTF-8 and truncation facts.
  It never invents a full file from omitted diff context.
- Accepted user stripes follow the **current admitted session profile** only in
  render copies, as pinned OC2 does. Stored agent/generation slots, RAW/IDs/turns,
  model/provider/tool graph, draft, selection and assistant-footer identity stay
  unchanged. Full and indexed custom/fallback profile colors are tested.

## Exact reference assets and licensing

Donor: `anomalyco/opencode` v2.0.12, commit
`2670273ff17da96f85c5826ced57aa1b368754fa`. OpenTUI 0.5.10:
`f6673a04ccb671b9207da358c57152bfd27c781f`. Floating nvim queries are frozen at
`cf12346a3414fa1b06af75c79faebe7f76df080a`. Original repository license/COPYING/
NOTICE bytes and query-source license files are retained, not replaced by a hash.

Swift's configured current `main` query refers to `unsafe_expression`, absent from
its actual 0.7.1 WASM. Both fixtures explicitly freeze the **official same-release
0.7.1 query** at `7c2f26b5dce12e82ef2bd932a883ef514ae566b8`, under the original
MAIN URL cache key (`swift-5d830544.scm`). No unsupported pattern was removed or
renamed. Native Swift C is generated from that tagged source with CLI 0.23.2/ABI14;
its ABI/node inventory is not claimed byte-identical to the original WASM.
Nix's native source and the configured ast-grep WASM are likewise independently
pinned and empirically vetted, not equated by a guessed version label.

`fixtures.reference.json` comes from the **actual** pinned OpenTUI worker,
Web Tree-sitter grammars, donor theme resolution/native SyntaxStyle and actual
chunk converter, not copied native goldens. All 39 languages / 78 cases / 156
dark-light projections compare token byte boundaries, names, injection metadata,
literal text, fg/bg and attributes. Empty captures and comment/spell overlap remain.

The running Original TUI consumes a fresh isolated ordinary URL cache, not a
replacement worker or renderer: **85 sealed inputs = 34 WASMs + 35 highlight parts
+ 16 locals**. All **69 required grammar/highlight inputs** advance their atimes
before the first inspection. The 16 locals do not advance in that first witness;
later inspection is not counted as consumption. Five packaged built-ins and the
original executable/descriptors remain unchanged. No user cache/config is touched.

## Actual paired campaigns and effects

The complete new acquisition is stored in `syntax-proof.tar.xz` with independent
receipt `syntax-proof.inventory.json`. Archive members retain the exact producer
files under `inventory/`, `ordinary/`, `temporal/`, `matched/`.

| Episode | Actual captures | Direct strict results |
| --- | ---: | --- |
| Syntax010, four stages per side | 8 | 8 DIFFERENT |
| Ordinary, 27 stages per side plus two native details | 56 | 50 DIFFERENT / 4 EQUAL; four separate native-only entries |
| Default WebGL 80×24, 72 temporal rasters per side plus regular stages | 160 | 160 DIFFERENT |

Total: **224 opaque full PNGs / 788448 styled cells / 104193792 pixels**.
Direct paired comparisons: **222 = 218 DIFFERENT + 4 EQUAL**; native-only details
are not paired PASS. All 72 matched temporal pairs add **144 DIFFERENT** reports.
Every unchanged strict report independently recomputes, without masking/cropping.

Syntax010 submits one real structured Write, streams partial syntax, completes all
39 visible grammar samples, captures the current-profile draft at 80×24, naturally
exits/relaunches and reopens the saved session through an actual painted Sessions
mouse target. Full inventory and replay use 120×320 / 1011×5120 complete rasters.
Both sides make **3 requests / 1 Write**, producing exactly **91 bytes**, SHA-256
`9095ffad076d11e160e996e792d0ddb79063091c2df94f1765f196280ac3327e`.
The native whole operation row is identical before/after; no provider/tool replay
occurs after restart. Generation-0 exits are natural code0; final generation-1
teardown is separately recorded as forced stop, not mistaken for the restart.

All 39 completed and replayed code bodies/headings match the real SDK reference:
**14256 styled source-cell comparisons, zero mismatches**, including Unicode/wide
continuations, literal Markdown blanks/fences, TAB=2, semantic backgrounds and H1
bold/underline. The accepted user border is `#12ab34` on both sides. Same-side
complete/replayed full cells, cursor and PNG bytes are identical.

The ordinary episode retains four actual calls/operations, five presentations
(the identical logical/terminal foreground-Shell facts are not a fifth call),
actual MCP/Shell effects and artifacts without replay. Default temporal retains
72 full opaque PNGs per side across six owners, **3863 samples / 23205 VT commands**,
zero cycles and zero phantom command states. Default behavior is not a blink claim.
Carets are individually preserved; native `(21,18)` and Original `(24,18)` differ.
One genuine hidden synchronized Original parser-cursor snapshot is retained, not
normalized into its visible raster caret.

## Current source/build association

All three capture roots record one successful actual `cargo build --locked` and
all **704** current producer-input hashes. Captured base HEAD is
`e1cce26cd242ece9c57a85684983fe294df6b3f8`; captured code dirty-diff SHA-256 is
`a2af528ff014ad850dbaab06a593c3ecae3d423b136d1de220c9753860b93ca3`.
Immutable capture receipts precede this factual document/checkpoint/delivery.

- Canonical producer manifest: `d73f564a24885d0a6614a5b4bead31a7a862f6cc7d065703c5e72939448e66e1`.
- Native actual ELF: `95fbc6139e644a3039be127d325c134f305489f8fb0a1b37e899dd8ba57ed51a`.
- Original actual ELF: `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
- Archive: **3631412 bytes**, SHA-256 `164778616790a31574dd3e48f93f3cd8523f7a7ca6d7614c52fb739aefa1e021`.
- TAR: **90245120 bytes**, SHA-256 `f5145700c9a32c15744da2ef5c9367d03022ded0d481e37dbe3b5de022e1bf52`.
- Whole member inventory: `438b0b546a62cdc8b17dba40c8427c58e611c0362d73b519810a7e50a0dca6e4`.

## Byte preservation and unchanged storage guard

`scripts/retain_syntax_proof.py` stores **all1406 whole files / 96245525 logical
bytes** in one stdlib USTAR/XZ stream: 1258 regular members and 148 byte-identical
same-run/same-side PNG storage aliases. Original encoded PNG bytes, cells/styles,
cursor, geometry, render/text/VT/traces, protocol, source manifest, commands and
comparison reports survive exactly. Bounds remain 16 MiB/member, 128 MiB logical,
2000 members, 32 MiB XZ decoder and 384 MiB development utility process. Independent
inventory/encoded/TAR seals and whole-source roundtrip precede durable exclusive
file+parent-directory fsync writes. Reopen audit is independent of acquisition paths.

The initial 72 KiB retained headroom did not fit measured complete delta bundles
(727732, 759424, 717500 bytes). Those failed experiments did not write final proof.
They were removed as unsuccessful untracked helper source, not promoted to a
mathematical impossibility or shipped as preflate/LGPL runtime dependencies.

Actual instructions require failed attempts to remain visible, not every own
untracked failed run to keep redundant loose storage forever. One such run,
`subagent-cards-001`, is preserved in **595732 bytes** at
`subagent-cards-001.tar.xz`, with complete103-file byte/path/mode/UID/GID/mtime metadata
in `subagent-cards-001.archived.json` and visible original-path `ARCHIVED.md` marking
**FAILED / NOT_QUALIFIED**. Archive SHA-256:
`48031bb7bbcace9a1581e3c54d0301631fa21009e02540be9cca1f07d8bf81e3`.
Only verified redundant loose copies from a private atomic transaction were removed
after durable full-byte/meta audit. No committed qualified evidence, baseline,
historical report/checkpoint or foreign data was rewritten. The **1 GiB own-evidence
cap and 16 MiB raw-log cap remain unchanged**; final allocation is checked before
delivery. Fresh acquisition scratch is removed only after independent retained
restoration audit; it is not a substitute for durable evidence.

## Actual current gates and diagnosis

- fmt/diff checks and strict locked workspace/all-target Clippy pass.
- Staged whitespace checks cover authored source/docs/JSON. Byte-exact licensed
  SCM inputs retain their original trailing whitespace (Elixir/Ruby locals);
  their asset seals/compilation are checked without trimming the donor bytes.
- Full locked workspace `--no-fail-fast`: **1816 passed / 0 failed / 11 unchanged
  opt-in ignores**, all46 results. Final run serializes test functions with
  `RUST_TEST_THREADS=1`; internally concurrent runtime/provider/family scenarios
  still run unchanged. Previous unrelated GO05/provider-burst failures and their
  unchanged targeted successes remain visible, not treated as proven CPU causes.
- Slow-consumer PTY test exposed a real fixture wait-before-drain cycle. Its helper
  now resumes the consumer before waiting, preserving the same startup/stall,
  30s timeout, terminal restoration and exact persisted-response assertions.
- Actual debug locked/ordinary and release builds plus both ELF `--help` pass.
- Release startup4 tests cover19 isolated routes / 8 GET-only discovery cases;
  TERM01 passes raw/control/VT/resize/hide/focus/shutdown/crash/no replay. This is not
  real-user strace qualification. Baseline→two-PTY RSS18184→21804 KiB, threads8→10,
  fds24→26, idle0 ticks/300ms.
- Original release pacing/archive guards pass unchanged: idle0 ticks/~1s;
  165/250Hz p95 4543/6648µs; provider-burst p95 13165/9194µs, lag0. S07 archive0→3000
  RSS26388→27340 KiB, HWM27680→28288, retained24231/window152/cache8581 unchanged.
  AUD32 pairs8→3000 HWM20824→22584 KiB, DB942080→152944640 bytes, child0.
- Python utilities51, explicit reference-cache2/cursor-receipt3/comparator25,
  frontend DOM+WebGL and full tall geometry checks pass. Complete38400-cell JSON
  transport is byte/field-equivalent to recursive transport; it avoids measured
  2.2–2.8s snapshot IPC cost without relaxing the15s/five-identical-full-frame gate,
  owned process-group pause/drain/resume ACKs or full opaque screenshot checks.

## Mandatory continuation

Whole-frame differences remain: completed Write uses a native derived diff versus
Original numbered postimage; narrow wrap/live chrome/usage/Add/caret/raster edges
and genuine elapsed times remain unmasked. This slice does not waive those or any
other frozen outcome, Thoughts/family/permission/question/DCP/MCP/auth supplements,
final current-source R6/V09, or the exact actual paired acceptance. Next: fix the
confirmed Write presentation through existing bounded recorded/result owners,
then every remaining frozen outcome and full final qualification. No task finish.
