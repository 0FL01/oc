## Result

T27 (bounded live campaign) is started and partially unblocked; it is NOT finished and
NOT passing. Two real harness gaps were found and one was fixed:

1. FIXED: `crates/oc/tests/live_bounded.rs` parsed the owner's config with strict
   `serde_json::from_str`, so the real JSONC user config failed with
   `is not JSON: expected ',' or ']' at line 7 column 1`. It now uses the shipped
   `oc_adapters::config::parse_jsonc`, exactly like the product.
2. OPEN: after that fix the harness reads the user config, then blocks with
   `model ocg/muse-spark-1.3-contributor is not declared in
   /home/opencode/.config/opencode/opencode.jsonc`. `.local/live.env` sets
   `OC_TEST_MODEL=ocg/muse-spark-1.3-contributor` (OpenProxy upstream id), but the
   product provider is `ludka2` and that provider declares NO models in the user config:
   its catalog comes from native `/models` discovery (docs/PROVIDER_OPENPROXY.md §discovery,
   verified deployment models `ocg/muse-spark-1.3-contributor`, `cx/gpt-5.6-luna`).
   `provider.ludka2.models = []` is a verified structural fact.

Credentials are present in this environment (LUDKA2_API_URL/KEY set, `mcp.codex_web`
enabled with credentials); nothing was printed from `.local/live.env` and no secret
value appears in the logs.

## Checks

`evidence/T27/live-campaign.log`: first run exit 101, blocked on JSON parse; second run
exit 101, blocked on the undeclared-model check. Harness JSON: `attempted 0 / passed 0 /
blocked 1`. `cargo test -p oc --test live_bounded --no-run` after the JSONC fix: compiles.
No live provider request was made (0 attempts), so no paid call happened.

## Risks

- The live gate is still unmet: no T27 PASS exists, so READY is impossible and T30 must
  not claim it.
- The harness must accept a model resolved through the product's discovery path (or the
  product must be given an explicit declared model), without weakening the mandatory
  `codex_web` MCP step.

## Next

1. Extend `LiveConfig` so `OC_TEST_MODEL` may name a model that the configured provider
   resolves via native discovery: accept `ludka2/ocg/muse-spark-1.3-contributor`, and
   obtain limits from the product's own catalog (e.g. a bounded probe run or the
   discovery snapshot) instead of requiring a `models` entry in the user config.
2. Re-run: `set -a; . .local/live.env; set +a; OC_TEST_MODEL=ludka2/ocg/muse-spark-1.3-contributor
   cargo test --locked -p oc --test live_bounded live_bounded_campaign -- --ignored --nocapture`
   and require the mandatory `codex_web` step to pass (no optional skip).
3. Record the live summary in `evidence/T27/`; only then run T30 FINAL over A01–A13.
