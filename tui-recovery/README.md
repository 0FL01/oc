# OpenCode Rust — TUI parity recovery kit

Аудит снимка `d232baa481ed6e4fc844359855d5f419f88b8a8e` ветки `agent/oc-rust-port`.
UI-эталон, выбранный текущим T44: OpenCode `v2.0.12`, commit
`2670273ff17da96f85c5826ced57aa1b368754fa`.

## Начало работы

Скопировать эту папку в корень рабочего checkout под именем `tui-recovery/`.
Не заменять `progress/STATE.json`, не импортировать старые audit fragments повторно,
не откатывать код на SHA аудита. Затем передать агенту `AGENT_PROMPT.txt`.

Порядок чтения: `RECON.md` → `T44_CONTRACT_AMENDMENT.md` →
`IMPLEMENTATION_GUIDE.md` → `VERIFICATION.md`. Детальные негативные проверки:
`SAFETY_REGRESSIONS.md`. Машиночитаемый список: `ACCEPTANCE.json`.

Документы — обязательное уточнение по текущему пользовательскому запросу, а не
реализованный patch приложения. При расхождении с новым HEAD сначала проверить
finding на новом SHA. Существующие T44/T43/T45 и progress.py сохраняются;
внутренние этапы V00–V09 не создают второй task engine.

## Что действительно сделано в этой проверке

Прочитаны актуальные исходники через GitHub, ключевые upstream-файлы на зафиксированном
commit, текущие goals/evidence/state и четыре пользовательских изображения.
Проверены PNG dimensions/colors/hashes. Кодовая проверка — статическая: в среде
аудитора нет cargo/rustc и прямого сетевого доступа контейнера для checkout/build.
Поэтому Rust, original TUI, PTY, реальные provider/MCP и регрессии порта здесь НЕ запускались.

Утилита `scripts/compare_frames.py` сравнивает уже полученные кадры, не снимает их,
не запускает приложение и не подтверждает честность provenance самостоятельно.
Её собственные Python-тесты выполнены отдельно; результаты в `VALIDATION.json`.
Не путать их с успешными тестами OpenCode Rust.

## Быстрая проверка вспомогательной утилиты

```sh
python3 -m unittest discover -s tui-recovery/scripts -p 'test_*.py' -v
```

Для PNG-режима нужен Pillow в отдельном dev/test Python environment.
Grid-режим использует только стандартную библиотеку Python 3.11+.

## Артефакты этапа

На каждый закрываемый slice: SHA, точные команды, exit codes, все попытки, метаданные
capture, upstream frame, Rust frame и diff. `references/*.png` — пожелания пользователя,
не автоматически пригодные goldens. Не писать «pixel perfect», пока не получены
парные воспроизводимые кадры. Нет запуска upstream — статус `BLOCKED_REFERENCE`,
а не разрешение заменить эталон собственными expected-массивами.
