#!/usr/bin/env bash
# Full offline gate: one Cargo build owner, unchanged tests, explicit doctests.
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v cargo-nextest >/dev/null; then
    nextest=(cargo nextest)
elif [[ -x .local/tools/cargo-nextest ]]; then
    nextest=(.local/tools/cargo-nextest nextest)
else
    echo 'cargo-nextest is required; see docs/TEST_PLAN.md.' >&2
    exit 1
fi
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}"
# Preserve the real toolchain homes when fixtures replace HOME. Production shell
# forwards the captured owner environment; fixture environments remain explicit.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$(rustup show home)}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-$(rustup show active-toolchain | awk '{print $1}')}"
unset XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME XDG_CACHE_HOME XDG_CONFIG_DIRS XDG_DATA_DIRS
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
"${nextest[@]}" run --workspace --locked --no-fail-fast
# Nextest runs unit/integration tests; rustdoc remains Cargo-owned.
cargo test --workspace --locked --doc --no-fail-fast
cargo build -p oc2 --bin oc2 --locked
target/debug/oc2 --help >/dev/null
target/debug/oc2 --version
