# T46/R4 bounded envelope prerequisite

## Result

Frozen atomic scope at base `e739cfbf387e8b869aa4920cebb7150010f2712b`:
test-only owned HTTP interposition; explicit, durable campaign identity; reserve
and fsync before every upstream dispatch; at most 24 generation attempts and
4 short MCP calls, including retries/titles/restarts/concurrent workers. All
generation requests use the conservative 2048-token smoke ceiling (also below
8192 coding). No response/payload/header/credential content in accounting.
Explicit fresh initialization refuses existing state; resume never initializes.
Identity binds canonical root and journal inodes; relocation/replacement,
unreadable/corrupt/untrusted/symlinked/exhausted state fails closed.

Implemented and proved offline. Authority: `scripts/bounded_live.py`, used by the
existing `live_bounded` Cargo target through `live_bounded/envelope.rs`. No product
source/API/dependency/schema changes. Owned 0700 state root / 0600 single-link
append-only journal; inode-bound ID; every reservation serialized with `flock`
and fsynced before upstream DNS/connect/send. Interrupted or uncertain attempts
remain consumed; a summary file or native-process restart cannot replenish them.
The journal's original identity binding also rejects an edited token carrying
the same nonce/inodes at a renamed root path; IDs cannot be re-encoded to relocate
or replace the campaign authority.

Follow-up offline qualification: the existing limiter authority is preserved;
the native same-owner late-warning race is now repaired and qualified in
`late-warnings.md`. That report records the combined **1262/0/10** gates and
current final debug/release artifacts; the original prerequisite results and
artifact association below remain their historical execution record.

## Checks

All offline, no real credentials/configuration or external peers used:

- Red actual-socket experiment before interposition: assertion required 24/4;
  actual peers received **25 generation / 5 MCP** requests (observed failure).
  Same peer-count proof after interposition: **24/4**, with matching journal.
- Python suite **9/0** (`TMPDIR=... python3 -B scripts/test_bounded_live.py -v`):
  two concurrent runner/helper processes submit 32/32 attempts, actual upstream
  gets exactly **24/4**; post-reservation process exit 19 consumes one attempt,
  restarted helper forwards only 23 more generations and rejects the next.
  Existing-root initialization refuses, including exhausted state.
- Missing/empty/corrupt/partial journals, file/root permissions, symlinked root
  or journal, hardlinks, relocated/replaced identity, and injected read/write/
  fsync failures: **zero upstream requests**. Failed fsync's written reservation
  remains consumed. Unknown routes, batches, tasks/sampling, oversized bodies,
  long searches and unbounded output requests fail before external dispatch.
- Exact configured escaped path + query, body bytes, protected header values,
  MCP protocol/session headers and user-agent forwarded to owned peers; chunked
  entities decoded incrementally. Credential canary absent from journal,
  metadata and helper output. SSE first bytes arrive before upstream completes;
  oversized response rejected. Cancellation and ownership EOF reap workers;
  four held workers plus a queued fifth never oversubscribe or leave child PIDs.
- Targeted `cargo test -p oc --locked --test live_bounded envelope:: -- --nocapture`:
  **4/0**. Two actual public `oc` processes and two helper processes reuse one
  campaign: first native run dispatches **3** POSTs (real retry + automatic title
  included); after both native/helper restart, combined actual POST count and
  journal count are **4**. Guarded five-step campaign: **14 generation / 1 MCP**,
  all mandatory steps PASS. Original fake five-step branches remain PASS.
  Narrow read/search-only R4 preflight is admitted with zero new generations;
  the unchanged coding preflight rejects that insufficient coding profile.
