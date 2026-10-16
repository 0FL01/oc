# Независимые regression tests: заготовки, не выполненный результат

`audit_regressions.rs` написан по публичным APIs проверенного commit. **Составитель не компилировал и не запускал этот файл.** Его наличие не закрывает ни один AUD gate. Против исходной реализации ожидаются failures; это нужно сначала подтвердить на хосте агента.

После интеграции документации и проверки актуальных API скопировать файл в `crates/oc-adapters/tests/audit_regressions.rs` и выполнить:

```sh
cargo test -p oc-adapters --test audit_regressions --locked -- --nocapture
```

Зависимости `tempfile`, `serde_json`, `sha2` уже используются проектом; проверить их доступность в integration target текущего Cargo manifest. При API drift адаптировать imports/signatures, **не смысл assertions**. Сначала сохранить baseline failure, затем исправление, затем успешный результат. Это не proof, что полный binary path работает: для него остаются AUD01/AUD02/AUD29/AUD35.

Файл содержит 10 небольших тестов: UTF-8 HTML, Add File grammar, temporary symlink, orphan blob, header case, legacy permission, agent body snapshot, literal command Markdown и два discovery oracle edge cases. Это часть 40 acceptance specifications, не десять новых acceptance owners. Сами IDs в planning имеют ровно одного owner.

Все filesystem side effects ограничены временным деревом; «outside» — sentinel рядом с project внутри того же tempdir, не реальный домашний файл. MCP URL example.invalid используется только для чистого from_entry, HTTP не выполняется. Нет настоящих secrets.

Shell descendant/deadlock и filesystem race/crash cases намеренно НЕ превращены в потенциально зависающий обычный unit test. Их реализовать отдельными subprocess fixtures с outer watchdog, process-group cleanup и bounded logs по task specs. Не считать эти 10 tests исчерпывающими safety tests.
