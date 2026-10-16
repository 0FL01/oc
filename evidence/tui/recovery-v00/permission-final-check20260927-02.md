# Current-source evidence checks

Scope: captures15–28, final selected23–28, native headless02. See report03 for
source hashes, all failed attempts, real lifecycle, exact differences and gaps.

Successful commands (exit0):

```sh
node scripts/tui_capture/check_permission_evidence.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-validation20260927-05.json \
  permission20260927-23,permission20260927-24,permission20260927-25,permission20260927-26 \
  permission20260927-27,permission20260927-28 permission-headless20260927-02
node scripts/tui_capture/permission_association.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-association20260927-02.json 15 28
node scripts/tui_capture/check_frontend.mjs
node --check scripts/tui_capture/permission.mjs
node --check scripts/tui_capture/analyze_permission.mjs
node --check scripts/tui_capture/summarize_permission.mjs
node --check scripts/tui_capture/check_permission_evidence.mjs
node --check scripts/tui_capture/permission_association.mjs
python3 -m py_compile scripts/tui_capture/permission_fixture.py scripts/tui_capture/permission_headless.py
git diff --check
```

Output paths are immutable; choose new names to repeat report-generating commands.
Capture full comparators return1 for differences/INVALID; lifecycle PASS and
comparator equality are distinct measured results. All14 builds used the capture
runner's `cargo build --locked`, exit0. No Cargo tests/clippy/release/acceptance
were run by this evidence-only agent. Cargo is released.
