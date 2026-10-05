# T53 — native numeric transport deadline

Base: `f4afe7b0a`. Partial R3/GO03; not full T53 PASS.

## Contract

Provider→model→variant `timeout` accepts a positive integer in milliseconds.
False/absent retains no total deadline; true, zero, negative, fractional and string
values are rejected. A positive `chunkTimeout` remains an independent idle budget;
the existing 6,000,000 ms default is unchanged.

The immutable wire binding captures the numeric budget. Its deadline starts before
DNS admission; the remaining budget covers connect, headers and the full streamed
body using the existing reqwest transport. Responses/Chat/Messages retain one
physical attempt, typed delivery/operation/status and output-committed facts. No
retry loop or artificial long-horizon round limit was added. The public legacy
`ResponsesConfig.timeout` bool field remains compatible; effective config carries
numeric milliseconds in its private binding.

## Verification

Approved disk TMPDIR, Cargo jobs 3, test threads 2:

```text
cargo test --locked -p oc-adapters --lib -- --test-threads=2  599/0/0
cargo clippy --locked -p oc-adapters --all-targets -- -D warnings  PASS
cargo fmt --all -- --check  PASS
git diff --check  PASS
```

`provider/timeout_tests.rs` exercises strict numeric admission, all six native
before-header/after-output timeout paths (one dispatch each), flowing SSE comments
which cannot reset a total deadline, and false/absent delayed successful requests.
An initial fake Responses delta omitted its required `type`; the structural failure
was diagnosed and the fixture corrected, not the parser relaxed. Initial type
conversion compile errors were resolved at all effective-config constructors.

Chronological effort consumers, full durable wire binding, connect/provider-aware
selection and bounded final live remain required separate slices. Concurrent
unrelated T57 planning changes were observed and are not staged by this slice.
