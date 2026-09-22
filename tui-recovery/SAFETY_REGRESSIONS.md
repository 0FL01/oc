# Негативные регрессии рядом с T44

Это направленная проверка, не полный security audit. Воспроизведение выполнять только
во временных directories, isolated HOME/XDG, fake endpoints и собственных subprocesses.
Внешний reviewer здесь не запускал Rust. Ни один сценарий ниже заранее не отмечен PASS.

## S01 — UI не отменяет ещё не принятую submission

Статически прослежено: tui_cmd ждёт handle_key → handle_enter ждёт submit → runtime
ждёт MCP initialize до accepted (P08/P09/P11). Fake держит initialize 5 секунд. Отправить
prompt через PTY, затем resize и Esc раньше 5 секунд. UI должен обработать их независимо
от сетевого ожидания, оставить draft и не создать completed/duplicate turn. Проверить
owned process/task cleanup. Прямой вызов `app.cancel()` из теста вместо клавиши не закрывает case.

## S02 — discovered local root escape и source admission

Подготовить temporary `project/` и `outside/`. Создать `project/.opencode` symlink на
outside, положить туда минимальный valid config с `{file:local-secret-fixture}` и fake
provider. Текущий P13 canonicalizes сам root: external dir может стать trusted root,
потому что потом config проверяется только относительно уже внешнего canonical_root.

Ожидание: discovered `.opencode` не расширяет Location boundary. До admission нельзя
читать contents/file substitution, подключаться к provider/MCP или запускать command.
Не открывать FIFO/device вместо config и не читать без byte/resource policy. В source
code `read_to_string` находится раньше final source check — это тоже проверить.

Отдельные положительные cases: явно разрешённый global config dir вне project допустим;
обычный nested local regular file работает; комментарии/JSONC сохраняются. Policy
для symlink внутри разрешённого root должна быть явной, не blanket workaround.
Внутренний безопасный openat для `{file:}` не спасает ошибочный выбор trusted root.

## S03 — modifiers и key event kinds

P07 содержит `contains(CONTROL | ALT)` вместо проверки пересечения; это конкретная
ошибка selection mask. Зафиксировать raw Ctrl+P не превращается в `p`, Alt+character
не вводится как обычный character без договорённости, Shift+Enter даёт newline,
KeyEventKind::Release не дублирует ввод/submit. Capture request count и draft.
Esc закрывает modal, а не приложение; Ctrl+C в modal/editor следует выбранному контексту.

## S04 — terminal control injection (проверка риска, exploit не утверждается)

Fake model и fake tool возвращают строку с CSI erase, OSC52 clipboard, OSC8 hyperlink,
BEL и carriage return. Renderer должен показывать/безопасно экранировать данные,
а не исполнять произвольные terminal instructions из них. Проверять capture VT stream
на неожиданные управляющие последовательности относительно доверенного renderer.
Не использовать реальные clipboard secrets или чувствительные URL. Unicode/обычные
переводы строк при этом должны сохраниться. Ratatui types сами по себе не доказательство
корректного sanitization на всех путях.

## S05 — MCP error не становится policy bypass

Три fixtures: disabled entry (0 spawn/probe), required unavailable (явный failure),
working configured entry (нормальная работа). Важно: не считать required optional
из-за дизайна TUI. Проверить bearer/no-bearer и protocol shape по реальной capability
семье, не переносить codex-specific требования на неизвестный `crw` вслепую.
Диагностика содержит server id+stage+safe error class, но не header values/raw body.
Повтор после исправления — новое явное действие, не скрытый replay side effects.

## S06 — поздние события, replay и Location

Во время PendingSubmit/Streaming смена Location/session либо отклоняется видимо,
либо выполняется после корректной отмены. Late accepted/delta/tool event со старым
request/turn/generation id не очищает draft и не появляется в новом transcript.
A→B→A: globals сохранены, local metadata/clients заменены, raw sessions Location-bound.
После restart tool card/partial outcome/unknown доступен в той же semantic presentation.

## S07 — memory / complexity regressions

Wide Markdown table, >20 visible transcript rows, длинная multiline insertion, неполный
streaming fence, большой tool result, частый resize/scroll/dialog search. Измерять
реальные transient allocations/CPU/frame time и retained bytes. Сравнить equal active
view при малом/большом архиве. Ограничение 20 видимых строк — не исправление memory leak.
Нельзя каждый frame копировать всё durable history или создавать cache всех посещённых сессий.
Нельзя «убрать лимиты» как отказ от safety budgeting: разрешить нормальные user размеры
через bounded serving, pagination и explicit overflow.

## S08 — не сломать прежние security/runtime contracts

Повторить patch no-follow/preimage/partial commits, durable-intent-before-effect,
store faults/recovery, provider incomplete/EOF/cancel, child process kill/reap, secret
redaction, parent/child permissions и subagent cancellation. Новые T43/T45 paths имеют
собственный незавершённый scope; TUI card не доказывает этот scope. Scope не расширять
новым orchestration framework. Ask не превращается в allow; отсутствие approval channel
должно быть ясно, либо реализовано отдельным согласованным контрактом.

## Как фиксировать результат

Писать конкретно: «FAIL воспроизведён на SHA X командой Y», «PASS после SHA Z» или
«не выполнено: причина». Список тестов/статическая гипотеза не равны запуску. Source-derived
defect и runtime root cause конкретного пользовательского MCP могут быть разными.
