## Result

Implemented demand-driven scheduling with bounded input/worker bursts and active
animation deadlines; no idle redraw loop. Default wheel preserves three rows/tick
and bounded temporal presentation; optional MacOS acceleration not claimed. Fixed
real detached-completion jump by semantic durable message/part/row anchoring,
retained older pages and expansion, preserving sticky bottom and explicit reset.

## Checks

Final serial workspace fmt/locked tests/strict all-target Clippy/locked build,
capture syntax/frontend/docs/progress/diff PASS: tool_0df7a262c0017rL2kcrdyCQPZi.
Actual source-built high-refresh-20260926-05 confirms detached/sticky completion,
Shell/list routing, 32/32 glyph paints per input rate and zero lagged worker events.
Four idle windows: zero bytes/CPU ticks/main-thread context switches. Native input
p95 38.307/35.761ms at requested165/250Hz; draw p95 24.441ms. These are PTY timing,
not measured FPS. Failed earlier attempts remain immutable.

## Risks

44 full comparisons DIFFERENT, three active PNGs unstable. Exact external idle
window/scheduler alignment, all-thread wakeups and large-history paired paging
remain unqualified. No VIS31/VIS32/V09 complete claim. .opencode/ untouched.

## Next

Deliver verified scheduling/anchor slice, then session compaction 4bee144 without
removing DCP; preserve newer owner patch/permission/leader-pending requirements.
