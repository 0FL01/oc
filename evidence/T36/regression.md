# T36 initial regression

Base HEAD: `1aac158` (T35 closeout). All endpoints and project/data roots were
temporary offline fixtures; no live/paid request or real credential.

Before production changes, the new actual-binary regression ran:

```sh
cargo test -p oc --test dcp_runtime -- --nocapture
```

Exit **101** after compilation; 0 passed, 1 failed. The first captured request
from the real `oc` binary exposed only:

```text
["read", "apply_patch", "bash", "webfetch", "skill"]
```

Failure:

```text
AUD19 missing model-visible compress function schema; visible tools:
["read", "apply_patch", "bash", "webfetch", "skill"]
```

The later glob/grep call, nudge, projection, durability and restart assertions
were unreachable in this red run. They are credited only by subsequent executed
green tests, not inferred from this failure.
