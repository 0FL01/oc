# T44 startup trace checkpoint

## Result

Native `oc` now writes a bounded, redacted, per-launch startup trace to a file (`OC_STARTUP_TRACE`, else `$XDG_STATE_HOME/oc/startup-trace.log`, else `$HOME/.local/state/oc/startup-trace.log`), mode 0600, truncated each launch. Stages cover bootstrap, TTY/data-dir resolution, TUI begin/exit, application spawn, config selectors/roots/sources, selected model/agent, provider id/base URL/credential source, env references, DCP/definition failures, discovery URL/attempt/status/class/retry, storage open, runtime ready and typed failure categories. No environment values, header values, response bodies or config contents are written; the optional `OC_STARTUP_TRACE_FINGERPRINT=1` appends a truncated one-way digest for credential comparison. Release binary rebuilt. T44 implementation remains paused; this is the requested startup diagnostic deliverable.

## Checks

Implementing agent: trace unit tests 6 passed; `recovery_startup` 3 passed; clippy/build/fmt/diff 0; serial workspace 0 with five existing ignores; two real intermediate failures and fixes are in `trace-report.md`. Parent after the `detail_len` headless change: fmt/test/clippy 0, full serial workspace exit 0 (36 green groups, zero failures), `cargo build --locked --release -p oc` 0. Real-profile release smoke: Home, Ctrl+C exit 0, trace written, credential absent. Trace sample and commands in `trace-report.md`.

## Risks

Only loopback fixtures plus the agent's own inherited product profile were exercised; the owner's shell identity and catalog status remain unknown until they run the rebuilt binary there. No log rotation exists by design (per-launch truncate). `oc --smoke` and `sessions list` are not traced. The optional fingerprint is secret-derived data written only on explicit opt-in.

## Next

Owner runs the rebuilt `target/release/oc` (or an `oc2` symlink) in the failing shell with `OC_STARTUP_TRACE_FINGERPRINT=1`, then compares the `provider.api_key`/`env.ref` fingerprint and `config.global` root against the working smoke (`fingerprint=54f454fc`, root `/home/opencode/.config/opencode`). The trace identifies the exact failing stage and typed category. T44 visual/resource work resumes only after the pause is lifted.
