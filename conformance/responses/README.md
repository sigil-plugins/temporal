# Independent response oracles

These fixtures are newly authored public-schema examples, not captured CAPI
responses and not claims of live server acceptance. Their shapes exercise the
measured Start/Describe/History projection. `protoc --encode` against the pinned
official schema produces every `.pb` directly from its checked-in textproto.
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

`scripts/rebuild-protobuf.sh --check` verifies hashes, regenerates message source
and all request/response oracles offline using protoc35.1/prost-build0.14.4,
and compares byte-for-byte. `--write` updates only generated source and response
oracles; the four inherited Sigil requests are always checked, never rewritten.
`--verify` checks exact inventories without invoking codegen or protoc.
