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

[D4] https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/config.ts

D4 read 2026-09-27: schema/defaultConfig/source paths, including upstream 50000/100000/summaryBuffer=true. Native owner-approved 40%/55%/false are a separate pending T45/R9/DCP12 policy; source reading is not runtime qualification or a baseline revision change.

Пути для последующего executable recon (не весь код проверен в этой редакции): `lib/compress/`, `lib/hooks.ts`, `lib/messages/`, `lib/prompts/`, `lib/config.ts`, `tests/`, `LICENSE` на том же commit. Не копировать README вместо точного test case.

## DCP 3.2.0 upgrade target

Read-only RECON 2026-10-03: v3.2.0 is commit
`d637981555a18c3992472268a0657a948925d5fa`, AGPL-3.0-or-later. Canonical repo is
`Tarquinen/opencode-dynamic-context-pruning`; package repository metadata retains the
redirected `Opencode-DCP` name. This is the approved pending donor target, not current
compiled baseline or an executable comparison. D1–D4 and historical T02/VIS38
provenance stay accurate; adopted fixtures/derivatives later append their own records.

[D11] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/package.json

[D12] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/lib/v2/index.ts

[D13] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/lib/v2/messages.ts

[D14] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/lib/protected-patterns.ts

[D15] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/tests/v2-protection.test.ts

[D16] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/tests/v2-messages.test.ts

[D17] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/tests/compaction-nudges.test.ts

[D18] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/tests/v2-ids.test.ts

[D19] https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/lib/config.ts

Adopted 3.2.0 derivative records (2026-10-05, AGPL-3.0-or-later, commit
`d637981555a18c3992472268a0657a948925d5fa`; translated to native Rust tests, the TS
sources/tests were not executed). D14/D15 → `dcp_auto.rs::tool_is_protected` alias
matching and `dcp_auto/controls_tests.rs::dcp320_protected_tool_names_follow_native_aliases`;
D16 → existing native projection tests (no product change); D17 →
`runtime_compaction_tests.rs::compaction_auto_threshold_and_known_overflow_keep_tool_effect_once`
summary-request assertions; D18 range/restart beyond 9999 →
`dcp.rs::tests::dcp320_ids_beyond_9999_keep_block_order_nesting_and_restart`
(normalized to native `m{seq}`/`b{id}`, no compact IDs). Mapping and native
differences: `evidence/T45/dcp320.md`. D1–D4 and the 3.1.15 display attribution remain
historical sources of the earlier port.

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
