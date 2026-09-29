# T46/R4 — qualified owner-live native run

## Result

**PASS** for the required T46/R4 live boundary, not full T44/V09/product READY.
The public native `oc run` completed a real turn with connected CRW and
`codex_web` catalogs, one completed short codex search, and exactly one visible
warning from the explicitly declared unavailable fixture. No raw live response,
prompt, header, endpoint, credential or full environment is retained here.

Source is implementation `1ce4cda09ff16c770895b48b555e1f6cbb669a8c`
(production Rust unchanged by that diagnostic commit). Normal binaries:

- debug `0b263d57efd089bdc7ff4cde23afd810a5ba01b3b5b3a806d2e89dfd589fd56f`
- release `b0c394701711ebb5efe37f6afcd03cdf12bb9461f2da71b8cb5dc881417479b3`

## Checks

Executed existing compiled test target:

```text
target/debug/deps/live_bounded-508a2d8c4345c739 live_bounded_r4
  --ignored --exact --test-threads=1 --nocapture
```

Explicit opt-in was `bounded-v1`; upstream manifest stayed RAM-only, source
association came from the approved product inputs and exact recorded CRW POST
association. The native fixture used a declared isolated provider namespace:
the **entire exact published product model input** remained the wire ID.
Only the test selector acquired a namespace. No provider/model fallback,
runtime ID rewriting, authoring model change or user-config editing occurred.
Directed actual discovery proved the full ID present among 43 published models,
while its incorrectly truncated portion was absent. CRW's actual known legacy
protocol was 2025-06-18; codex retains the native exact 2025-11-25 contract.

### Actual observations

| Required observation | Result / independent source |
| --- | --- |
| Native completed turn | exit 0, watchdog false, reaped true, pipe I/O OK |
| CRW initialized and catalog available | initialize 200, initialized notification 202, tools/list 200; actual provider tool schemas advertise `crw` |
| codex initialized and catalog available | initialize 200, initialized notification 202, tools/list 200; actual provider tool schemas advertise `codex_web` |
| Actual short search completed | one counted codex tools/call, HTTP 200 and native durable ToolOp completion; response text alone cannot satisfy the assertion |
| Unavailable server remains visible without abort | one native headless warning; negative fixture address refused before dial, tools never advertised |
| Both catalogs remain available after result | two actual provider requests advertise both server catalogs, including the request after the completed search |
| Whole gate | unchanged strict R4 assertions: 1 passed / 0 failed, exit 0, 25.46 s |

Native output was discarded after byte accounting (682 stdout / 583 stderr).
No OCR, LLM-keyword inference, successful prose substitution or skipped assertion
establishes these observations; catalog identities come from the actual wire
schemas and search completion from operation state plus the counted RPC.

### One continuing trusted campaign

Nonce `ceb2142492354cfc8d479bf6bfa4e864`, same root/inodes/identity/journal for all
attempts and diagnostics. The successful attempt continued **4/0/12** to:

- **8 total generation** (including failed attempts, retries/title policy)
- **1 total MCP tool/search call**
- **19 total control requests**, 74,733 aggregate input bytes

Successful attempt used four actual generation POSTs, one short search and seven
controls. All four generation receipts were HTTP 200. The search receipt was 200;
actual provider schemas included both server identities before and after it.
Reservations 20/21 are first main/title exchanges, 26/28 carry both catalogs,
27 is the codex tool call. Earlier HTTP-400 attempts and status-less/unfinished
reservations remain consumed and unchanged. Upper bounds **24 generation / 4
short searches**, output <=2048 and all original input/worker/SSRF/time caps hold.
Remaining allowance is **16 generation / 3 searches** for subsequent authorized
qualification under this campaign; do not mint another ID to replenish it.

### Fresh final offline qualification

Serial actual-host workspace Rust gate passed **1263 / 0 failed / 10 opt-in
ignored** (old nine plus the explicit R4 runner). Current MCP binary target
41/0, adapter runtime 94/0, TUI 417/0, adapters lib 333/0, core 26/0.
Fmt, strict locked workspace/all-target Clippy, locked debug/release workspace
builds and both public `--help` commands passed. Log:
`tool_0ec27401a001OAxiyXPsdjReKY` under the runner's tool-output directory.

After that Rust gate, Python alone exposed a receipt-observation race: a local
Content-Length error can be fully read before the worker's finally/fsync receipt.
The existing streaming test now closes its response and observes the receipt
within its existing three-second fixture budget. It still **requires uncertain**
and additionally exact `response_limit` / `response`; it does not accept reserved,
weaken a dispatch bound, or change the limiter. This final Python-only repair
does not invalidate the preceding unchanged Rust gate.

Coordinator reruns: Python envelope **13/0**, code-size **5/0**, progress **15/0**,
docs **14/0**, documentation/progress/diff checks PASS. Final normal-binary direct
proofs: envelope **6/0**, actual MCP lifecycle **4/0**; artifact hashes before/after
were identical. Advisory: 263 owned files, no >5000-line warning. No further live
requests were made by these offline proofs or the final ledger inspection.

## Risks

This is the permitted owner's isolated live fixture, not their normal workspace
or authoring agent. The unavailable target is an explicit safe negative fixture,
not an assertion that the real CRW/codex service is down. Interposition is explicit
and exact-target/header preserving, not unguarded direct access. The first two
failed selector attempts are preserved in `live-attempt-01.md` and
`live-attempt-02-diagnosis.md`. No charge/request refund, silent model switch,
legacy receipt backfill or budget-reset claim is made. Browser remains disabled;
OAuth, CodeMode and unsupported modern protocol remain declared boundaries.

## Next

Close the assigned T46 R1–R7 plus media/prompts/resources outcomes with their
scoped reports and current full gate. Then continue T51/T45/T50 and remaining
T44/V09/product gates; R4 PASS does not close those distinct owners or A09/FINAL.
