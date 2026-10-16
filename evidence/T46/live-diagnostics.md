# T46/R4 — additive safe dispatch receipts (offline scope)

## Coordinator review

All five diagnostic paths were independently reviewed. Additive finish fields
are typed and whitelisted; old version-1 identity/header/reservation bytes remain
valid and unchanged. No raw body, header, exception text or model reply is used
for diagnosis. Coordinator reruns passed Python **13/0**, complete `live_bounded`
**15/0/3 unchanged opt-in ignored**, native lifecycle **4/0**, fmt and strict
workspace all-target Clippy. Existing normal debug/release fingerprints below
remained identical. Production Rust was unchanged by this diagnostic extension.

The previously exposed pending-key fixture ordering was corrected narrowly:
observe a following Enter's details frame and return while catalog stays held,
then release catalog. This acknowledges all preceding pending Space keys before
their action can change to Disconnect; original spawn/effect/retirement assertions
and deadlines remain intact. Current lifecycle case passes with that real UI
acknowledgement, rather than relying on a stale Connecting paint.

Parent independently inspected the actual existing campaign: **2 generation /
0 MCP / 4 control**, same nonce and original unknown receipts. No initialization,
refund or backfill occurred. First live attempt remains NON_SUCCESS in
`live-attempt-01.md`; the next actual experiment is separately bounded and must
continue these counts. Full R4/T46/T44/V09/READY remains open.

## Result — frozen before implementation

Parent reports that its first actual R4 attempt failed: native exit 1, watchdog
false, owned process reaped, pipe I/O OK (0 stdout / 648 stderr bytes), one visible
unavailable warning, no advertised required catalogs or completed search. The
existing trusted campaign consumed **2 generation / 0 MCP / 4 control**, with
4313 input bytes. Its existing outcomes cannot distinguish HTTP status from
transport failure. Those supplied facts are not a new observation by this task;
no provider status or CRW failure cause has been established.

This atomic extension adds only bounded metadata at the existing test-only
dispatch boundary: actual HTTP status (integer 100..599) and fixed typed failure
categories. Existing journal identity/header/reservations and spent counts must
remain valid and immutable. Old outcomes without new fields remain honestly
unknown. No raw response/header/URL/error text or model text is a diagnostic.

### Implemented result

Offline receipt extension **PASS**, based on HEAD
`2d34e9b87a5408ad436eb0a181c9761ec9e8cba3`. Five changed paths: this report,
`scripts/bounded_live.py`, `scripts/test_bounded_live.py`,
`crates/oc/tests/live_bounded.rs`, `crates/oc/tests/live_bounded/envelope.rs`.
The native failure overview retains process exit/watchdog/reap/I/O byte metadata
and uses fixed `native_error_code: "unknown"` for non-success; it does not infer
provider error text from discarded stderr or model output.

