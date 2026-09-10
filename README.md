# Sigil Temporal plugin

The measured three-operation `wasm.temporal` component:
start, describe and caller-paginated history.

This checkout prepares **0.1.1 (unreleased)**, requiring **Sigil >=0.35.0**
and exact Host API 1.3.0/schema 4. Published **0.1.0** still requires **Sigil
0.35.x**; its immutable manifest is not changed by this preparation. A version
in this checkout is not evidence that its GitHub release exists. The official
locked **0.1.0-rc.1** passed CAPI caller-replacement
acceptance on **2026-09-10**: five profiles, ten scenarios, 319 unchanged
assertions and both exact expected-RED fingerprints. Its 0.1.0 stable promotion
preserved the component bytes but created a new manifest/package identity
requiring its own reviewed publication and verification. Routing, authority, TLS policy,
credentials and transport limits belong
to the operator-frozen Sigil host profile. The component receives none of them.
It performs no retries, redirects, reconnections, sleeps or implicit pagination.

The candidate's minimum-only evaluator range removes routine minor-version
coupling, not host compatibility checks. Future stable versions must still
support the exact schema and host interface. An admissible version range is
not evidence that an unmeasured future host passed native or CAPI acceptance.
Prerelease evaluators retain Sigil's checked last-stable compatibility rules;
they do not impersonate their own final versions. Install examples below stay
on published 0.1.0 until a new package is officially published and verified.

The application WIT and machine contract are copied without semantic change from
reviewed Sigil source `7403a479a36dc7a2fadf38c47578d64ba37ed679`.
The host WIT must remain byte-identical to the canonical SDK contract.
Independent protobuf fixtures use the pinned Temporal API and protoc 35.1,
not the plugin encoder as their own oracle.

RC publication requires independent exact-candidate review, immutable
provenance-bearing artifacts and an actually supporting stable Sigil release.
Stable Temporal promotion additionally requires real CAPI replacement acceptance.
Existing CAPI assertions, exact expected-RED fingerprints and non-Temporal pins
must not change to obtain a pass.

## Implementation and checks

The component exports only StartWorkflowExecution, DescribeWorkflowExecution
and GetWorkflowExecutionHistory. Each performs at most one semantic host call.
The operator's host profile maps the fixed `start`, `describe`, and `history`
aliases to RPC paths; the guest never chooses a network address or credential.
Protobuf code and wire-field tables are generated offline from the pinned
Temporal schema. Request bytes are checked against independent protoc fixtures.

A borrowed preflight checks the fixed response-field ceilings before protobuf
decoding and preserves duplicate metadata evidence. The enclosing 4 MiB wire
ceiling does not bound every decoded allocation by that same size: unprojected
repeated messages and completion payload counts also rely on Sigil's Wasm
memory/fuel enforcement. No process-wide memory-erasure claim is made.
External payload-storage references and raw encoded history are unsupported;
the component must not present their absence from this WIT as empty success.

`just check` includes protobuf inventory verification, native client tests,
offline Lua polling tests, packaging checks, and the component harness's helper
tests. `just component-check` builds and executes the actual WASM component
through its canonical ABI against an in-process host. That host has no network,
WASI, credentials or services. These checks do not prove real Temporal server
compatibility, Sigil's scenario-level sticky-fault behavior, or CAPI acceptance.

The optional project-side polling helper and its caller obligations are in
[`examples/README.md`](examples/README.md). It is not an additional WIT export.

## Operator and caller contract

