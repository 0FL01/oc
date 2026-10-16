# Минимальное архитектурное исправление

## Что сохраняем

Cargo workspace из `oc-core`, `oc-adapters`, `oc-tui`, `oc`. Tokio, Ratatui/Crossterm, SQLite, reqwest и rmcp. Существующие полезные parser/storage/discovery/tests компоненты. Не нужен новый framework.

## Одна рабочая цепочка

```text
oc CLI / oc TUI
    ↓ typed application commands, queries, bounded events
один session/turn owner
    ↓ immutable effective config + context projection + permission decision
Responses transport ↔ structured provider items
    ↓ admitted complete tool calls
ordered tool dispatcher
    ↓ durable intent → executor/MCP/DCP → durable outcome
SQLite + bounded blobs / generation-owned resources
```

`oc` собирает конкретные implementations. `oc-core` задаёт application types и инварианты. Adapter может реализовывать interface, но TUI не должен создавать параллельный provider/storage/config путь. Не держать одновременно mock session history и вторую canonical history в реальном Runtime. UI snapshots — ограниченные read views, а не владельцы DB migrations/turn commits.

## Разделить данные по назначению

Raw transcript — долговечная неизменяемая история. Provider input — typed, admissible projection текущего контекста. Provider continuation — opaque items с точными ID и нужными scope, не UI text. Tool output — полные допустимые bytes в storage с quota; UI preview отдельно, с явными references. DCP — план замены частей projection, не тайная перезапись transcript и не filesystem permission rule.

## Владение и остановка

Turn владеет generation snapshot и streaming operation. Location/config generation владеет MCP clients. Application shutdown отменяет turn, закрывает owned resources и дожидается их в bound. Освобождение future и выход leader PID не доказывают cleanup descendant tree. Single-flight guard должен восстанавливаться и при dropped future/error.

Для provider/model/agent/skills/commands/permissions одна immutable snapshot publication между turns. Reload не должен дать новый provider config со старым skills body и произвольными mixed policies. Нужна простая согласованная generation, не универсальная DI/actor платформа.

## Маленькие abstractions, которые здесь оправданы

Typed ProviderInput/ProviderEvent/ToolCall с отдельными item_id/call_id; typed tool outcome вместо определения failed по строковому префиксу; узкий application handle для frontend; один стандартный URL/header representation; безопасный file-operation helper с tests. Остальные abstractions вводить лишь когда конкретное исправление без них дублирует опасную логику.

## Чего не обещаем

Exactly-once внешних side effects, all-files patch transaction, OS sandbox от собственного Unix uid, безусловный task abort для blocking operations, измеренную memory safety из одного RSS snapshot или вечную совместимость arbitrary plugin. Эти ограничения не оправдывают silent success: unknown/partial/unsupported должны быть видимыми.
