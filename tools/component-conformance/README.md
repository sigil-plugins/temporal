# Actual-component functional conformance

This isolated, locked Rust workspace runs the **supplied compiled component**
with Wasmtime 48.0.0. It binds the repository's exact
`sigil:temporal/plugin@0.1.0` world, invokes all three typed exports, and implements
only `sigil:host/grpc-unary@1.3.0` in process. It installs no WASI, networking,
credentials, service, or other host capability. Its generated bindings perform
canonical lowering/lifting and post-return cleanup; all calls reuse one instance.

From the repository root, first verify the fixture inventory:

```sh
scripts/rebuild-protobuf.sh --verify
CARGO_BUILD_JOBS=2 cargo check --manifest-path tools/component-conformance/Cargo.toml --locked --offline
CARGO_BUILD_JOBS=2 cargo clippy --manifest-path tools/component-conformance/Cargo.toml --all-targets --locked --offline -- -D warnings
cargo fmt --manifest-path tools/component-conformance/Cargo.toml -- --check
```

Then supply an actual built component (for example, the artifact produced by the
root `just build` recipe after the client is implemented):

```sh
CARGO_BUILD_JOBS=2 cargo run --manifest-path tools/component-conformance/Cargo.toml --locked --offline -- target/component/temporal.wasm
```

The argument is resolved relative to the caller's current directory. Missing
arguments/files, a core module instead of a component, incompatible WIT/imports,
traps, unexpected host calls, and assertion failures return nonzero. Compile,
lint, or helper-test success is **not** a component-conformance PASS. The final
PASS line appears only after all 12 actual component export calls have succeeded
and been checked. Record the tested artifact's SHA-256 alongside the output; the
path printed by the harness is a locator, not an immutable identity.

The store has a cumulative 100,000,000-unit fuel budget (including
instantiation), and each linear memory is limited to 64 MiB. Final output reports
fuel consumed and both configured limits. These bounds catch accidental loops or
growth; they are not a claim of hostile-artifact safety or total-process/RSS limits.
The ordinary local compiler/engine still runs outside Wasm fuel accounting.

Each case queues exactly one public synthetic response and checks exactly one
recorded host call: frozen profile, RPC **alias** (`start`, `describe`, `history`),
timeout, 4 MiB response limit, and exact protobuf request bytes. The real Sigil
host, not the component, maps these aliases to canonical RPC paths.

Coverage:

- Start success, already-existing result, and future numeric status; exact
  independent Start fixture proves fixed request defaults, payloads and identity.
- Describe running/completed/future status; exact independent request proves the
  workflow-ID-only latest-run selector and nested response execution mapping.
- Close-event typed completion with ordered payloads, sorted metadata, non-UTF-8
  bytes, integers above 2^53, maximum task ID, and exact seconds/nanoseconds.
- Scheduled activities in order, no implicit pagination, and the exact binary
  response token passed into the next explicit call; the next page retains a
  future event number with no label, negative IDs, i64::MIN and nanoseconds.
- Typed failure/cause chain and exact unchanged all-events request fixture.
- Normal server status 6 on Start preserves unknown mutation effect; status 5 on
  Describe preserves read-only effect. Numeric status, class, known name, message,
  and binary details cross the canonical ABI unchanged.

All four original request `.pb` files are compared unchanged in at least one
case. Response bytes come from the checked-in independent protoc corpus. The
empty/binary-token request variants are explicitly **derived** oracles: the helper
substitutes the unique length-prefixed public token in the independent all-events
fixture (or removes its field for empty bytes), preserving every other byte. It
uses neither the plugin encoder nor handwritten field numbers. These two variants
are not separately protoc-generated fixtures. Helper tests check that substituting
the original token reproduces the original bytes and exercise the two variants.

Dependencies are pinned and locked separately; ordinary root builds do not build
this tool. Cached dependencies and the existing Rust toolchain are required for
`--offline`; nothing is downloaded by these commands. The local `target/` is
ignored. Run the same commands natively on Darwin for separate platform evidence.

This is benign functional integration coverage, **not** a security review, real
transport test, native CAPI replacement acceptance, provenance/signing gate, or
authorization to publish. It uses only public schema fixtures, never raw captures,
real endpoints, secrets, or adversarial inputs.
