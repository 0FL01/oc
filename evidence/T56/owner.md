# T56 initial owner / native dependency spike (2026-10-06)

Frozen R1–R4/TERM01 remain the finish line. This slice proves the PTY/VT owner,
not frontend completion or paired VIS39. Base `ea6b0b2cc`; T56 started after T53
done, T44 remains PAUSED. No real API/auth calls and no user `.opencode/` reads.

## Dependency / provenance

Existing libc supplies Linux posix_openpt/grantpt/unlockpt/ptsname_r, atomic CLOEXEC,
controlling slave, TIOCSWINSZ and owned session groups. No new PTY dependency.
Pinned `vt100 =0.16.2` (MIT, Jesse Luehrs), lock transitives `vte 0.15.0`
(Apache-2.0 OR MIT) and `arrayvec 0.7.8` (MIT OR Apache-2.0). Crate sources/licenses
are the published crates.io archives resolved by Cargo.lock, not vendored donor code.
`cargo check -p oc-adapters` compiled these on pinned rustc/cargo 1.93.0, exit0.
Cells are bounded22-byte text + styles; emulator dimensions120x240, scrollback256.
vte std OSC otherwise retains unbounded bytes: native syntax-state gate caps
sequences1024 and discards overlong OSC/DCS through terminator, including aborted
CSI→OSC. Clipboard/window host effects are not forwarded. DSR/DA replies only go
to the owned PTY. Terminal title is bounded/control-free.

## Real fixture / bounds

Separate `terminals/tests.rs`: actual bash PTY/session/PID/cwd, strict existing env
allowlist excludes synthetic auth/unlisted input, input→output, raw Ctrl+C,
actual30x91 stty resize, atomic screen/cursor then byte replay; wrong actor/modified
generation refused. Two PTYs retain admission order, hidden output still drains,
hide clears selection without killing, remove/shutdown wait/reap. Unicode wide
cells and ANSI red, million-byte OSC,100k visible flood, explicit replay gap/current
screen and bounded ring/parser; stale identity comparison and no new spawn on
recovery. Native metadata only in migration12 of existing SQLite/Db/flock.

Eight concurrent PTYs; per-terminal32 input command slots,8192-byte messages,
64KiB pending input and output ring,256 scrollback rows; no output archive. Threads
are owned/joined; shell job-control groups are terminated before unreaped leader.
Creation/record/thread failure tears down actual process. No startup PTY spawn.

## Checks / material experiments

- Initial compile RED: test used obsolete two-argument Db::open; fixed to actual API.
- `cargo test --locked -p oc-adapters --lib terminals::tests::`:4/0, real PTYs.
- Strict impacted Clippy initially flagged bool::then/filter_map; changed filter/map.
- Current final `cargo clippy --locked -p oc-adapters -p oc-core --all-targets -- -D warnings`,
  workspace fmt and journal/diff checks exit0. No tests disabled or deadline/stack override.
- Next: wire the owner through existing application lifecycle and typed CoreApp,
  source/epoch admission and configless session binding, then actual frontend.
  R2/R3/R4 actual-binary and full TERM01 still NOT_RUN; no DONE claim.