- Affected `cargo test -p oc --locked --test live_bounded`: **13/0/3 ignored**.
- Workspace `cargo fmt --all -- --check`, strict
  `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS.
  Initial Clippy missing adjacent unsafe comment fixed; final strict run PASS.
- `cargo test --workspace --locked`: **1258 passed / 0 failed / 10 ignored**.
  Final execution log: harness output `tool_0eba6b8d7001cyplZxZ2AvsJCH`, 114040 bytes,
  outside repo evidence. No new large captures or source-hash manifests.

Cargo serial environment: `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1`,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
tool timeout 900000 ms. Preflight uid 1003, available RAM 7.0 GiB / disk 173.8 GiB.
Actual debug/release association captured before and after final gates; stable:
debug `50cfb8e3f778bcd27706f15b50c0db07fd651a2c4e9530e9d15643b9da02d525`,
release `e724540586cab3594aa2c3235247c90c3c16af9422701a9f5c673f52fac7b84d`.
Both `--help` PASS. Debug is the already media-recorded test-profile ELF
(`media.md:22`), distinct from the supplied final-build fingerprint; release
matches the supplied fingerprint. No production-source edits or extra release
rebuild. Native fixture uses Cargo's actual `CARGO_BIN_EXE_oc`.

## Risks

Live NOT_RUN. This is a bounded test campaign authority, not a product lifetime
quota. Native fixture routes explicitly through owned loopback endpoints;
configured upstream targets/protected headers remain in RAM. This prerequisite
does not qualify R4, T46, T44/V09, or READY.

Bounded support surface: HTTP/1.1 transfer framing; exact supplied HTTPS target
path/query, body and end-to-end headers (no redirects, probing, endpoint/model
fallback, proxy-env routing or URL normalization). Live dial addresses must be
public; `--offline` permits only loopback. Native base URL is deliberately the
owned loopback interposer, **not** the original upstream URL. The explicit
manifest is the forward-target source association, not a claim of bypassing the
guard. Config on disk has only loopback routes and credential placeholders.
All tools/call conservatively count toward four and require short search/query
<=256 UTF-8 bytes; unsupported RPC fails closed. Every generation request must
carry stream=true and max_output_tokens<=2048. Request <=1 MiB, campaign input
<=8 MiB, response <=8 MiB, journal <=64 KiB; four owned workers, 300s deadline.
R4 tool grants are read plus configured crw/codex only; coding harness allows
only the seeded fixture's `cargo test` shell resource. Browser/npx never enabled.
Task/batch/input-replay extensions in calls are rejected. A final stricter params
check initially rejected the pinned native rmcp-added progress token (observed
five-step MCP failure); narrowly admitting that bounded correlation metadata
unchanged restored **14/1** without changing caps/assertions/timeouts. Final
direct, native, affected-target and whole-workspace gates ran after this fix.
After all gates, own-UID helper/fixture process scan is empty (pgrep exit 1).

Native follow-up originally discovered offline: two materially different unavailable-peer
experiments advertised catalogs and completed search but first headless turn
did not print the unavailable warning. `runtime.rs` captures warnings before the
async failure completes; `headless.rs` prints the turn report's snapshot. The
new ignored `live_bounded_r4` asserts warning visibility and reports non-success
honestly. The race is now resolved/qualified by `late-warnings.md`, including
strict same-envelope native offline R4 and exactly one stderr warning. Real R4
remains NOT_RUN; this was not an external blocker or a limiter failure.

## Next

Return sole mutation/Cargo/fixture ownership to parent for review and explicit
bounded live R4. Live **NOT_RUN**; no task-state/history/delivery claims changed.

Explicit fresh campaign, only once in a new root beneath an owned private parent:

```sh
export OC_LIVE_CAMPAIGN_ID="$(python3 -B scripts/bounded_live.py init "$OC_LIVE_CAMPAIGN_ROOT")"
python3 -B scripts/bounded_live.py inspect --campaign "$OC_LIVE_CAMPAIGN_ID"
```

Preserve the returned ID and journal in owned metadata. Every rerun/restart or
concurrent runner MUST export that **same ID** and reuse the same root/inodes;
never run init again or mint another root/ID to continue this campaign. Missing,
untrusted, corrupt or exhausted authority is a stop, not zero usage.

Parent-required environment names: `OC_LIVE_CAMPAIGN_ID`, `OC_TEST_MODEL`,
`OC_LIVE_UPSTREAM_JSON`, `OC_LIVE_OPT_IN`; optional `OC_TEST_VARIANT` and
`OC_LIVE_SUMMARY`. `OC_LIVE_UPSTREAM_JSON` schema:
`{"provider":{"generation_url":...,"discovery_url":...,"headers":{...}},
"mcp":{"crw":{"url":...,"headers":{...}},"codex_web":{"url":...,"headers":{...}},
"unavailable":{"url":...,"headers":{...}}}}`.
Supply exact configured generation/discovery URLs and remote MCP URL/header
associations from owner-approved product sources in RAM only. Future parent's
approved test source names include `LUDKA2_API_URL`, `LUDKA2_API_KEY`,
`OC_TEST_MODEL`; this subtask did not read `.local/live.env` or runner auth/config.
After review and offline warning qualification, explicit parent-only command:

```sh
OC_LIVE_OPT_IN=bounded-v1 CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 \
cargo test -p oc --locked --test live_bounded live_bounded_r4 -- --ignored --exact --nocapture
```

Only explicit opt-in plus independently verified existing identity permits the
manifest read/start. Environment presence alone cannot activate a live branch;
summary overwrite cannot reset authority. R4 report requires the actual native
catalog IDs, completed codex operation and durable dispatch, warning and exit 0.
