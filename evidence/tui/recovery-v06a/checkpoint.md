# T44 V06a Markdown parser and indexed viewport checkpoint

## Result

Base `98e81eedea4963bc16aae1d944210f9da65c18c1`, active T44. Native pinned `pulldown-cmark 0.13.0` replaces handwritten Markdown parsing; actual 2-column/11-row upstream table uses matching styled grid in bounded paired regions. Long completed text, tables, lists and fenced code are scrollable through indexed pages with bounded completed-block cache and visible-only page decode; streaming partial input is bounded. Independent reviews found and drove regression fixes for swallowed footer/tail, one-cell wide glyphs, OSC in reasoning, source-page structure/cache thrash, oversized-grapheme nonprogress, long/table/blockquote paging and escaped-pipe label continuation. All reproduced attempts are retained in `report.md`; no product-owned transcript was added.

## Checks

- Parent `cargo test --locked -p oc-tui v06a_ -- --test-threads=1` exit 0 (29 focused tests). Parent `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0 throughout; five existing ignored tests.
- Latest actual original/Rust paired `paired-80x24-escaped-label` and `paired-120x40-escaped-label` normal-executable captures: provider contracts executed, source/profile/fixture lock recorded; capture runners exit 1 because post-capture whole-frame comparator reports DIFFERENT (80: 1054/1920 styled cells, 5502/258816 pixels; 120: 1655/4800 styled cells, 5473/647040 pixels). Diagnostic table rectangles agree 490/490 and 2688/2688 styled cells respectively; this is not VIS13/VIS14 or R4 full parity.

## Risks

Full-frame and additional paired Unicode/long-cell/streaming/after-restart comparisons remain open. Source indexing covers tested GFM structures, not arbitrary nested CommonMark, and exceptionally oversized indivisible graphemes may be preview-limited; full response stays in durable history. No measured RSS/PSS/long soak for new cache yet. V06b reasoning, tool/diff status and replay remain; VIS01–VIS24, R3/R4 and product READY unverified. User's real-profile release-startup cause is still unknown; no private configuration accessed.

## Next

V06b: compare actual live reasoning/tool/patch partial and durable replay after restart with pinned reference, keep operation IDs/status and owner effects, then re-run relevant gates and a new checkpoint. After V06b, V07 security/resource qualification and V08–V09 full-frame VIS/comparator/independent review.
