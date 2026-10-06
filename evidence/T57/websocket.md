# T57 — native Responses channel owner (partial AUTH04)

Date: 2026-10-06. Implementation base: `6ea620032`.
This is checked native local WS/HTTP protocol and ownership evidence, **not** a
real OpenAI/Codex login/request, rebuilt CLI/TUI or complete AUTH04/AUTH06 claim.
Frozen outcomes and the restricted-error amendment remain unchanged.

## Source and dependency

Pinned OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`, source paths:
`packages/ai/src/protocols/{openai-responses,open-responses-channel,open-responses-continuation}.ts`
and `packages/core/src/session/model-transport.ts`. Key and OAuth Responses default
to WS; explicit HTTP remains HTTP. The channel sends `response.create`, strips
stream/stream_options/background, uses the source beta header, rotates after 55
minutes and publishes continuation only after complete decoded consumption.

Pinned `tokio-tungstenite = 0.28.0` (MIT), `tungstenite = 0.28.0` (MIT OR
Apache-2.0), handshake and native-OS-root rustls features, no connect/DNS helper.
The existing futures-util dependency enables sink. Cargo.lock records 14 additional
packages; existing TLS/native-root dependencies are reused. No JS/SDK host,
provider model probe, new store, generic channel service or generation retry loop.

## Owned transport and capture

`provider/websocket.rs` owns a shared root pool with at most eight session slots,
serialized exchanges, exact final HTTP URL/header/account/endpoint affinity and
joined bounded shutdown. `Db::shared_handle` shares this owner; per-request native
preparation attaches it only to the selected OpenAI leaf. Application exit joins
auth/children and closes channels even on worker failure. Config admits typed
http/websocket only; foreign/generic WS remains unready before turn acceptance.

DNS resolves the original admitted HTTP authority once and validates every answer;
TCP dials those addresses and verifies the peer before TLS/credential handshake.
Only then is ws/wss derived. No proxy, redirect, private-peer or alternate-route
bypass. Incoming frame/message cap is the existing 2 MiB SSE bound, request/write
ceiling the existing 32 MiB request cap, and the existing event/generation limits
remain. Pool lock/connect/dispatch/send/read share the numeric total deadline;
idle and cancellation checks remain independent.

The physical dispatch callback runs before each actual generation send. Successful
upgrade is factual HTTP 101, not invented retry-header evidence. Unknown/accepted
channel send/read/partial failures cannot trigger HTTP fallback or runtime replay.
Only affirmative connect/not-sent/returned-original-write-buffer/early close-1009
rejection may pin the session to HTTP with the **same** final URL, credentials,
model and original full body. Affinity change clears that pin.

## Decode, continuation and errors

Strict created-response identity precedes response events, with duplicate/mismatched
IDs refused. Pretty multiline JSON is canonically re-encoded for the same bounded
SSE structural decoder, not parsed with line/keyword heuristics. Its existing
completed output/argument/ciphertext checks and collector are shared with HTTP.

Append requires a strictly longer input, identical canonical prefix (including
prior output) and unchanged request invariants. Only bounded hashes/response ID
are retained, not a second transcript/output archive. Earlier done reasoning
ciphertext survives completion re-encryption. Compaction triggers, cancellation,
incomplete/failed decoding and queued unsolicited data withhold the checkpoint.
Provider continuation/connection-limit rejection clears the socket and permits
one same-operation/full-body recovery through the **existing finite runtime retry
owner**; repeated identical rejection is terminal, never an inner send loop.

The common failure owner classifies cyber_policy and the exact bounded structured
provider-message fallback, not ordinary user/LLM prose or joined fields. Known
category diagnostics are bounded/redacted before observation/Debug/storage; only
the exact admitted Daybreak public URI survives. Unknown/incomplete raw prose
retains the older withheld privacy contract. Policy followed by close remains
policy, not EOF/fallback. Handshake headers never become WS attempt-local override.

## Current checks

With approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2, serial Cargo and normal
stacks:

- `cargo test --locked -p oc-adapters --lib auth04_`: **18 passed / 0 failed**,
  including ten new owning channel scenarios in `provider/websocket/tests.rs`.
- `cargo test --locked -p oc-adapters --lib`: **662 passed / 0 failed / 1 ignored**
  (the unchanged explicit Go opt-in), 663 total, 104.09 s.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`, `git diff --check`: exit 0.
- Final local chain: `/home/opencode/.cache/opencode-tmp/opencode/t57-ws-slice-final.log`,
  terminal marker `T57_WS_FINAL_SLICE_GATES_PASS` after every command exited zero.

Real local TCP/WS peers capture both auth kinds, native identity headers, append/
token-affinity/55-minute rotation, connect/1009 fallback and HTTP pin, ambiguous/
partial/policy/identity failures with no HTTP, recovery bound, cancellation,
canonical checkpoint proof, unsolicited tail, pending-dispatch deadline and typed
foreign admission. They are synthetic private fixtures, not real provider proof.

Material corrections: fixture completion now emits actual consistent text delta
and authoritative done output; pretty WS JSON needed canonical SSE framing; strict
Clippy collapsed one condition. First full gate failed two unchanged privacy tests
because unknown raw messages became visible: the decoder now withholds those
unadmitted categories, retaining classified safe details. Both original tests and
the full suite pass unchanged. Explicit HTTP-only profile-header fixture declares
HTTP; separate default-channel tests prove WS. No assertion disabling, threshold/
timeout/stack increase, validation weakening, baseline rewrite or live call.

Next: full captured runtime/actual-binary lanes, shared CLI/TUI auth consumers and
AUTH06 dedicated owner-operated browser/device authorization plus ordinary OpenAI
test key. T44 owns VIS45 and stays PAUSED. No finish or backend-only READY claim.
