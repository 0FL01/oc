# T33 — initial failures on current HEAD

Base: `18b0792` (T31/T32 already delivered; no rollback to audit SHA).
All files, SQLite triggers, endpoints and subprocesses use temporary fixtures.

Before the parent production durability edits:

```text
$ cargo test --locked -p oc-adapters --test runtime aud0 -- --nocapture
Finished test profile in 3.24s
aud07_rejected_input_has_no_turn_or_event: FAILED
  unaccepted input left a started turn: left 1, right 0
aud07_terminal_failure_does_not_commit_assistant: FAILED
  left [(user, input), (assistant, must not commit)]
  right [(user, input)]
aud06_intent_failure_prevents_patch: FAILED
  mutation ran before durable intent
test result: FAILED. 0 passed; 3 failed; 12 filtered out; 0.21s
exit: 101
```

Independent blob workstream, before its production edits:

```text
$ cargo test --locked -p oc-adapters --test blob_audit existing_content_addressed_orphan_is_readable_after_write -- --exact
existing_content_addressed_orphan_is_readable_after_write: FAILED
called Result::unwrap() on an Err value: BlobNotFound
test result: FAILED. 0 passed; 1 failed
exit: 101
```

These were executed failures, not inference from code. Assertions retained.
Later checks are recorded separately; no old PASS claim substitutes for them.
