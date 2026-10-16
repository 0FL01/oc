# T56 — exact migration expectations include the terminal owner

Date: 2026-10-06. Production source remains reviewed/pushed `5ce0b9baf`.
This slice changes only two existing test expectations and their owner comment.
T56 is not complete until the full final workspace gates pass.

## Observation and required correction

The fresh rebuilt release fixture with deliberately ignored/masked parent signals
passed. The subsequent full workspace run reached the adapter unit target and
failed exactly these two tests:

- `storage::tests::child_schema_migration_is_idempotent_across_reopen`
- `storage::tests::child_schema_upgrades_legacy_database_to_same_schema`

Both compared actual `[1,3,4,5,6,7,8,10,11,12]` with an old expected list ending at
11. The additive, minimal terminal identity/selection owner requires migration
12 (`t56-terminals`); migration 11 remains the separate T53 credential owner.
The actual schema was correct, rather than a migration collision or destructive
session upgrade. The failed log is retained at
`/home/opencode/.cache/opencode-tmp/opencode/t56-final-current.log`.

The tests now require the exact full version vector including 12. They still
require unchanged session columns/index, the migration-3 sentinel surviving reopen,
preserved legacy session facts and equality of upgraded/fresh session schemas.
No assertion was removed, converted to a subset check or ignored. No production
schema, migration, baseline, dependency, recovery or API changed in this slice.

## Checks

- `cargo test --locked -p oc-adapters --lib child_schema_`: **3 passed / 0 failed**;
  includes both corrected tests and the existing child-schema rollback scenario.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`, `git diff --check`, progress journal check: exit 0.
- Fresh current-production release TERM01 remains PASS; only test expectations
  changed afterward, so its production artifact/evidence is not invalidated.

No parallel Cargo, stack/timeout override, disabled test, live API or credential
access. `.opencode/` remains unread/unstaged. Next: rerun the failed full workspace
and final gates, then perform the frozen R1–R4 closure; T44 remains PAUSED.
