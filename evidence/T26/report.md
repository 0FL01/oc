# T26 — PTY и UX qualification (UI01)

Status: PASS (fake/local PTY; 9/9 PTY tests + 1 unit test).

## UI01 PTY qualification — PASS

Харнес `crates/oc/tests/pty.rs`: реальный бинарь `oc tui` под настоящим PTY
(libc `openpty`, без новых зависимостей), вождение через master-сторону,
ассерты по реальному поведению — байты рендера, exit-коды, termios slave —
а не по снепшотам рендера.

- Smoke: набор `hi` + Enter, `you: hi` в эфире, echo-turn, `/quit`, exit 0,
  `\x1b[?1049l` в выводе, termios slave назад в cooked+echo.
- Unicode/paste: кириллица + эмодзи, Backspace стирает char (не байт),
  Db содержит `привет 🌎` точь-в-точь; паста 1500 символов чанками входит
  целиком, Db содержит все 1500 + echo.
- Resize: 80x24 → 100x30 → 60x12, рамка перерисовывается в полную ширину;
  tiny 40x8 работает без layout-паники, turn завершается, Ctrl-C выходит 0.
- Cancel: Ctrl-C из idle завершает 0 с восстановлением терминала.
- Panic: `OC_TUI_TEST_PANIC=1` (квалификационный проб, только PTY-тесты) —
  nonzero exit, `panicked` в выводе, alt-screen покинут, termios sane.
- SSH-like: `TERM=screen-256color` и unset TERM — полный цикл зелёный.
- Long history: сид 3000 сообщений стартует за секунды, хвост виден,
  5×Up подскролливают старые строки (проверено по реконструированному
  экрану), выход чистый.
- Slow consumer: мастер не читает 3+ секунды пока идёт turn и Ctrl-C —
  выход 0, echo-turn завершён и записан в Db, restore-маркеры на месте.
- Без TTY (piped stdin): `no TTY` usage-ошибка, exit 1, терминал не тронут.

## Найденные и исправленные баги продукта

1. Дренаж событий бинарника не сбрасывал `active_turn`/статус
   (`crates/oc/src/tui_cmd.rs`): после первого turn все submit вечно
   `turn busy`. Добавлены `TuiState::apply_delta/apply_finished/
   apply_interrupted` (turn-scoped, stale-события игнорируются) +
   юнит-тест; свободные `push_live/finish_live/cut_live` удалены.
2. Вьюха не следовала за новыми строками (`draw`): `Paragraph`
   top-align обрезал хвост — на 40x8 экран замирал на первых трёх строках.
   Добавлен bottom-align окна (`scroll` = len − pane_rows с учётом
   пользовательского скролла).
3. Один key за итерацию делал большую пасту минутной: bounded drain
   до 256 pending keys за кадр (`MAX_KEYS_PER_FRAME`), worker drain не
   голодает.

## Проверки

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test --workspace --locked`: всё зелёное (oc-tui +1 юнит,
  pty 9/9, остальное без изменений; ignored live — без изменений).
- `cargo build --locked` exit 0; `check_docs.py` OK.

## Scope и ограничения

- Квалификация локальная (openpty), не по сети: SSH-like = TERM-варианты
  и tiny-размеры, не реальный sshd. Явный live-SSH вне scope T26.
- Пасты >1024 байт одним write strandятся edge-triggered mio
  (апстрим-кап чтения crossterm): тесты шлют чанками с паузами, как живой
  терминал; задокументировано здесь, кода продукта не потребовало.
- Проб `OC_TUI_TEST_PANIC` — только для PTY-квалификации, в обычном
  использовании никогда не установлен.
