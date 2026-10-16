# T32 red baseline — F04/F05

2026-09-21, HEAD `e92b614`, before patch runtime/schema edits. Started T32 using
the existing journal. Tests run as the dedicated non-root account; every project,
data directory and outside sentinel was under a fresh temporary directory.

Command: `cargo test --locked -p oc-adapters --test patch_audit -- --nocapture`

Exit **101**, **0 passed, 3 failed**, test runtime 0.02s (compile 10.18s):

```text
aud03_add_file_decodes_the_patch_plus_prefix FAILED
left:  [43, 104, 101, 108, 108, 111, 10]   (+hello\n)
right: [104, 101, 108, 108, 111, 10]       (hello\n)

aud04_predictable_temp_symlink_must_not_modify_outside_file FAILED
left:  [43, 115, 97, 102, 101, 10]        (+safe\n)
right: [68, 79, 32, 78, 79, 84, 32, 84, 79, 85, 67, 72, 10]
The preplanted new.txt.tmp-<pid> symlink caused the outside sentinel to be overwritten.

aud05_late_stale_hunk_must_not_commit_earlier_file FAILED
deterministic preimage failure committed a file
```

The first two reproduce the supplied independent audit sketches; the third
pins T32 preflight acceptance. Assertions were not relaxed. The final file
extends these with the distinct races/partial failures required by AUD03–05.
No valuable data, live provider, MCP endpoint or user credentials were involved.
