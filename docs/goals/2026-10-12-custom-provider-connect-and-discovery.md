# Goal: custom provider через /connect с OC2 auth flow и discovery

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete — T59 R1–R5 / CONN01–CONN05 verified on code `42c026053`.
Source: запрос владельца 2026-10-12: название/base URL/API key через `/connect`,
стандартные Responses/Chat/Messages, автоматический каталог с обязательными modalities
и effort, запуск без providers/models; «нам надо повторить его паттерн авторизации»;
после read-only RECON — «Вноси в план правок работ и коммит пуш на текущей ветке».
Last updated: 2026-10-14
Task: T59 / M8; detailed acceptance CONN01–CONN05 только T59.

## Objective

Пользователь запускает native `oc` без подключённых провайдеров и моделей, открывает
обычный OC2-like `/connect`, создаёт custom connection по названию, literal API base
URL и ключу либо явно выбирает anonymous. Read-only discovery получает модели и
реально объявленные capabilities/efforts; после owner acknowledgement открывается
provider-filtered picker, без автоматического выбора модели. Connection и scoped
credential переживают restart, следующие requests используют выбранный native wire.

## Frozen Contract

### Required Outcomes

- R1: создание и durable storage custom connection через одного native owner.
  - Source: пользователь вводит название, base URL и API key в `/connect`;
    согласованный план хранения и обязательные T53 endpoint/credential boundaries.
  - Acceptance: typed create сохраняет connection metadata и начальный Key account
    атомарно в existing SQLite; explicit anonymous не создаёт dummy credential.
    Config precedence/provider filters, scoped local/LAN admission, restart и Location
    switch сохраняются; cancelled probe/form не создаёт connection или account.
  - Primary evidence: CONN01 existing storage/composition owner tests, reopen и
    actual-binary restart из CONN04; rollback, config collision и foreign-key negatives.
  - Status: verified.
- R2: bounded read-only обнаружение каталога и честный выбор native API.
  - Source: «сам фетчит и ловит какие там апи эндпоинты есть»; выбранное владельцем
    «Подтвердить тип API» при неоднозначности, без платной генерации для detection.
  - Acceptance: GET `<captured-base-prefix>/models` получает OpenAI-compatible или
    Anthropic catalog; Responses/Chat/Messages — только три существующих wires.
    OpenAI list не доказывает Responses versus Chat: запросить явный выбор, не
    угадывать по provider/model name, 404/OPTIONS или скрытому generation fallback.
    Anthropic shape — evidence семейства, не универсальная гарантия generation route.
    Failed/partial refresh не публикует частичный catalog; valid custom empty list
    действительно очищает remote set, не меняя legacy OpenProxy empty semantics.
  - Primary evidence: CONN02 fake GET transport/parser/pagination/clock/security;
    CONN04 endpoint counters подтверждают ноль generation POST до явного submit.
  - Status: verified.
- R3: modalities и reasoning effort доходят до picker без выдуманных capabilities.
  - Source: «модальности и effort тоже берёт — это ... нужно обязательно учесть»;
    выбранное владельцем «Показывать неизвестное» для ID-only catalogs.
  - Acceptance: ID-only models остаются видимыми/пригодными для явного выбора;
    missing/null, explicit false и declared empty различимы. Все валидные relevant
    `opencode` metadata импортируются при любом custom provider ID. Anthropic partial
    capabilities не превращаются в fabricated input/output arrays; selectable efforts
    только явно supported, exact selected effort доходит до существующего wire.
  - Primary evidence: CONN03 parser → core snapshot → TUI reconstruction/picker
    fixtures плюс captured effort на трёх native wires; legacy DISC/VAR01 regressions.
  - Status: verified.
- R4: знакомый OC2 connect/accounts/auth → models и рабочий zero-model startup.
  - Source: «нам надо повторить его паттерн авторизации ... не шугать ... форка»;
    требование запустить приложение без добавленных providers/models.
  - Acceptance: обычный provider list/search/categories/checkmark → accounts или
    method → masked key/существующий разрешённый OAuth → durable ACK → filtered picker.
    Единственный interactive method открывается сразу; env source не selectable login.
    Перед API key нет обязательного account label: default `default`, Rename позже.
    New custom отличается только entry `Custom provider…`, name/base URL, explicit
    Key/anonymous и protocol confirmation при неоднозначности. Esc/pending/error/retry
    и focus/draft restoration действительны; late response не меняет другой view.
    Нет model auto-commit; Home/history/commands работают без модели, submit/headless
    отказывает до session/turn/generation/tools effects.
  - Primary evidence: CONN04 TUI owner tests и existing `pty_t39` actual debug/release
    binary zero-provider → connect → discovery → explicit model → fake request → restart.
    Shared auth presentation остаётся T44/VIS45, не второй visual owner.
  - Status: verified.
