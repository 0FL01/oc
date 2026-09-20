# Evidence

Здесь хранятся короткие sanitized reports по task ID и итоговый FINAL.md. Raw stdout/stderr, live request/response bodies и credentials сюда не входят; `.local/evidence/` исключается из Git.

Минимум report: task, code commit/dirty status, метод (synthetic/source-derived/differential/live/manual), команды/exit codes, проверенные assertions, ограничения, ссылки на tests и measurement inputs. PASS нельзя получить одной ссылкой на план. Test-only fabricated model IDs допустимы и не попадают в production registry.

`PACKAGE_VALIDATION.md` относится только к документации/утилитам этого архива, не к будущему Rust-продукту. Все A01–A13 сначала NOT_RUN.
