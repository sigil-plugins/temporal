# Independent response oracles

These fixtures are newly authored public-schema examples, not captured CAPI
responses and not claims of live server acceptance. Their shapes exercise the
measured Start/Describe/History projection. `protoc --encode` against the pinned
official schema produces the message fixtures from checked-in textproto,
with the two explicitly described wire-fragment compositions below.
The plugin encoder is never used as its own oracle.

The cases cover start/de-duplication and known/future status numbers, nested
describe identity, completion payload order, multiple metadata keys and binary
values, a workflow/activity failure cause chain, ordered scheduled activities,
caller-visible binary pagination tokens, empty pages, future event numbers,
signed int64 boundaries above double precision and nanosecond timestamp values.
Negative event/task IDs are synthetic representation tests, not claims that
Temporal normally allocates negative identifiers.

`history-future-fields.pb` concatenates the page-two oracle with the separately
protoc-encoded `FutureFields` message. Its synthetic high-numbered field does
not copy an official field tag. This tests unknown-field compatibility with
independent wire bytes and keeps the normal history fields unchanged.

`start-omitted.pb` and `start-false.pb` are identical outputs from the official
schema: proto3 implicit-presence encoding omits the default boolean even when
the textproto explicitly sets it. `start-success.pb` supplies the true case.
`start-wire-false.pb` appends a separately protoc-encoded `StartedPresence`
proto2 fragment to the omitted response, so field 3 is explicitly present with
value false. This test-only fragment copies the pinned official field identity
(checked against the generated descriptor in the unit test); it is independent
of the plugin encoder, not an independent source for Temporal's field numbers
or evidence of any live server's serialization. See
[Start semantics](../../docs/start-semantics.md) for the caller implications.

`scripts/rebuild-protobuf.sh --check` verifies hashes, regenerates message source
and all request/response oracles offline using protoc35.1/prost-build0.14.4,
and compares byte-for-byte. `--write` updates only generated source and response
oracles; the four inherited Sigil requests are always checked, never rewritten.
`--verify` checks exact inventories without invoking codegen or protoc.
