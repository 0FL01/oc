# T12 — Native Responses stream

Status: PASS. Implementation commit: `ff2476bbe836f33bd24625797a94ec7326a219d3`. Method: offline `cargo` unit execution against a hand-rolled loopback SSE server plus parser unit vectors; no external network, no live credentials, no Docker.

## PROV01 Request — PASS

- Exact generation URL (trimmed configured prefix + `/responses`; `/v1` never doubled or stripped), `Bearer` auth, `store:false`, input message + ordinary function tool schemas, stable `prompt_cache_key` (sha256 over canonical prompt + name-sorted tools; byte-identical across calls, distinct across prompts). No OAuth, no fallback. Secrets redacted in `Debug`/diagnostics.

## PROV02 SSE/usage — PASS

- Incremental decoder: identical items for whole/1-byte/odd-chunk feeds; split multibyte UTF-8 held across pushes (plus a real discarded-items bug found via clippy and fixed); CRLF endings, `:comment` heartbeats, text + function-call-argument deltas, terminal usage metadata; event cap enforced (10 001st event errors).

## PROV06 Errors — PASS

- 401/403 typed without retry (1 request); 429→success via exactly one owned retry (2 requests); persistent 500 → `Server` after initial + one retry, never more; mid-stream abort after a committed delta → `Incomplete` with exactly 1 request (committed generation never repeated).

## PROV07 Timeouts — PASS

- `CHUNK_TIMEOUT_MS == 6_000_000` asserted; 300 ms gap vs 100 ms override → `IdleTimeout` (timeout applied to the chunk wait itself — first version only checked between polls and was fixed); `timeout:false`/absent builds a client with no total deadline; explicit cancel flag drops the connection → `Cancelled`; closed-port fails fast as `Transport` (only true timeouts map to `Deadline`); `timeout:true` rejected as invalid config.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p oc-adapters provider` 5/5; workspace 76 total (oc 4 + adapters 52 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Tool-call roundtrip (`function_call_output` → next response) and reasoning-item persistence/replay are later tasks (PROV03/04), not claimed here.
- Full DNS-pinning (connect-by-IP) remains residual as in T11; per-hop lookup + post-dial peer check apply to provider traffic too.
