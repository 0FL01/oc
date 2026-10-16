# T44 — fresh Shell-mode prompt presentation

## Result and scope

Fresh Home Shell mode now uses the pinned example list and mode-specific footer;
session Shell prompts have no invented placeholder. Shell metadata is non-bold
and uses the existing leader-muted tone. Whole frozen R1–R6/VIS01–VIS45/T44 stays
ACTIVE/NOT_PASS. The actual version and independently selected examples remain
visible in strict comparisons; neither is replaced, masked or called pixel PASS.

## Existing owner and source contract

Pinned OC2 `routes/home.tsx:19–23` supplies three Shell examples: `ls -la`,
`git status`, `pwd`. `component/prompt/index.tsx:220–221,357–369,965–973,1583–1595`
has one placeholder index, selects on mode entry, and renders no placeholder when
the current example list is empty. Session prompts supply no Shell list. Native
`app/user_shell.rs` keeps the bounded index in the existing prompt owner; Home
entry updates the corresponding normal example too. Exit/redraw do not reselect;
another Home entry may select again. No random dependency, seed override or timer.

`feature-plugins/prompt/footer.tsx:107–114` replaces normal hints in Shell mode
with base-tone `esc` and muted `exit shell mode`, shortened to `shell` below44
columns. Native `shell.rs` preserves existing geometry, clipping, Location and
error/foreground feedback priority, suppressing normal agent/context/command/live
status hints and their hidden pointer target. `component/prompt/metadata.tsx`
uses a non-bold Shell label; pending leader uses the existing border tone.

No admission, executor, provider routing, schema/store, permission, selection,
RAW/result graph, capture/resource bound or key ownership changes. The existing
sequence owner consumes its pending Esc before a subsequent Esc leaves Shell
mode; tests verify that behavior rather than changing dispatch to suit a test.
No env/.opencode/GOAL/acceptance/Cargo.lock/auth changes or paid API calls.

## Current source association

Four independently read final built capture locks share base
`7cb9cd1e4fc2055dd30ca5757c606ad351b46f00`, tree
`372651946c52d8e6971bdf31718371a03a755543`, and:

- Dirty diff: `da7538400a66096fa94dab873ee5a9826e121bb970e5bac42bfdf142d6c0122b`.
- Source manifest: `6007ec0c2163329c04ccaf9512d1495bbd40f586ca11cfb829d1dec9b4275b04`.
- Actual debug ELF: `e9895b2167c403ae271249898dd21da43c16edf60af7610d0d8c205989856794`.
- Running original v2.0.12: `2670273ff17da96f85c5826ced57aa1b368754fa`.

Each uses `--build-oc true` / `cargo build --locked`. Factual docs added afterwards
do not rewrite immutable captures; reviewed Git records actual delivery.

## Actual process and full-frame evidence

[user-shell-mode-004/](user-shell-mode-004/)80×24 and
[user-shell-mode-005/](user-shell-mode-005/)120×40 each retain12 actual stages,
**10 EQUAL/14 DIFFERENT** full grid/PNG comparisons. Running, completed,
recalled-normal, explicitly entered recalled-mode and second-completed are exact
full styled-cell/PNG/cursor pairs. This does not expand the frozen VIS12 exclusion
of recalled full-parts/mode/Mini restoration at amendment2235–2238.

Typed-input frames differ by exactly six actual-version cells:80×24 x72–77/y22,
120×40 x112–117/y38; cursor and command/footer geometry agree. Empty-mode frames
have17/13 differing cells, including independently selected example text plus
those six version cells. Final actual native/reference PNGs were physically
viewed. Native reports0.1.0, original2.0.12: the real values remain unmodified.
Earlier examples happened to match; that is not an oracle for future independent
random selections. No crop, masking, realignment, forced refresh or source golden.

Both sides qualify exactly two explicit commands/effects, zero Responses/title/MCP
calls and no restart replay. Native retains two distinct NULL-turn operations/no
LLM turn, and genuine process-owned SQLite URI-modeRO witnesses committed exact
deduplicated history before effect. The existing finite first-six/later-three
second hold has zero output before completion and **does not prove live stdout**.
Diagnostic001–003 remain separate, untracked and outside delivery evidence.

Independent same-source regressions:

- [Normal model tools](user-shell-mode-tool-regression-001/): both27 stages,
  six fixture requests/four effects/three MCP calls/17-byte model Shell effect;
  no replay across switch/reopen/restart.4 EQUAL/50 DIFFERENT comparisons plus
  four native-only authorized capture-detail snapshots.
- [Temporal](user-shell-mode-temporal-001/):80×24 default/WebGL/collapsed/supported,
  six owners per side, all zero cycles appropriate to DEFAULT, no phantom/lost
  caret/draft/replay; all160 comparisons DIFFERENT. Independent
  [matched audit](user-shell-mode-temporal-matched-001/) checks144 opaque PNGs,
  72 phase pairs and17,694,720 full-raster pixels per side. Result:
  QUALIFIED_CURSOR_BEHAVIOR_ONLY/pixel NOT_PASS; DEFAULT is not blink proof.

## Current checks

Fmt-check, strict locked workspace all-target Clippy `-D warnings`, full locked
workspace tests, locked/ordinary debug and locked release builds, both ELF help,
actual release startup19 and GET-only discovery8 cases PASS.
**1776 passed/0 failed/11 unchanged opt-in ignores across46 workspace results**.
The separate focused8 result is not added to the workspace total. Full current log:
`/home/opencode/.local/share/opencode/tool-output/tool_11e7a9db1001EwLmwV45NfRx6M`.

The new UI scenario covers Home/session,43/44/80/120-column empty/nonempty input,
non-bold/base/muted/leader tones, actual caret/draft, no hidden live-status hit,
existing sequence cancellation and zero Core queries/effects. The receipt test
also checks shared normal/Shell selection and redraw stability. Independent
source review confirms the pinned co-index lifecycle; full production/test diff
review found no remaining concrete risk in this slice.

Python unittest discovery47, Node capture/tool-preview/user-Shell syntax, fixture
py_compile, source whitespace/docs/progress and advisory code-size checks PASS.
Changed-file footprint5 files/4523 physical lines/168848 bytes; no new >5k file.
Application facade5425 is unchanged. No test or resource/security guard disabled.

Additional unchanged actual-binary gates:

- VIS31 idle1.004604078s/zero CPU ticks;165/250Hz p50/p95/max2.298/7.728/7.908ms
  and4.345/9.455/9.582ms. Burst14.433/25.779/27.652ms and
  14.695/21.439/21.495ms, queue peaks63/69/no lag/settled live0, below100ms.
- Equal-view archive0→3000 RSS65596→67936KiB/PSS63306→65676KiB, retained22896
  bytes/152 rows/cache7517 unchanged, queues7/5/no lag/children0,
  frames90/90, elapsed2228/2221ms.
- AUD32 archive8→3000 peak RSS56904→57020KiB (+116KiB), peak PSS54170→54192KiB,
  end PSS49658→47625KiB, children0/threads8/Db942080→152944640 bytes;
  unchanged64MiB guard.

## Remaining contract

Next connect bounded typed partial stdout/stderr from the existing capture owner
to the direct-user card on existing ShellChanged events and prove real output
before terminal completion. Separate native panels do not qualify the combined
Subagents/Shell/Terminals VIS39 composer. Keep every remaining frozen outcome and
all genuine Home/version/random-example/restart frame differences visible until
their actual contract is verified, followed by final current-source R6/V09.
