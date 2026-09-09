# Pinned public schemas

`temporal-api/` contains the complete file import closure of WorkflowService
from Temporal API v1.63.5, commit `3ebdff42a9f07ac484b415fe8ff0b483b4ce3340`,
tree `59f2d60177929c6930c10e1d952067bdeb1c639d`. Sources were acquired from
the official repository and copied without modification after checking both
Git identities. SHA256SUMS inventories every proto and license file.

The snapshot omits imported `google/protobuf/field_mask.proto`. That single
file is supplied by official protobuf v35.1, matches the installed protoc35.1
include byte-for-byte, and is independently inventoried. All include inputs
are vendored; regeneration rejects a descriptor whose source is absent here.

Temporal's MIT license, protobuf's license and Google's Apache2 license are
under `licenses/`. Older vendored protobuf files retain their complete BSD
notices in their headers. All original source notices are preserved.

The source files contain more declarations than the component uses. Codegen
retains only the six Start/Describe/History request/response roots and complete
message/enum types transitively referenced by those roots. All services are
removed from the generation descriptor. No namespace/admin/health clients,
transport code, reflection, or runtime descriptors are generated. Upstream
response fields can themselves refer to other response messages; these remain
ordinary protobuf data types, never callable service methods.
