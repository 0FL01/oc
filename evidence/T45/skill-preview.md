# T45/R10 — skill preview ordering and per-lane permission filtering

R10 (spec line 132): "Skill preview has pinned deterministic ordering/permission
filtering; missing description or autoinvoke:false excludes only automatic
preview while explicit use survives." Not an R10 closeout.

## Donor reading (pinned OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`)

- `packages/core/src/skill/instructions.ts:70–80`: preview lists
  `Skill.available(skills, permissions)` minus skills without description or
  with `autoinvoke === false`, sorted by id.
- `packages/core/src/skill.ts:33–34`: `available` drops skills whose
  `skill`/id permission evaluates to deny.
- `packages/core/src/config/plugin/skill-file.ts:23–51`: `autoinvoke` comes
  from frontmatter `metadata."opencode/autoinvoke"` as a boolean or a
  case-insensitive "true"/"false" string.

## Native behavior

- `config.rs::parse_skill` parses `metadata."opencode/autoinvoke"` with the
  same boolean rule; `tools.rs::SkillSnapshot::projection` excludes
  no-description and `autoinvoke: false` entries (BTreeMap id order). Explicit
  `skill` calls keep their existing admission and still load the body.
- `runtime/turn.rs::skills_input` renders the preview per lane after its final
  policy: root (`Runtime::primary_lane`) and child (`child_lane`, parent rules
  narrowed by the child profile) omit ids whose `skill` effect is Deny. Native
  default for an unconfigured action is Deny, so a lane that cannot call
  `skill` no longer advertises a preview (previously the preview was shown
  even when the tool was hidden).
- Bodies stay out of every request until the native skill result.

## Checks

- `config.rs::tests::r10_skill_autoinvoke_metadata_follows_pinned_boolean_parsing`
- `tests/subagent.rs::r10_skill_preview_is_ordered_and_filtered_per_lane_policy`
  (alpha before zeta; root deny `secret`, child deny `zeta`; `manual`
  autoinvoke "FALSE" and no-description `bare` excluded; no body text). RED
  confirmed by disabling the permission filter.
- `pty_t39::lifecycle::s07_pty_equal_view_archive_resource_samples`: the
  fixture permits only `compress`; expected input length 202 → 201 because the
  hidden-tool preview is no longer advertised, now asserted explicitly
  (no `skill` tool, no preview). `pty_t39` 48/0.
- Full workspace before the s07 expectation update: 1563 passed / 1 failed
  (that s07 count) / 10 ignored; fmt and strict workspace Clippy PASS.
