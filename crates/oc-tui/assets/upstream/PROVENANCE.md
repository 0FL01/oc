# Vendored upstream assets

Files here are copied byte-for-byte from upstream OpenCode. They are
upstream-derived, not authored in this repository. Keep them byte-faithful
when updating and re-record the hash below.

## `v2/opencode.json`

- Source URL: <https://raw.githubusercontent.com/anomalyco/opencode/v2.0.12/packages/tui/src/theme/assets/v2/opencode.json>
- Repository: <https://github.com/anomalyco/opencode>
- Tag: `v2.0.12` (tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`)
- Fetched: 2026-09-22
- SHA-256: `f25bc5ae9187524f1997cccadeb9f724b2fac2fe7cc4a250bdadb24311e6b367`
- Upstream license: MIT (`LICENSE` at the tag); this copy stays under the
  upstream MIT terms.
- Consumers: `crates/oc-tui/src/theme.rs` (`include_str!`) and its palette
  parity test.
- Facts used from this asset: default theme name `opencode`, default mode
  `dark`, 103-slot token model plus hue scales and the `@dialog` surface
  (see `evidence/tui/upstream-inventory.md` §2).
