# Fixtures, не product capabilities

`transcript.md` — вручную подготовленный воспроизводимый сценарий по видимому тексту
первого original screenshot. Это не byte-for-byte экспорт его скрытой исходной Markdown
разметки и не доказательство фактически зарегистрированных tools. Его нужно подать
обоим executable и снять новую пару кадров. Названия execute/write/edit здесь допустимы
как обычный текст модели, не требование добавить их в runtime.

`model-catalog.json` — offline fake metadata для визуальной проверки. Имена из screenshot
не подтверждают существование публичных моделей и не становятся production constants.
`Free` в fixture имеет явно нулевую цену, unknown-price case не должен рисовать Free.
`scenarios.json` задаёт состояния, но не результаты выполненных тестов.

Для paired capture объединить config/provider stub/state/key sequence в канонический
fixture bundle и записать его SHA256. Изменение fixture после reference capture требует
новой обеих сторон capture. Не менять только reference или только expected Rust.
