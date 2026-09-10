# Changelog

## 0.1.0 — stable

- Promote the accepted three-operation component without runtime, WIT or
  dependency changes; the stable manifest produces a new package identity.
- The official locked 0.1.0-rc.1 passed CAPI caller-replacement acceptance on
  2026-09-10: five profiles, ten scenarios, 319 unchanged assertions and both
  exact expected-RED fingerprints. This is evidence for the RC; stable
  publication and its exact artifact verification remain separate gates.
- Document the fixed Start identity, internal RPC aliases, per-filter History
  flags and timeout ceilings, and empty-cache acquisition with an existing lock.
- Requires Sigil >=0.35.0, <0.36.0 and Host API 1.3; no compatibility claim for
  Sigil 0.34.x or 0.36.x.

## 0.1.0-rc.1 — release candidate

- Bounded WebAssembly Temporal WorkflowService client for Start, Describe and
  caller-paginated History; one semantic host call per operation, no retries.
- Exact payload bytes, numeric enum identities and timestamp values, with
  independent pinned protobuf fixtures and actual-component conformance tests.
- Operator-frozen routing and credentials through Sigil Host API 1.3; requires
  supporting stable Sigil 0.35.x, not published Sigil 0.34.0.
- Main-only immutable keyless-provenance release pipeline, separate from
  explicitly non-gating local development packages.

At RC publication, real CAPI caller replacement acceptance remained open. The
subsequent accepted run is recorded above; it used the ordinary official lock.
