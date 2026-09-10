# Changelog

## 0.1.0-rc.1 — release candidate

- Bounded WebAssembly Temporal WorkflowService client for Start, Describe and
  caller-paginated History; one semantic host call per operation, no retries.
- Exact payload bytes, numeric enum identities and timestamp values, with
  independent pinned protobuf fixtures and actual-component conformance tests.
- Operator-frozen routing and credentials through Sigil Host API 1.3; requires
  supporting stable Sigil 0.35.x, not published Sigil 0.34.0.
- Main-only immutable keyless-provenance release pipeline, separate from
  explicitly non-gating local development packages.

This is not a claim of real CAPI caller replacement acceptance. That acceptance
must use the official locked RC and precedes stable Temporal promotion.
