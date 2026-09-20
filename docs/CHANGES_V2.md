# Изменения v2 после ответов владельца

Незакрытый Q01–Q07 questionnaire заменён конкретным daily-direct goal. Executor теперь Luna, не Sol. Product provider — native Responses через пользовательский proxy; no OAuth/provider zoo. Discovery переносится с присланного user script, а не более старого repository plugin; известные имена моделей остаются только пользовательским static примером, не Rust registry.

DCP 3.1.15 pinned с AGPL provenance. Обязательны range compress, prompts/nudges, protections, dedup/purge и восстановление projection. Unified apply_patch применяется для всех моделей и учитывается DCP protected paths. MCP квалифицируется на фактическом POST/JSON/2025-11-25 contract OpenProxy; Chrome остаётся выключен.

Убраны обязательные Code Mode/serve/attach/migration/release задачи. Остались TUI/headless, storage, tools, provider, DCP/MCP, offline/live qualification. Scope не называется full OpenCode parity.

Граф: 31 задача / 7 этапов. Приёмка: A01–A12, 74 спецификации; missing keys блокируют live задачи, не весь код. Final report task может завершиться честным partial handoff; это не завершение product goal READY.

Долгий progress log заменён STATE/NOW → phase index → task index → immutable leaf. Python stdlib utility делает короткие атомарные записи, обнаруживает orphan и stale index. Никакого summarizer/RAG/framework. В пакет включены собственные regression tests utility и offline JS reference tests.

Исправлены внутренние нюансы: text substitution до JSONC parsing; отсутствие env у неиспользуемого provider не мешает selected credential validation; Cargo target остаётся worktree/target, только временные файлы перемещаются из tmpfs в .local/tmp. Multi-file patch не обещает общей атомарности; cancellation не обещает rollback. YOLO не объявлен sandbox.
