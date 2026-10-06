# T53 — Chat/Messages runtime retry qualification

Base: `57f7c459a`. This closes the required new-wire runtime qualification,
not a new retry implementation or a live Go claim.

`runtime/binding_replay_tests.rs::go03_chat_and_messages_runtime_retry_records_partial_without_tool_replay`
passes both Chat and Messages through the actual runtime and shared T54 owner:

1. HTTP 429 with Retry-After, no output: retry retains exactly the same captured body.
2. Complete native read call: one settled local read and its normal follow-up.
3. Native text plus incomplete second read arguments, EOF without a genuine terminal:
   recorded partial text continues, the incomplete call never executes or becomes input.
4. Complete native final answer: settled note result remains in continuation once.

Each wire records exactly four request receipts and four `generation_dispatched`
events, two retry events, one tool operation and one executed call. Failed span has
the typed retry/finish/error facts; reopen reads the identical settled TurnLog and
one original tool outcome. No adapter-owned retry, replay, second executor or
deadline/test-stack override was introduced. All eight HTTP sends use a synthetic
local peer, not the real Go service.

Commands, disk TMPDIR, jobs=3/test threads=2:

```text
cargo test --locked -p oc-adapters --lib go03_chat_and_messages_runtime_retry PASS 1/0
cargo test --locked --workspace                                            PASS 1648/0/10
cargo clippy --locked --workspace --all-targets -- -D warnings              PASS
cargo fmt --all -- --check                                                 PASS
cargo build --locked; target/debug/oc --help                                PASS
git diff --check                                                          PASS
```

The ten pre-existing opt-in ignored tests are not live PASS. GO06 remains blocked
by absent authorized Go credential; OpenProxy credentials cannot replace it.
