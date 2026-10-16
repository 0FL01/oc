# T45/R6 — profile color presentation

Atomic slice of R6 ("color … presentation semantics, not silent acceptance").
Not a T45 closeout. T44 stays PAUSED; no new picker binding or visual
qualification was added here.

## Donor reading (pinned `2670273ff17da96f85c5826ced57aa1b368754fa`)

- `packages/schema/src/config/agent.ts`: `Color = /^#[0-9a-fA-F]{6}$/`.
- `packages/core/src/v1/config/migrate.ts:156`: V1 color without `#` becomes
  `#aaaaaa`; the result is decoded through the native schema again.
- `packages/core/src/config/plugin/agent.ts:164–211`: a Markdown file is V1 when
  any frontmatter key is outside the native agent keys.
- `packages/tui/src/context/local.tsx:125–131`: `color(id)` uses the agent's
  explicit color first, otherwise the categorical palette slot.

## Native behavior

- `defs.rs::agent_color`: native `agents`/native Markdown require `#RRGGBB`;
  JSON `agent` and V1-shaped Markdown map a theme name to `#aaaaaa`; an invalid
  value refuses the profile with a diagnostic. Supplied-field merge keeps an
  earlier color.
- `TuiChrome.agent_colors` (presentation-only) carries explicit colors of every
  admitted profile, including subagent-only ones; `TuiState::agent_color`
  resolves by id before the categorical slot.
- Root and child lanes whose profile has an explicit color pin no categorical
  slot, so history/meta rows fall back to the same id resolution.
- `config.rs::parse_frontmatter_scalar` now decodes YAML 1.2 core decimal
  int/float plain scalars (`temperature: 0.1`), matching the donor YAML
  loader; quoted values and non-decimal forms stay strings.

## Checks

- `defs::profile_tests::r6_color_native_pattern_legacy_theme_migration_and_supplied_merge`
- `defs::profile_tests::r6_frontmatter_plain_scalars_follow_yaml_core_numbers`
- `application::profile_tests::r6_explicit_profile_color_reaches_tui_chrome_and_skips_slot_pin`
- `oc-tui app::tests::r6_explicit_profile_color_wins_over_categorical_slot`
- fmt check and strict workspace all-target Clippy: PASS.
- Full workspace test (3 jobs / 2 threads, bench TMPDIR): **1555 passed /
  1 failed / 10 ignored**; the only failure is the pre-existing webfetch
  `tool17_held_body_…` race recorded in `profile-request.md`.
