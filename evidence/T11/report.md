# T11 — Webfetch

Status: PASS. Implementation commit: `32362e1381800c1ddaa89332aec096b4f396d3b1`. Method: offline `cargo` unit execution against a hand-rolled loopback HTTP server plus IP-classification units; no external network, no live requests, no Docker.

## TOOL07 Web text — PASS

- New `oc-adapters/webfetch.rs`: plain `GET` with metadata-first results (`status`, `content-type`, original + final URL) and bounded readable extraction — text as-is, JSON pretty-printed when parseable, HTML reduced (comments/script/style dropped, tags stripped, entities decoded, whitespace collapsed).
- Manual redirect following (max 5): relative `Location` resolved against the current URL; final URL reported; non-2xx statuses returned as data.

## TOOL08 Web safety — PASS

- Dial-time SSRF guards: DNS pre-check on the initial URL and every redirect hop (`lookup_host`, all addresses verified); post-dial `remote_addr` verification against rebinding flips; `no_proxy` + `Policy::none` so reqwest performs no hidden hops.
- `ip_is_public` refuses private/loopback/link-local/multicast/unspecified/documentation/`100.64/10` v4, loopback/multicast/`fe80::/10`/`fc00::/7`/documentation v6, and unwraps `::ffff:`-mapped v4 first (mapped-public stays public). Literal `10.0.0.1` and `localhost` refused pre-dial; redirect to `10.0.0.1` refused mid-chain; loopback dials require the explicit test-only `allow_loopback` exception (production default `false`).
- 3 MiB body truncates at exactly the 1 MiB cap with flag; 5 s server vs 300 ms deadline reports `Deadline`; bearer reaches the first hop only (server-observed: `/login` has it, `/check` does not).

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0 (fixed `collapsible_if`, `redundant_guards`, `bool_assert_comparison`, unstable `is_documentation` replaced with manual `2001:db8::/32`).
- `cargo test -p oc-adapters webfetch` 6/6; workspace 71 total (oc 4 + adapters 47 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Full DNS-pinning (connect-by-IP with Host/SNI override) is not implemented; the pre-dial lookup runs per hop immediately before connect and the post-dial peer check closes the common rebinding flip. Live rebinding against a hostile resolver remains documented residual risk.
- Registry wiring into the model `webfetch` tool arrives with the tool executor; T11 proves fetch/extract/guard semantics.
