# T65 — verified0.2.0 pre-publication checkpoint

## Result

Configuration R1–R6 is qualified. All four packages and six internal requirements
are0.2.0; only four local lock versions changed. This checkpoint is committed
before any actual upload. Public lookups still show all four0.2.0 versions absent.
Implementation commits:2df915e0e,0d773d72e,ea49f2cfe,c5e9c3765,1b48f77fa.

## Checks

Pinned TS/shared79 PASS; adapters1086/0/5existingignored, binary333/0/7existingignored,
fresh normal two-thread workspace1998/0/12existingignored across46 targets PASS.
Fmt, strict workspace/all-target Clippy, debug/release build and actual release
help/version/smoke with empty PATH PASS. Latest extracted four-package workspace
publish dry run PASS, all uploads explicitly aborted; licenses/assets retained.
Full evidence: evidence/T65/report.md and its linked candidate history/logs.

## Risks

R8 actual publication and isolated registry install are still pending; R7 journal
return follows them. No full product/TUI READY claim. Existing locked yanked
dependency warning is unchanged. Use only Cargo's existing credential mechanism;
never print credentials or repeat an unknown publication effect after interruption.
T44 WIP remains stash1087067b3025322885c6a964579d80cea4931b06, not applied.

## Next

After committing/pushing this checkpoint, run the already-verified workspace's
ordinary `cargo publish --workspace --no-verify --locked` once. Reconcile public
versions if interrupted; publish only confirmed missing packages in dependency
order. Verify public0.2.0 metadata and registry-only isolated `cargo install oc2
--version 0.2.0 --locked`, then actual installed help/version/smoke/headless turn
and session persistence. Record receipts, finish T65, start T44 and restore the
parked WIP once after checking Git. No release/tag or user-config replacement.
