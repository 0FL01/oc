# T44 selected-model discovery diagnosis checkpoint

## Result

Base `6e6f70e`. The owner's exported provider key disproved the prior missing-export diagnosis. The selected ID has no statically configured entry, so startup requires dynamic catalog discovery. Existing TUI collapsed all catalog/selection failures into **Configuration load failed**; source now carries typed value-free status: rejected (401/403), other HTTP, network/timeout, invalid/empty response, pre-network invalid config, cancellation, or successful catalog lacking the selected model. 401/403 is classified from HTTP status before reading any error body. No fallback model is fabricated, no failed refresh replaces local models, and successful refresh still removes retired IDs. Actual binary PTY with fake catalog exercises categories, no Responses POST and restored terminal. The owner's key's catalog outcome remains unknown; it was not used.

## Checks

- Parent `cargo fmt --all -- --check && cargo test --locked -p oc --test recovery_startup -- --test-threads=1 && cargo test --locked -p oc-adapters discovery::tests --lib -- --test-threads=1 && cargo build --locked --release -p oc && git diff --check`: exit 0 (3 startup integration, 19 adapter discovery unit, fresh release build). Actual real-profile agent-environment release with the default data root reached Home/exit 0, no prompt, restored terminal; with unrelated provider/remote environment variables unset and only selected provider credentials inherited, isolated-data real-profile release also reached Home/exit 0.
- Parent `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0, five pre-existing ignored tests. Test-first failures, reviewer finding/correction and typed fake HTTP/PTY evidence in `discovery-report.md`.

## Risks

The owner uses a different private key and may have different config-root env overrides. Its catalog status cannot be inferred from our successful profile; no owner credential, raw headers or response were inspected. The fix is actionable safe diagnosis, **not** proof the owner's specific remote model is accessible. No bypass of selected-model validation; full T44 parity and live gates still open.

## Next

On the owner's fresh release in the same zsh environment, the new static startup category determines the next necessary correction: catalog authorization/service, network, invalid response, or selected ID absent. If it remains generic Configuration load failed, inspect effective config roots and stage-specific typed errors without sharing secrets. Do not make a model turn before establishing a valid catalog. Remaining T44 safety/resources and VIS qualification are separate.
