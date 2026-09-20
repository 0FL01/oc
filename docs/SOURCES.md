# Источники и provenance

USER-Q01–Q07 — ответы владельца в текущем диалоге; USER-DISCOVERY — присланный код, сохранённый в `references/openproxy-models.user.mjs` с нормализованным форматированием. User facts выше inference и live docs; их не заменять более старым repository plugin.

Все ссылки ниже — первичные источники. Pinned ссылки используются для fixtures, live documentation — справочная и может отличаться. Дата чтения 2026-09-20. Никаких выводов о live endpoint health из GitHub source.

## OpenCode baseline

[O1] https://github.com/anomalyco/opencode/tree/b8cedc1a7a5e2916bbb65dc1d4b620729c261638

[O2] https://github.com/anomalyco/opencode/blob/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/src/config.ts

[O3] https://github.com/anomalyco/opencode/blob/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/src/config/variable.ts

[O4] https://github.com/anomalyco/opencode/blob/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/codemode/README.md

[O5] https://github.com/anomalyco/opencode/blob/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/src/config/discovery.ts

[O6] https://github.com/anomalyco/opencode/tree/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/src/config/plugin

[O7] https://github.com/anomalyco/opencode/blob/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/src/plugin/source-directory.ts

[O8] https://github.com/anomalyco/opencode/blob/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/src/tool/plugin/skill.ts

[O9] https://github.com/anomalyco/opencode/tree/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/core/test/config

[O10] https://github.com/anomalyco/opencode/tree/b8cedc1a7a5e2916bbb65dc1d4b620729c261638/packages/schema/src/config

T02 extracts representative source-derived fixtures from `config.test.ts`, `skill.test.ts`, `agent.test.ts`, `command.test.ts`, `command-subagent.test.ts` and `instruction-discovery.test.ts`, plus corresponding config/plugin source files. Fixtures retain exact path/commit/license and normalization metadata; this list is not a claim that the upstream suite was executed.

## DCP

[D1] https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/README.md

[D2] https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/package.json

[D3] https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/types.ts

Пути для последующего executable recon (не весь код проверен в этой редакции): `lib/compress/`, `lib/hooks.ts`, `lib/messages/`, `lib/prompts/`, `lib/config.ts`, `tests/`, `LICENSE` на том же commit. Не копировать README вместо точного test case.

## OpenProxy

[P1] https://github.com/0FL01/openproxy/blob/4ef76dbce2cdbb85206cbe5e59acbad9d96ae387/contracts/lean-proxy.md

[P2] https://github.com/0FL01/openproxy/blob/4ef76dbce2cdbb85206cbe5e59acbad9d96ae387/src/server/api/mod.rs

[P3] https://github.com/0FL01/openproxy/blob/4ef76dbce2cdbb85206cbe5e59acbad9d96ae387/src/server/api/codex_web_mcp.rs

[P4] https://github.com/0FL01/openproxy/blob/4ef76dbce2cdbb85206cbe5e59acbad9d96ae387/plugins/openproxy-models.js

## Официальные справочные документы

[W1] https://ai-sdk.dev/providers/ai-sdk-providers/openai — native family/default API; generic compatibility name не доказывает все provider options.

[W2] https://developers.openai.com/codex/cli/slash-commands — при чтении redirected на https://learn.chatgpt.com/docs/developer-commands?surface=cli ; `/goal`, maximum objective 4000 chars, YOLO flags и `/status`.

[W3] https://modelcontextprotocol.io/specification/2025-11-25/basic/transports — HTTP/stdio и negotiated headers; проверить конкретный rmcp version в compile spike.

[W4] https://developers.openai.com/api/docs/guides/reasoning — continuation/reasoning output; поддержка конкретного OpenProxy model path требует wire/live tests.

## Evidence discipline

Каждый derived fixture хранит source repo/commit/path/license и normalization recipe. Synthetic fixture помечается synthetic. Report содержит executed command/exit/time/commit; source reading, inferred contract и executable comparison — разные методы. User JS fixture не требуется устанавливать в production.
