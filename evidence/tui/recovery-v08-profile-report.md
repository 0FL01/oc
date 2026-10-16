# T44 — paired explicit primary profile and Home metadata

## RECON

The previous reference's implicit `Build` and native fixture's absent
profile are not equivalent state. Pinned v2.0.12
`packages/schema/src/config.ts:35-58`, `config/agent.ts:9-22` and
`packages/core/src/config/plugin/agent.ts:88-124` support configured
`default_agent` and `agents.<id>.system/color/mode`. Native config uses
`default_agent` and `agent.<id>.prompt/mode` (`defs.rs:553-609`), deriving
the sole primary profile's color from categorical index zero. Test-only
`--agent-profile true` now uses supported config schemas to select `Reader`
on both sides and supplies the same fixture-only instructions. Original's
explicit color `#5c9cf5` is the vendored opencode dark
`categorical[0]` hue.blue.200; native's index-zero color is the same.
Both actual Home metadata and provider requests must reflect the profile:
the fake Responses server rejects transcript requests missing the shared
instruction sentinel. This checks effective instruction delivery and the
real read, **not** the complete equivalence of all default permission rules.
Ordinary no-agent fixture captures are unchanged.

Attempt `recovery-v08-profile-01` deliberately failed the new selection
check: the original displayed `Paired-Reader`, the native `paired-reader`.
Changing the genuine configured ID to `Reader` made both selected names
identical without painting a fake label. Attempts 02 and 03 use this ID;
both PTYs execute their real read with `provider_contract=true` and
`profile_prompt_present=true` on transcript requests, and SGR mouse
expand/recollapse succeeds. They use original v2.0.12 executable SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
the same 120×40 terminal profile ID
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`
and paired fixture SHA-256
`d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`.

## Native correction and paired result

After the equivalent read profile was selected, capture 02 showed the
Home left prompt border **0/5** differences x=23,y=20–24 and identical
agent/model/provider strings, but 12 foreground-only gaps across the
metadata row x=26–76,y=23: `·` separators were muted correctly while
flex-layout spaces and trailing blank were not. Pinned
`packages/tui/src/component/prompt/metadata.tsx:43-83` uses a box gap,
not tinted text spaces. A test-first TestBackend regression failed on the
first space. Native `shell.rs` now paints the metadata canvas truecolor
white, renders the text over only its actual width, and leaves layout
gaps and trailing blanks with the canvas foreground. The actual agent,
model, provider, variant colors and content remain unchanged.

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true --agent-profile true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-profile-03
python3 scripts/tui_capture/check_region.py \
  evidence/tui/recovery-v08-profile-03/upstream/home.cells.json \
  evidence/tui/recovery-v08-profile-03/oc/home.cells.json --rect 26 23 70 1
```

Attempt 03 independently selected **the same random Home example** in
both processes; the entire Home difference bounding box is now only
y=38, the bottom version row: **118/4,800** styled-cell differences and
**304/647,040** PNG pixels. Prompt metadata x=26–95,y=23 and border
x=23,y=20–24 are **0** symbol/style differences. Completed session
remains DIFFERENT **209/4,800** styled cells, including a genuinely
absent native add-tab action, variable measured duration, bottom version
and other chrome. Every full-frame comparator still exits 1. Do not
change the real native version to `2.0.12` to make a screenshot green.

## Checks and limits

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet` PASS (TUI 184, existing live ignores); workspace
fmt, all-target clippy -D warnings, locked build, runner Node syntax,
docs/progress and diff checks PASS. This pair is equivalent for selected
identity, instruction sentinel, fixture model, categorical color and
observed read operation; it is **not** an assertion that all original
default permissions equal native policy, nor a VIS PASS. The reference
and native random Home example choices can differ in another run. T44,
V08–V09 and remaining VIS01–VIS24 remain open.
