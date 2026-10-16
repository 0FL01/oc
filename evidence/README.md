# Evidence

Здесь хранятся короткие sanitized reports по task ID и итоговый FINAL.md. Raw stdout/stderr, live request/response bodies и credentials сюда не входят; `.local/evidence/` исключается из Git.

Минимум report: task, code commit/dirty status, метод (synthetic/source-derived/differential/live/manual), команды/exit codes, проверенные assertions, ограничения, ссылки на tests и measurement inputs. Для каждого назначенного task acceptance ID указать `PASS`, `FAIL`, `BLOCKED_EXTERNAL` или `NOT_RUN`; `NOT_RUN_DISABLED` допустим только для условного browser smoke. PASS нельзя получить одной ссылкой на план. Test-only fabricated model IDs допустимы и не попадают в production registry.

Task reports являются authoritative evidence; `FINAL.md` только связывает их с A01–A13 и не копирует полные логи.
