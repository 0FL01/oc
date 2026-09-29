# T46/R4 second attempt and directed control diagnosis

## Result

Second strict R4 native run remains **NON_SUCCESS**. Same campaign nonce
`ceb2142492354cfc8d479bf6bfa4e864`; no reset/new root/refund. Counts progressed
from 2/0/4 to **4 generation / 0 MCP / 8 control**. Both new generation receipts
were actual HTTP **400**; codex initialize was HTTP **200**. Declared negative
fixture was refused at connect before dial. Native process exited 1, reaped,
watchdog false, pipe I/O OK, one unavailable warning; contract assertion exited
101 unchanged. Early shutdown left CRW startup and codex notification reserved,
which remains consumed, not a claim of remote failure or no effect.

## Checks

Two directed pairs of non-generation controls, still through the same guard,
then brought control count to **12**, leaving generation/search at **4/0**:

- Exact configured provider discovery: actual HTTP 200, JSON OpenAI `data` array,
  43 entries. The entire literal product `OC_TEST_MODEL` is an exact published
  ID; the portion after its first slash is **not** published. No catalog body,
  other model IDs, endpoint, credential or raw headers are published here.
- Explicit CRW initialize diagnostic: actual HTTP 200, typed JSON-RPC result,
  tools capability and known legacy protocol **2025-06-18**. This is a control
  observation, not native R4 catalog/search proof. No protocol rewrite/upgrade.

The second pair disambiguated full-literal versus selector membership and the
actual whitelisted protocol version; it was not another unchanged generation
retry. All four diagnostic exchanges consumed control reservations. Current
aggregate input bytes: 8928. Original unknown receipts remain unchanged.

## Risks

The operator had treated the vendor/model slash in the product model input as
the public harness's provider/model selector separator. The harness therefore
sent a truncated, unpublished wire ID; this is an operator-fixture association
defect, not authorization to add a model fallback or rename runtime IDs. HTTP
400 by itself is not proof of credential rejection or an external blocker.
CRW's older legacy protocol is within the native contract, not a modern-version
capability waiver. No completed MCP call/search has occurred.

## Next

Use an explicit isolated test provider namespace for the public fixture selector,
while preserving the **entire exact published product model input** as the wire
ID. Target URL/auth, source facts, output caps and production selection stay
unchanged; no authoring model/configuration is switched. This is a declared
test-selector mapping, not silent product fallback. Continue the same campaign
from **4/0/12** and require the unchanged actual catalogs/search/warning gates.
