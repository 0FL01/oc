# T35 initial regressions

Base HEAD: `9daa55c`. All automated fixtures use temporary HOME/XDG/projects,
loopback fake Responses endpoints and trap executables. No live/paid request.

## User-observed actual config

The user ran `target/debug/oc tui` and observed:

```text
error: /home/opencode/.config/opencode/opencode.jsonc: unsupported plugin @tarquinen/opencode-dcp@latest
```

Targeted key-only inspection confirmed that exact marker without reading or
recording credentials. After admitting it to the fixed compiled native DCP,
a bounded local run still stopped before provider I/O at the next exact marker:

```text
UnsupportedPlugin: unsupported plugin @prevalentware/opencode-goal-plugin@0.1.49
```

The latter is now an exact authoring-only compatibility marker: warning + no
capability/import/process/network. Arbitrary packages/URLs remain hard errors.

## Actual-binary RED

Before production workspace wiring:

```sh
cargo test --locked -p oc --test configured_workspace -- --nocapture
```

Exit 101: 0 passed, 6 failed. The failures proved:

- `@tarquinen/opencode-dcp@latest` rejected before startup;
- legacy write/edit and selected-agent denies allowed real temp patch effects;
- a selected malformed agent reached the provider;
- valid skill metadata was absent from the request;
- unknown plugin diagnostic lacked the typed `UnsupportedPlugin` category.

Assertions were not weakened to current implementation. One initial assertion
that counted model-visible `compress` was corrected before green because that is
explicitly T36/AUD19 scope; T35 proves exact native admission/dedup and no JS.

## Discovery oracle RED

Before AUD18 repairs, `cargo test -p oc-adapters aud18 -- --nocapture` reported
5 failed / 3 passed. Failures covered JavaScript safe-integer `2^53`, first-slash
model naming, standard URL validation, case-insensitive reserved header replacement
and successful non-200 2xx handling. The locked user oracle remained authoritative.
