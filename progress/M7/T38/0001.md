## Result
T38 (F12/AUD27-AUD28, F13/AUD25-AUD26) implemented and verified: shell supervisor with concurrent stdin/output supervision, bounded group teardown, strict child env allowlist and canonical cwd containment; webfetch with Unicode-safe HTML, reqwest::Url handling, dial-bound guarded resolver and one total budget.

## Checks
cargo test --workspace --locked exit 0 (308 passed / 0 failed; only the 3 pre-existing external harnesses ignored); targeted adapters unit 129, shell_watchdog 2, e2e_offline 3, soak 4, runtime 31; clippy -D warnings, fmt --check, build, oc --help, progress.py check, check_docs.py, git diff --check all exit 0.

## Risks
AUD27 cases are Linux-specific (session groups). One non-reproducible `reap failed` observed before the spawn-diagnostics change; 3 reruns plus full workspace green, now reported with the OS error. HTML scanner is not spec-complete; exotic markup degrades to text.

## Next
Commit T38 implementation and evidence, close the task, start T39 (TUI, F14/AUD29-AUD31).
