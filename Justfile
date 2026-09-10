set shell := ["bash", "-euo", "pipefail", "-c"]

check:
    python3 scripts/check-contracts.py
    scripts/rebuild-protobuf.sh --verify
    lua tests/temporal_poll.lua examples/lib/temporal_poll.lua
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p 'test_pack*.py' -v
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p 'test_release*.py' -v
    cargo fmt --all -- --check
    cargo test --workspace --locked --offline
    cargo clippy --workspace --all-targets --locked --offline -- -D warnings
    cargo fmt --manifest-path tools/component-conformance/Cargo.toml -- --check
    cargo test --manifest-path tools/component-conformance/Cargo.toml --locked --offline
    cargo clippy --manifest-path tools/component-conformance/Cargo.toml --all-targets --locked --offline -- -D warnings

build:
    root="$(pwd -P)"; cargo_cache="${CARGO_HOME:-$HOME/.cargo}"; build_flags="${RUSTFLAGS:-} --remap-path-prefix=${root}=/workspace --remap-path-prefix=${cargo_cache}=/cargo"; CARGO_TARGET_DIR=target RUSTFLAGS="${build_flags# }" cargo build --release --target wasm32-unknown-unknown --locked --offline
    mkdir -p target/component
    wasm-tools component new target/wasm32-unknown-unknown/release/sigil_temporal.wasm -o target/component/temporal.wasm
    wasm-tools validate --features all target/component/temporal.wasm
    wasm-tools component targets wit --world sigil:temporal/plugin@0.1.0 target/component/temporal.wasm
    python3 scripts/check-contracts.py --component target/component/temporal.wasm

# Executes the actual component, using only an in-process host and public fixtures.
component-check: build
    cargo run --manifest-path tools/component-conformance/Cargo.toml --locked --offline -- target/component/temporal.wasm

# Explicit binary argument: the released 0.34.0 binary is NOT sufficient.
local-pack sigil_binary output_dir: build
    python3 scripts/pack.py plugin.local.toml {{quote(output_dir)}} --sigil {{quote(sigil_binary)}}

# Full local qualification includes the real Sigil validator, not just Wasmtime.
# Still NON-GATING: no official provenance or CAPI project-execution authority.
qualify-local sigil_binary output_dir:
    SIGIL_SOURCE_BINARY={{quote(sigil_binary)}} just check
    just component-check
    python3 scripts/pack.py plugin.local.toml {{quote(output_dir)}} --sigil {{quote(sigil_binary)}}

# Candidate creation only. The independent exact-tuple approval and main-only
# publication workflow are separate, and never rebuild the approved archive.
release-dist source_commit sigil_binary:
    SIGIL_RELEASE_BINARY={{quote(sigil_binary)}} just check
    just component-check
    python3 scripts/release-pack.py dist --sigil {{quote(sigil_binary)}} --source-commit {{quote(source_commit)}}
