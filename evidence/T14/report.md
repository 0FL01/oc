# T14 — Native discovery user plugin

Status: PASS. Implementation commit: `471cf9276c31726079242dc1820ab331b98283f3`. Method: offline `cargo` unit execution with fake clock + scripted HTTP client; no external network, no live credentials, no Docker, no JS execution.

## DISC01 Basic dynamic — PASS

- Any model id (slashes preserved) needs no registry: pretty names derived (`GPT-4o Mini`, `GLM 4`), source suffix applied once, 500k context/input clamp, output untouched; request URL is trimmed-prefix + `/models`, Bearer replaces configured headers, custom headers survive.

## DISC02 Metadata — PASS

- Safe-integer/null-object/boolean/modality validation per row; one invalid row fails the whole refresh terminally with the configured catalog byte-identical.

## DISC03 Authority — PASS

- Remote `npm`/`options`/`headers` never enter the merged config; connection credentials untouched by construction; reqwest client built with `redirect: error`.

## DISC04 Merge — PASS

- Shallow spread (local wins scalars) + shallow `limit`/`variants` merges; explicit local name/limit/variant wins; remote variant allowlist fills absent standard names with `disabled`.

## DISC05 Limits — PASS

- Context/input clamped to 500000, output never clamped; limit without both positive context+output deleted, never fabricated; static `ludka` entries live outside this provider map and are untouched.

## DISC06 Variants — PASS

- Covered with DISC04 vectors (`none.disabled`, custom `high`, remote `medium`, local explicit name wins).

## DISC07 Timing — PASS

- Constants asserted (15000/30000/250-750-1500); fake clock proves 4 attempts with `[250, 750, 1500]` sleeps; latency-consuming attempts clip delays against the remaining budget and exit on exhaustion; cancellation stops the loop with zero further requests.

## DISC08 Retry classes — PASS

- 408/425/429/5xx + network + unparseable JSON + empty lists retry; other statuses, bad envelopes and invalid rows are terminal; 503×4 burns exactly 4 requests (single retry owner, no layered multiplication).

## DISC09 Publication — PASS

- Empty-then-success publishes; persistent empty keeps old; success replaces the whole map (retired ids removed); duplicate ids last-win; row/body caps terminal.

## DISC10 Isolation — PASS

- Disabled/missing selection never constructs a fetch (`should_run`); generations are independent maps; failures return the local snapshot clone; 401 warning is exactly `HTTP 401` with no body/URL/key leakage; credential-bearing URLs rejected pre-network with zero requests.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p oc-adapters discovery` 10/10; workspace 95 total (oc 4 + adapters 71 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Refresh scheduling/persistence into `provider_catalog` arrives with the runtime; T14 proves fetch/validate/merge/publish semantics against the user reference.
- JS oracle (`scripts/test_discovery_reference.mjs`, 15 tests) still validates the reference itself and is untouched.
