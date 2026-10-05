# T53 durable wire authority — first slice

Base: `d06ea615a`. Partial GO04, not full T53 PASS.

## Implemented

- One finite durable protocol discriminator in core: absent means legacy Responses;
  explicit unknown fails. Prepared primary receipts and TurnLog carry provider,
  actual API model, protocol, deployment/source-trust digest and effective
  auth/header/tenant digest. Connection secrets are not persisted in these facts.
- Main prepared requests, retry/recovery and busy continuations project against
  the captured effective model/variant binding, not composer selection.
- Unknown/foreign binding retains public text and settled tool pairs/results,
  but withholds opaque provider IDs, encrypted reasoning, redacted thinking and
  signatures. RAW is unchanged. Legacy journals remain readable and do not acquire
  current authority retrospectively when resumed.
- SQL `json_object` projection, selected HOT/current working renewal, original raw
  segments, fork rebasing and restart preserve original protocol/binding facts.
- Native route digests now include finite protocol and API/deployment/auth authority;
  independently selected auxiliary/variant route consumers are the next slice.

## Checks

Using approved disk TMPDIR, Cargo jobs=3, test threads=2:

```text
cargo test --locked -p oc-adapters --lib go04_           PASS 3/0
cargo test --locked --workspace                         PASS 1627/0/10
cargo clippy --locked --workspace --all-targets -- -D warnings PASS
cargo fmt --all -- --check                              PASS
cargo build --locked                                   PASS
target/debug/oc --help                                  PASS
git diff --check                                        PASS
```

Workspace includes existing actual-binary PTY, retry, compaction, runtime and child
gates. Ten existing opt-in live/internal ignored tests are unchanged, not live PASS.
Final review moved initial binding capture to new-log creation: resumed legacy
unknown prefixes must remain unknown even after a new physical receipt is appended.
Final targeted/crate verification is recorded below after that review.

Experiments: first compile exposed an unavailable URL serde feature; used a narrow
endpoint digest method rather than adding dependencies or publishing endpoint data.
Tests cover all three wires, independent mismatches of provider/API/protocol/
deployment/auth, missing authority, receipt coordinate ownership, HOT, SQL, raw,
fork and reopen. Ordinary tool results remain durable and are never re-executed.

## Remaining

Native checkpoint binding-aware replay and selected auxiliary/variant route checks,
cross-protocol full actual runtime continuation, provider-qualified/configless
connect UI and bounded GO06 live/final report remain pending. No paid/live calls,
new dependencies, test suppression, secret files or user `.opencode/` were used.

Final post-review verification: `cargo test --locked -p oc-adapters --lib`
**606/0/0**, strict workspace all-targets clippy, fmt and diff check PASS.