Start with the [complete operator grant template](examples/operator-grant.toml)
and its [setup and sizing notes](examples/README.md#operator-grant-template).
The template uses reserved, non-routable placeholders, no credential, and no
endpoint binding. It is not production authority or an installation command.

The Lua export names are `start-workflow-execution`,
`describe-workflow-execution`, and `get-workflow-execution-history`. The host
profile's RPC keys are a **different namespace**: the plugin sends exactly
`start`, `describe`, and `history`. Within an existing profile named `example`,
use these entries (this is a fragment, not a complete operator configuration):

```toml
[plugins.grants.temporal.grpc.example.rpcs]
start = { path = "/temporal.api.workflowservice.v1.WorkflowService/StartWorkflowExecution", kind = "mutation" }
describe = { path = "/temporal.api.workflowservice.v1.WorkflowService/DescribeWorkflowExecution", kind = "read" }
history = { path = "/temporal.api.workflowservice.v1.WorkflowService/GetWorkflowExecutionHistory", kind = "read" }
```

The existing profile's `request_policy` must use
`identity = "sigil-temporal@0.1.0"`. This is the plugin's fixed protobuf Start
identity, including for RC packages, not an operator-chosen client name or the
host-owned `client-version` metadata. Keep namespace, workflow-id prefix,
workflow type and task queue confined to the intended caller. The scenario's
`profile` must name that same configured profile.

Keep the network endpoint referenced by the gRPC profile: it supplies the
host-owned route, **not raw network authority for the guest**. Sigil 0.35.0 may
warn that this network grant is irrelevant even though the gRPC route is
retained. Do not remove the route or add raw `network`/`secrets` capability to
silence that warning; those capabilities cannot coexist with semantic
`grpc-unary`. Authorize only the official source for `grpc-unary`, without
widening installation or third-party policy.

History accepts exactly these request shapes; each call returns one page:

| `filter` | `wait-new-event` | `skip-archival` | Maximum `timeout-millis` |
| --- | --- | --- | --- |
| `close-event` | `true` | `true` | 65000 |
| `all-events` | `false` | `false` | 10000 |

Start and Describe also cap `timeout-millis` at 10000. Timeouts must be positive;
the host's operator, scenario and outer-call deadlines can shorten them further.
A 10000 ms timeout is admissible for all operations, but is not a guarantee of
that much execution time. Close-event history long-polls; reading a still-running
workflow may reach the deadline. Follow nonempty page tokens explicitly, keep
Start single-shot with one caller-owned request ID, and propagate infrastructure
errors rather than converting them into expected product failures.

`payload.data` contains the exact Temporal payload bytes. WIT declares
`list<u8>`; Sigil exposes that list as a **binary Lua string**, not base64 or an
already-decoded JSON value. See [payload decoding](examples/README.md#payload-bytes-and-json)
for metadata checks and an optional, single-layer `sigil.json.decode` example.

For Start responses, read [the `started` and mutation-effect semantics](docs/start-semantics.md)
before using either field as evidence of a newly created workflow execution.

## Local packaging (non-gating)

`plugin.local.toml` is an explicit **local development manifest**, not release
metadata. Its `=0.34.0` Sigil requirement matches the current source binary's
version string; **published stable Sigil 0.34.0 does not support Host API 1.3**.
Use a reviewed source build from Sigil
`99fae9f553ee3f2e139897bedf64a1a33f8f2d51` (runtime source
`d2130839530d7e11a26e5eba4720c52fa42d1776`, plus normative policy amendment)
and SDK
`3467153cc7c87979bd55db84c5a03f2fc77c7fe7`. Version text alone does not prove
that source identity. The packer records the actual binary SHA-256 and requires
successful schema-4/host-1.3 static validation, without claiming source provenance.
This source includes the Host API work from
`7403a479a36dc7a2fadf38c47578d64ba37ed679` plus separate bounded core-control
and recursive metadata limits. The earlier host source rejects this component's
ordinary generated protobuf dispatch; a harness PASS cannot override that.

Local tools are Python 3.11+, wasm-tools 1.252.0, zstd 1.5.7, b3sum, and the
pinned Rust toolchain. No check installs tools or accesses the network.

```sh
SIGIL_SOURCE_BINARY=/absolute/path/to/reviewed/sigil just check
just local-pack /absolute/path/to/reviewed/sigil target/local-candidate-1
```

For a complete local qualification, use
`just qualify-local /absolute/path/to/reviewed/sigil target/local-qualified-1`.
It runs every check (including package integration tests), builds and executes
the actual component, then validates and packages those bytes with real Sigil.
Any failure stops the recipe. In particular, a Wasmtime harness PASS does not
prove the artifact satisfies Sigil's separate structural limits. This command
still produces only non-gating local evidence, not CAPI acceptance.

`just check` covers frozen WIT/contract digests, exact semantic interface shapes,
packaging tests and the Rust checks. Without `SIGIL_SOURCE_BINARY`, the three
pack integration tests explicitly skip; they must pass before a packaging
change is accepted. Their generated dummy component proves packaging and type
compatibility only, never Temporal behavior. The actual component is checked
after `just build`; real-component integration remains required separately.

The packer accepts only explicit `0.1.0-dev.N` candidates and the sole semantic
host import/client export. It snapshots checked inputs, emits canonical POSIX
ustar plus checksummed zstd, and statically validates both unpacked inputs and
the finished archive with the supplied binary. Output must be a new directory:
existing candidates are never overwritten. It contains only the canonical
archive, `SHA256SUMS`, and `LOCAL-EVIDENCE.json` with input/binary hashes and
source references. The checkout identity identifies the packer, not proven
component source origin; package hashes identify the exact checked bytes.
No `release-manifest.json` or official attestation is made.
The nominated repository source is not verified provenance.

There is no installation, trust-policy widening or lock creation in this local
workflow. The local archive is **NON-GATING**: it cannot
authorize project execution or substitute for official provenance, a genuinely
supporting stable Sigil release, independent review or CAPI acceptance.

## Official candidate and publication workflow

`plugin.toml` is the separate release manifest. The client WIT and measured
request identity remain `0.1.0`; `0.1.1-rc.N` versions identify release candidates
without altering protobuf payloads or the interface contract.

1. Configure the public repository's protected `main`, main-only `release`
   environment, immutable releases and owner enforcement. These are external
   controls, not established by merely committing these workflow files.
2. Release Sigil 0.35.0 with public Linux assets. Independently measure and pin
   both the archive and extracted executable SHA-256 values in
   `scripts/release-tools.json`; either unfilled value fails closed.
   Acquisition uses only that public version and hash, no private Sigil checkout,
   token, floating latest installer or source-built version impersonation.
   Acquisition checks the executable pin before exposing it; every release
   validator invocation checks that pin immediately before and after execution.
   These checks preserve executable identity at invocation boundaries, not
   isolation against arbitrary concurrent same-user compromise.
3. Dispatch `prepare-release` once on the reviewed main commit. It installs
   pinned build tools, fetches locked dependencies, runs checks and actual
   component conformance, then validates the release package with Sigil 0.35.0.
   The candidate artifact contains exactly the canonical archive, `SHA256SUMS`
   and canonical `release-manifest.json`. That manifest alone is not provenance.
4. An independent reviewer approves the exact source commit, **first-attempt**
   successful candidate run, SemVer and all three asset SHA-256 values. The
   release agent freshly reads immutable-release controls before dispatching
   `publish-release` with that tuple. The workflow downloads those bytes; it
   does not rebuild. It verifies a draft readback, emits keyless GitHub OIDC
   provenance and checks the immutable public release and its asset hashes.
   Existing versions are burned rather than overwritten or republished.
5. Acquire the exact published version and verify its official lock. For an
   existing project lock with an empty cache, sync that lock **before** add:

   ```sh
   sigil plugin sync
   sigil plugin install temporal@0.1.0
   sigil plugin add temporal@0.1.0
   sigil plugin sync
   ```

   These commands require the stable release to exist. During RC acceptance,
   use the exact approved RC version instead. Omit the first sync when there
   is no existing lock; omit add when the lock already selects the intended
   version. Add resolves the whole project lock, so first populate dependencies
   already pinned there rather than assuming an empty cache contains them.

   Sigil 0.35.0's `add` grants project access and can acquire a missing package
   through the verified remote-install path. Explicit install-first is
   recommended here to expose acquisition evidence separately and keep the
   sequence compatible with older hosts. No `local:path` source, third-party
   allowance or `plugin test` may substitute for the official lock and ordinary
   `sigil run` acceptance path.

The published 0.1.0 RC remains prerelease and is not latest. This checkout's
pipeline admits only the new `0.1.1` and `0.1.1-rc.N` family (positive canonical
N). It rejects the historical 0.1.0 family rather than preparing replacements.
Any new publication requires its own reviewed source, candidate and package
identity. It never mutates an immutable release. See [RELEASING.md](RELEASING.md)
for evidence and failure handling.
