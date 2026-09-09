# Temporal v0 request fixtures

These public-safe protobuf requests are positive conformance evidence for the
strict `temporal-workflow-v0` validator. They are independent of Sigil's
hand-written parser and canonical encoder: `protoc --encode` reads the checked-in
text-format values against Temporal's official schema and emits each `.pb` file.

The pinned schema identity is:

- repository: `https://github.com/temporalio/api.git`
- tag: `v1.63.5`
- commit: `3ebdff42a9f07ac484b415fe8ff0b483b4ce3340`
- Git tree: `59f2d60177929c6930c10e1d952067bdeb1c639d`
- primary schema SHA-256:
  `f303127eaea53bfd6a10a011ca1583443dcd7a26f6193b9a3ddb632742aef8a0`
- generator: `libprotoc 35.1`

The four `.pb` files and textproto sources are copied unchanged from Sigil's
`tests/fixtures/temporal-v0/requests` at approved source
`7403a479a36dc7a2fadf38c47578d64ba37ed679`.
`scripts/rebuild-protobuf.sh --check` verifies the vendored schema inventory,
regenerates all four requests offline in a temporary directory, and compares
them byte-for-byte. Even `--write` checks these imported request bytes rather
than replacing them. `--verify` checks inventories without code generation.

The four cases cover all three admitted RPCs and both admitted history shapes:

- start, including present empty execution/run timeout messages, a 10-second
  task timeout, an explicit normal task-queue enum, and a present empty header;
- describe, with the run ID omitted to select the latest run;
- close-event history, with both policy booleans true and no page token;
- all-events history, with both default-false booleans omitted and a present
  non-empty page token.

`SHA256SUMS` binds the checked-in protobuf byte corpus, source text and this
provenance note. Sigil separately verifies its original four protobuf digests
before passing the bytes to the production validator.