Finish-row examples (schema examples, **not** parent's live receipts):

```json
{"attempt":1,"outcome":"complete","http_status":401}
{"attempt":2,"outcome":"uncertain","failure":"dns","failure_stage":"connect"}
```

`http_status` is optional, strictly integer 100..599 (not bool/string/null), read
from the actual parsed upstream HTTP response. `failure` is a fixed whitelist:
`dns`, `tls`, `timeout`, `connection`, `http_protocol`, `io`, `address_refused`,
`address_limit`, `response_limit`, `response_header_limit`, `envelope_refused`.
It is classified from Python exception types / owned fixed `Closed` codes,
never exception messages. Its required paired `failure_stage` is one of
`connect`, `request`, `response`, `relay`; failure fields require `uncertain`.
Unknown `Closed` constructor input becomes fixed `envelope_refused`, preventing
even injected exception-text canaries from being emitted as a code.

Identity/header version and binding, reservation rows and mandatory reserve /
flock / fsync-before-DNS order are unchanged. Old two-field finish rows remain
valid; absent receipt fields stay absent. Only new finishes append metadata.
Unexpected fields, invalid values/pairs/outcomes refuse before dispatch. A
local rejection before successful reservation emits fixed metadata with
`failure_stage: "pre_dispatch"` only to its local caller, without an upstream
request; a failed reservation fsync can still consume written count. Reserved
connect/address failures consume count. `complete` means HTTP relay completed,
including 400/401/500; it is not model/MCP success. Existing input/output/body /
response/journal limits, exact target/header/path/body forwarding and owned
worker cleanup are retained.

## Checks — frozen experiment

1. Owned HTTP 401/400/200/500 receipts match actual peer counts and preserve each
   consumed reservation; response/header/exception secret canaries stay absent.
2. Resume an explicitly created offline journal with two old-schema consumed
   generations; preserve its identity and prior bytes, append new receipts only.
   Malformed or unexpected receipt fields refuse before external dispatch.
3. Inject DNS/TLS/address/connection/timeout exceptions using only local fakes;
   distinguish fixed transport/address refusals from local pre-dispatch refusal.
4. Exercise safe native R4 non-success overview with an actual owned HTTP error;
   retain all nine existing Python and all fourteen native bounded scenarios.
5. Run focused tests, affected target, workspace fmt and strict locked all-target
   Clippy if Rust harness changes. Verify current native artifact association;
   production source is not part of this diagnostic change.

### Actual checks and counter sources

Preflight: UID 1003; available RAM 7.5 GB / disk 187 GB, approved TMPDIR.
Commands used `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1`, approved TMPDIR and
900000 ms tool timeout; Cargo used `CARGO_NET_OFFLINE=true`.

- Initial RED: the two new receipt tests failed in 0.470 s: actual four owned
  statuses had `[None,None,None,None]` receipts, and injected DNS had no category.
  No live data was involved; failures were preserved as this factual result.
- `python3 -B scripts/test_bounded_live.py -v`: **13 passed / 0 failed** (5.790 s),
  including all original nine. An old-format owned journal with two consumed
  reservations resumed without init/reset: four actual upstream POSTs returned
  401/400/200/500, one each; counts progressed **2 → 6**, original byte prefix /
  identity unchanged, old receipt fields absent. CLI inspect matched the complete
  snapshot JSON, with no clipping or secret canaries.
- Invalid/new receipt fields produced no additional actual peer effects. Typed
  injected DNS/TLS/connection/timeout/I/O/protocol errors and actual private-address
  check produced fixed metadata; local output refusal consumed no new attempt.
  Response/header/raw-exception canaries were absent from journal/metadata/output.
  Finish write/fsync failures dispatched exactly **2 actual requests / 2 consumed**,
  never refunded/re-dispatched; failed write left `reserved`.
- Existing concurrent two-runner proof still observed exactly **24 generation /
  4 MCP actual upstream calls**. Crash-resume, identity/trust/corruption/permissions,
  exact escaped paths/protected headers/payload/protocol, streaming/response caps,
  cancellation and four-held-worker ownership-EOF reaping all passed.
- Focused new native HTTP-400 gate: **1 passed / 0 failed**. Actual public binary
  produced **2 upstream generation POSTs / 2 receipts at 400 / 0 MCP calls**;
  strict report was non-success, with native error code honestly unknown and
  canaries/credential/target URL absent.
- `cargo test -p oc --locked --test live_bounded -- --nocapture`: **15 passed /
  0 failed / 3 unchanged ignored**, including six envelope cases (21.50 s).
  Negative incomplete-campaign cases deliberately catch their printed panics.
- `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`: PASS (1.26 s). This is scoped test/harness verification;
  parent's full **1262/0/10** workspace result remains a prior result.

Cargo selected the test-profile debug ELF during test compilation. A cached
`cargo build --locked` restored the normal debug artifact; no release rebuild was
needed. Both public binary `--help` commands passed. After the last root build,
direct compiled targets (no subsequent root Cargo) verified:

```sh
target/debug/deps/live_bounded-508a2d8c4345c739 envelope:: --test-threads=1 --nocapture
target/debug/deps/mcp_application-c554f016ee604ae9 lifecycle:: --test-threads=1 --nocapture
```

- Final envelope **6/0**: strict offline R4 **5 generation / 1 search / 1 warning**;
  native HTTP-400 failure **2/0** with actual statuses; five-step **14 generation /
  1 MCP**; two native / two helper processes sharing one ID **3 → 4** generations,
  with one actual 500 retry receipt and automatic-title attempts counted.
- Final unchanged lifecycle target **4/0** (5.51 s). Earlier supplementary run
  observed **3/1**; details and the materially different probe are below.
- Full normal-binary bounded target also passed **15/0/3** before that diagnostic
  probe (21.38 s). Artifact hashes were identical before/after final direct gates:
  debug `0b263d57efd089bdc7ff4cde23afd810a5ba01b3b5b3a806d2e89dfd589fd56f`;
  release `b0c394701711ebb5efe37f6afcd03cdf12bb9461f2da71b8cb5dc881417479b3`.

## Risks

An interrupted worker may leave only a consumed reservation; no receipt means
unknown, never no effect. Receipt persistence failure cannot refund or authorize
another attempt. Receipt status describes the configured upstream HTTP response,
not successful model execution or MCP negotiation. Parent's existing live journal
is neither read nor rewritten by this offline task.

Supplementary unchanged PTY case
`mcp08_actual_retry_and_reload_location_retire_pending_scope_without_late_effects`
once showed `repair/Disabled` instead of expected `Connected`, after sending three
pending-space keys and releasing its file-held catalog. Source evidence:
`PtyProcess::raw` only writes/flushes; the existing Connecting-frame check can
return without acknowledging queued key consumption. Once Connected,
`TuiState::mcp_toggle` legitimately selects Disconnect. A temporary diagnostic
experiment awaited a newly entered safe-details frame and return to pending
footer **before** catalog release, acknowledging the key burst: **1/0**, 1.22 s.
That temporary test-only instrumentation was removed, original source rebuilt,
and the unchanged four-case target rechecked once: **4/0**. No assertion, timeout,
baseline or product control was relaxed. The initial failure remains recorded;
the unacknowledged PTY input ordering merits separate fixture follow-up if it
recurs, rather than a receipt or external-service diagnosis.

## Next

Offline proof is complete; mutation / Cargo / fixture ownership returns to parent
for review. Parent's next authorized actual
experiment must reuse its **same campaign ID, root and journal**, continuing the
already spent 2/0 counts. Live is **NOT_RERUN** here; full R4/T46/READY remain open.

Parent review / next separately authorized experiment: reuse existing
`OC_LIVE_CAMPAIGN_ID`, exact `OC_TEST_MODEL`, explicit RAM-only
`OC_LIVE_UPSTREAM_JSON`; optional `OC_TEST_VARIANT`, `OC_LIVE_SUMMARY`. Inspect the
existing identity independently (no init/new ID), then invoke the same strict gate:

```sh
python3 -B scripts/bounded_live.py inspect --campaign "$OC_LIVE_CAMPAIGN_ID"
OC_LIVE_OPT_IN=bounded-v1 CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 cargo test -p oc --locked --test live_bounded live_bounded_r4 -- --ignored --exact --nocapture
```

No claim of provider 401, CRW filtering, external blocker or live PASS follows
from the old outcomes. New actual receipts must establish the next diagnosis.
