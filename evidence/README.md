# Fixtures, provenance и сохранённые proofs

Здесь остаются используемые тестами inputs, paired captures/seals, source licenses/notices и live campaign ledgers.
Это не task journal: обычный bugfix не требует нового report, FINAL или closeout. История удалённой отчётности — в Git.

Не удалять дерево целиком: Rust `include_str!` использует `T55/sparse-completed.fixture.json` и compaction `.cells.json` под `tui/`.
Source attribution и sealed proofs остаются на исходных путях; live identities/uncertain reservations не сбрасывать и не переносить между campaigns.
Новые diagnostics — bounded gitignored per-run storage по [TEST_PLAN](../docs/TEST_PLAN.md); credentials/raw live payloads не коммитить.
