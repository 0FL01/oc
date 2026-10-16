# T65 — actual0.2.0 publication receipt

## Result

**All four0.2.0 crates are published.** Verified source/checkpoint commit
`feeea77fc` was normally pushed before the single actual publication command.
No release/tag or default-user installation was created. Configuration gates
are green; isolated installed-binary verification is still running, so R8/T65
completion is not claimed yet.

## Checks

`cargo publish --workspace --no-verify --locked`: PASS. Prior latest dry-run
verification built every extracted package against staged registry dependencies;
production source, manifests, tests/assets and packaged README were unchanged
between verification and upload; Cargo's VCS metadata now names the clean checkpoint.
Cargo uploaded, waited for availability and reported Published in dependency order:
core → adapters → TUI → oc2. Log: `.local/evidence/t65-publish-actual-01.log`.

Independent public crates.io version API reads returned200 for every package,
each version0.2.0, unyanked and published_by0FL01, rust_version1.93:

| Crate | Published UTC,2026-10-16 | Public archive bytes | License | Binaries |
| --- | --- | ---: | --- | --- |
| oc-core | 00:49:24.525624 | 72543 | MIT | none |
| oc-adapters | 00:49:27.871314 | 1477829 | MIT AND AGPL-3.0-or-later | none |
| oc-tui | 00:49:31.732725 | 8057839 | MIT AND AGPL-3.0-or-later | none |
| oc2 | 00:49:35.316828 | 457551 | MIT AND AGPL-3.0-or-later | oc2 only |

Fresh approved scratch work/HOME/install roots were created without replacing
an existing installation. Registry-only installation is running from outside the
checkout with `cargo install oc2 --version 0.2.0 --locked --registry crates-io`,
explicit isolated `--root` and the existing compilation cache reused solely as
`--target-dir`. No path/git package selection; environment excludes provider
credentials. Log: `.local/evidence/t65-registry-install-01.log`.

## Risks

Publication is an observed completed external effect: **do not repeat it**.
No credentials were printed; existing Cargo authentication was used unchanged.
The old locked yanked-dependency warning remained visible. This is not full
product, built-in-agent or TUI parity. T44's stash remains intact and unapplied.

## Next

Resolved by [the installed-registry closeout](closeout.md): registry install and
actual installed ELF checks pass; T65 is finished, T44 is active and its stash was
restored once. The following publication-time next action is retained as history.

Wait for the managed registry-install completion; inspect its actual result,
then run the bounded installed-ELF check in a fresh HOME/project/data root and
empty PATH. Require actual version/help/smoke, a local synthetic headless turn
with canonical V2 cache configuration, and persisted-session listing with no
new generation. Record the receipt, finish T65 and resume T44/restore its parked
WIP once after inspecting Git. No further upload is needed.
