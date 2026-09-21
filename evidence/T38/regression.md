# T38 initial regressions (RED)

Base HEAD `4574a2d` (T37 closeout). All checks used temporary roots and loopback
fixtures; no live/paid calls and no real credentials.

## Captured against the pre-repair implementation

A detached worktree at `4574a2d` received the new test files unchanged and ran
them with the shared target directory.

### AUD28 / AUD25 / AUD26 (`cargo test -p oc-adapters --test t38_red`)

Exit 101, **0 passed / 4 failed**:

- `aud28_strict_allowlist_drops_unlisted_names`
  `unlisted names must be dropped: {"HOME": "/home/fixture", "LANG": "C.UTF-8",
  "MYAPP_OK": "1", "PATH": "/usr/bin:/bin"}` — the substring denylist let an
  unlisted name through.
- `aud28_symlink_cwd_escape_is_refused`
  `symlink cwd escape was allowed; child ran in
  /home/opencode/.cache/opencode-tmp/.tmpMb31MR/outside` — lexical containment
  only.
- `aud25_html_multibyte_input_must_not_panic`
  `byte index 4 is not a char boundary; it is inside 'П' (bytes 3..5) of
  <p>Привет 🦀</p>` — `html[i..]` slicing panicked on valid UTF-8.
- `aud26_total_deadline_spans_redirect_hops`
  elapsed `1.559s`, result `TooManyRedirects` instead of `Deadline`: the timeout
  applied per request, so five delayed hops ran far past the 400 ms budget.

### AUD27 (`cargo test -p oc-adapters --test shell_watchdog`)

Exit 101, driver failed:

- `case leader-exit-descendant-pipe failed`
- child: `owned descendant survived the group teardown`

An earlier variant of the same case (descendant `sleep 30 & exit 0`) showed the
harsher failure mode: `case leader-exit-descendant-pipe was not bounded:
30.003498348s` — the old supervisor blocked joining the drain thread until the
descendant exited naturally, i.e. exactly the AUD27 hang.

## Test-only hints removed (production parity)

`e2e_offline`/`e2e_live` previously passed `RUSTC` as a synthetic parent-env
entry and invoked absolute `cargo`, because the old child `PATH` was a fixed
`/usr/bin:/bin`. Under the T38 contract that is a test-only absolute hint: both
now pass the process's real observed environment and call bare `cargo`, which is
how production resolves the toolchain. The first run after the change failed with
`could not execute process 'rustc -vV' (never executed)`, proving the old tests
depended on the hint; after switching to the production environment the suites
pass with no hint.

## Transient note

One intermediate run failed with `error: reap failed` that did not reproduce in
three consecutive runs of the same test. The supervisor now reports the OS error
for a refused exec (`spawn refused: exec: …`) instead of a bare `reap failed`, so
a future occurrence is diagnosable. This was not accepted as a pass: the case was
rerun and the whole workspace gate repeated green.
