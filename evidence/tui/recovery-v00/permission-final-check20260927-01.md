# Final script/check state

After final report02, the following commands exit0:

```sh
node --check scripts/tui_capture/capture.mjs
node --check scripts/tui_capture/permission.mjs
node --check scripts/tui_capture/analyze_permission.mjs
node --check scripts/tui_capture/summarize_permission.mjs
node --check scripts/tui_capture/check_permission_evidence.mjs
python3 -m py_compile scripts/tui_capture/permission_fixture.py scripts/tui_capture/permission_mcp.py scripts/tui_capture/permission_headless.py scripts/tui_capture/bridge.py scripts/tui_capture/apply_patch_fixture.py
git diff --check
```

Final `git rev-parse HEAD` observed `cc837fa2c25c2f00ef1ca2c9833a477a3a3d67ef`,
`docs(runtime): require rootless Linux environment context`. This is a concurrent
parent documentation commit after capture work; the evidence agent made no commit.
Capture manifests/build association remain their recorded historical facts.
Owner's approval Rust changes are still dirty, and `.opencode/` remains uninspected.
Own changes are capture scripts and new evidence only. Cargo remains released.