- R5: новые каталоги публикуются coherent, без stale credentials/selection/reload effects.
  - Source: немедленно показать fetched models; согласованный narrow publication plan;
    существующие captured request, read-only CLI и one-owner invariants.
  - Acceptance: discovery outcome связан с provider ID и captured binding revision;
    старый key/Location/form результат не публикуется. Добавление connection не меняет
    committed/saved unavailable selection, generation ID или Location epoch, не reload
    проекта и не restart MCP. Все catalog consumers получают safe provider-qualified
    view; `oc models` читает bounded sanitized saved snapshot без custom endpoint probe,
    credentials, второго Db owner или выбора модели; incomplete даёт nonzero.
  - Primary evidence: CONN05 barrier-held actor/runtime results и read-only CLI/cache
    effect counters, held prepared request и next-request/reopen assertions.
  - Status: verified.
### Constraints and non-goals

- Четыре existing crates, один application/Db/credential/discovery work owner;
  no Node/Bun/npm/JS/plugin runtime, generic auth registry/service или второй store.
- Не удалять `ludka2` compatibility, не переписывать OpenProxy oracle/merge/caps/
  retirement/credential priority. Alias сам по себе по-прежнему не разрешает probing.
- Explicit local/LAN connection — узкая native provenance; сохранить DNS-before-dial,
  pin/peer check, prefix/origin, no redirect/proxy и запрет metadata/link-local.
  Не использовать `OC_TEST_ALLOW_LOOPBACK` для production onboarding; webfetch не расширять.
- Нет model/vendor-name routing, платного detection POST, hidden API/auth fallback,
  host scan или автоматического добавления `/v1`. URL literal, не env/file substitution.
- Custom auth — Key либо explicit anonymous. Произвольный OAuth/MCP autodiscovery,
  command auth, Ollama/Gemini native wires, config writer и auth.json/import вне scope.
  Existing built-in OpenAI browser/device OAuth остаётся T57, не обнаруживается по URL.
- Masked ephemeral secrets вне composer/history/Debug/copy/diagnostics; remote URL,
  package, headers, auth/options/body не исполняются как connection overrides.
- Raw history/DCP/replay/permissions/caps и captured prepared request/tools неизменны;
  next request той же задачи потребляет committed selection по T50. Busy connection/
  credential mutations отказывают по existing admission, не ослабляют модельный switch.
- Минимальные самостоятельные risks проверять у owner; не копировать всю матрицу
  на каждый слой, не менять baseline/ignored/deadlines ради PASS. No new paid campaign.

## OC2 source facts and chosen UX

Oracle: donor v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`, не moving web docs.
Read-only native baseline `d4cfa8698b67e57036a0275a94f541b8f1b6c6d2`; dirty T44 files
не являются частью этого плана/доказательством нового поведения.

- `opencode/packages/tui/src/app.tsx:934–945` и
  `packages/tui/src/component/dialog-integration.tsx:75–313,459–773,995–1029`:
  `Connect an integration`, Popular/Services/MCP, search/checkmark/source summary,
  one-method shortcut, Add account/activate/rename/two-trigger Delete и ACK→models.
- `opencode/packages/core/src/integration.ts:700–723` сохраняет Key, не валидирует
  upstream запросом; `packages/core/src/credential.ts:100–125` default label `default`.
  Текущий native `oc-tui/src/app/accounts.rs` обязательный `Label → Key` убрать.
- OAuth Starting → URL/instructions → Waiting → outcome, `o open`/`c copy`/Esc:
  reuse T57/VIS45 owner. Device copies typed user code, browser — authorization URL;
  не переносить donor regex extraction из instructions, если typed field уже есть.
- Pinned TUI не содержит `Other` endpoint wizard, хотя web docs его описывают.
  `opencode/packages/core/src/config/plugin/provider.ts:23–41` добавляет key method
  для уже сконфигурированного custom provider. Новый wizard — явно native extension.
- В доноре plaintext key textarea; native masking — named security difference VIS45.
  Не повторять unmount/late-result слабости донора ради буквального совпадения.
- Models references: OpenAI list `id/created/object/owned_by` не стандартизует
  protocols/modalities/efforts; [Anthropic Models](https://platform.claude.com/docs/en/api/models/list)
  даёт paginated capability flags; [OpenProxy](https://github.com/0FL01/openproxy)
  `opencode` extension содержит supplied modality/effort metadata. Ничего не угадывать.

```text
/connect → provider → accounts или method → key / existing approved OAuth
         → acknowledged connect → provider-filtered models → explicit choice

/connect → Custom provider… → name → API base URL → API key / No authentication
         → read-only catalog discovery → API confirmation if ambiguous
         → save acknowledgement → provider-filtered models → explicit choice
```

Probe не меняет storage; cancel до Save ничего не сохраняет. Discovery failure не
запрещает явно выбрать concrete API и сохранить connection/key: ACK и catalog status
разные факты. Picker показывает loading/empty/failed/Retry/change-key, без false server
validation или стирания сохранённого key. Повторный connect использует обычные accounts.

## Material Decisions

- 2026-10-12: ambiguous API подтверждает пользователь; detection только read-only.
  ID-only models visible, metadata unknown вместо model-name guesses.
- 2026-10-12: typed same-Db table, не prefs/config writer; narrow provider publication,
  не full reload. One-owner captured binding и immutable raw остаются обязательными.
- 2026-10-12: familiar OC2 auth/accounts/picker, без mandatory account label; custom
  setup — narrow extension. API confirmation не auth-method selection; OAuth scope не расширяется.
