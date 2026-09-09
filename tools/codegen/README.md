# Explicit message-only code generation

This separate, locked developer crate pins prost-build/prost/prost-types to
0.14.4. It is excluded from the component workspace. There is no component
build script, generated gRPC service, socket library or Tokio dependency.

Run `scripts/rebuild-protobuf.sh --check` from the repository root after
installing protoc35.1 and fetching this tool's locked dependencies once.
Every subsequent regeneration uses Cargo `--locked --offline`. Use `--write`
to deliberately update checked-in output and response oracle hashes.
`--verify` requires neither this crate nor protoc and verifies exact file
inventories. Normal component compilation consumes `src/generated/` directly.
The inventory command uses portable `find` and accepts either GNU `sha256sum`
or Darwin's `shasum -a 256`; set `PROTOBUF_SHA256_TOOL=shasum` to explicitly
exercise the latter path on other hosts.

The generator asks protoc for the complete WorkflowService import descriptor,
then follows message/enum field dependencies from exactly six request/response
roots. It retains full reachable top-level messages (including their nested
types), removes services/extensions and stale source-info indices, and invokes
prost-build on that descriptor. Standard protobuf messages map to prost-types.
Maps use BTreeMap so request encoding is deterministic. Unknown enum numbers
remain i32. The component does not include the descriptor artifacts in its
runtime; they exist for reproducibility and review.

`wire.rs` is independently derived from the same pruned descriptors and
contains message names, field names/numbers, wire kinds, repeated markers and
map-entry identities. It permits the client to validate bounds and duplicate
map keys before allocating generated messages. These are schema identities,
not a second handwritten protobuf schema. Wire preflight/semantic acceptance
belongs to the client; prost alone accepts duplicate map keys by replacement.

`provenance.json` records the initial generator binary and compiler identities.
The binary hash is evidence for this local generation, not an expected portable
hash across platforms or builds. Portable reproduction requires the pinned
versions and identical vendored/generated/oracle hashes. Any release must
bind the exact generator artifact used for that release independently.
