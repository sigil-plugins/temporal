# Operator and project-side examples

## Operator grant template

[`operator-grant.toml`](operator-grant.toml) is a complete parseable configuration
template for stable Temporal 0.1.0 on Sigil 0.35.x. It grants only the official
source semantic gRPC authority. The network table supplies host routing, not
guest networking. No third-party allowance, raw guest secret, endpoint binding,
credential, scenario or generated lock is included. Reserved `.invalid` DNS
names and `replace-me` values must not be used for a real service.
Every unused capability allowlist is explicitly empty: omitted lists inherit
official-source defaults. These lists apply project-wide; separately review
and add any authority needed by other plugins instead of deleting empty lists.

1. Review the template before merging it into your project configuration. Replace
   the service, logical target, TLS authority/server name, namespace, workflow
   prefix, workflow type and task queue. Keep the metadata namespace equal to
   the request-policy namespace. Review the other four metadata values for your
   deployment. Do not change the fixed `sigil-temporal@0.1.0` request identity or
   the `start`, `describe`, `history` RPC aliases, paths and kinds.
2. Bind the logical target to your endpoint map. A remapped port needs explicit
   binding, such as `--plugin-route LOGICAL:7233=https://HOST:PUBLISHED_PORT`.
   Substitute the reviewed values before use. Socket address, exact HTTP/2
   authority, and TLS server name are separate inputs. TLS server name must
   match the authority's DNS host and the certificate. Use `tls_ca_file` for
   an operator-selected private CA when needed. The template uses direct TLS
   without authentication. For bearer authentication, set the profile's
   `bearer_secret` to a declared scenario environment variable name, never its
   value. Do not add raw `secrets` authority. Plaintext `h2c` is only suitable
   for a separately reviewed, isolated local service and cannot carry bearer
   credentials.
3. Follow the README's official install/lock sequence. A scenario must declare
   `wasm.temporal` and select `profile = "workflow"`. Give it an explicit
   scenario budget sufficient for the intended sequence. For example,
   `budget = { max_seconds = 120 }` can accommodate one 65-second history call,
   but does not promise that a whole polling loop will finish.

### Budget relationships

Start, Describe and all-events history accept at most **10000 ms** per call.
Close-event history accepts **65000 ms**. Use `wait-new-event=true` and
`skip-archival=true` only with `filter="close-event"`; all-events requires both
flags false. Every request requires a positive `timeout-millis`.

The template sets profile `max_call_seconds=65`, outer runtime
`max_call_seconds=70`, and endpoint `io_timeout="70s"`. This avoids shorter
default ceilings truncating a close-event long poll. The five-second connect
limit only bounds connection establishment. The earliest profile, caller,
outer-call, scenario and I/O deadline still wins. No configured limit promises
that the server will answer within it.

The 1 MiB request and 4 MiB response limits bound individual protobuf messages.
The endpoint's 16 MiB allowance instead covers **cumulative bidirectional HTTP/2
plaintext bytes**, including framing, across calls in that scenario/lane.
It is not refreshed by each call and is not sized for 180 maximum-size replies.
Review this quota against your intended calls and pages. `max_connections=1`
limits concurrent connections, not the total number of sequential calls.

`max_memory="256MiB"` follows Sigil's measured 4 MiB response qualification.
It is not a universal sizing formula or a guarantee for every response shape.
Guest memory, transfer fuel, host allocations and retained outputs impose
independent limits. Runtime settings apply project-wide to plugins, so review
their impact before copying them into a multi-plugin configuration. See
[Sigil's semantic gRPC limits](https://runsigil.com/guides/semantic-grpc/#limits-and-outcomes).

The tests compare aliases, methods, mutation/read kinds, fixed identity and
metadata names with the frozen contract. With `SIGIL_RELEASE_BINARY` set, they
also parse this file using the hash-pinned public Sigil 0.35.0 executable and
enumerate its capability defaults from its generated schema, require every
capability list explicitly, and prove an invalid endpoint reference is rejected.
Negative controls demonstrate why an omitted unused allowlist is broader.
These offline checks do not
resolve a route, acquire a plugin, contact Temporal, or prove service acceptance.

## Payload bytes and JSON

WIT `payload.data` and metadata values are `list<u8>`. Sigil maps each to a
binary Lua string without base64 conversion, JSON decoding or UTF-8 conversion.
The plugin preserves the exact response bytes. Check each payload's `encoding`
metadata before interpreting it. Different payloads may use different encodings.
`workflow-completed` carries an ordered **list of payloads**, not one payload;
an empty list means completion without a return payload, not non-completion.

For an explicitly selected `json/plain` payload, one optional decode looks like:

```lua
local encoding
for _, entry in ipairs(payload.metadata) do
  if entry.key == "encoding" then encoding = entry.value end
end
if encoding ~= "json/plain" then
  error("expected a json/plain Temporal payload")
end
local value = sigil.json.decode(payload.data, { max_bytes = 65536 })
-- Read the application's expected shape with sigil.data or direct field access.
-- If JSON encoded a string, value is a string. Do not decode it a second time
-- unless a separate application field explicitly contains JSON text.
```

This capability-free helper returns Sigil's shape-preserving structured values
without a `jq` process. It is **not a general Temporal data converter**:
Sigil 0.35.0 rejects fractional JSON numbers, integers outside signed 64-bit
range, duplicate keys and malformed UTF-8/JSON. The example lowers the hard
2 MiB input ceiling to 64 KiB. Depth, node-count and allocation limits also
apply. Preserve the raw payload when you need an unsupported encoding or number
representation. Decode errors must remain errors, never an empty successful
workflow result. JSON `null` remains `sigil.data.null`, distinct from no payload.

## Project-side polling example

`lib/temporal_poll.lua` is a reference helper for a scenario library, not a fourth
plugin export or a drop-in replacement for the CAPI shell script. Copy it into
the caller's scenario `lib/` only as part of the reviewed migration. Each calling
scenario must declare `wasm.temporal` in its capabilities.

The caller constructs the two ordered `json/plain` payloads and one request ID
before calling `run`. Start executes once. Describe selects the latest run using
only the same workflow ID, with a 10-second per-call timeout. The helper retries
only RUNNING and post-start NOT_FOUND, at most 180 completed describes, sleeping
one second between retryable responses. It returns the exact terminal/unknown
status, an unchanged error, or `{tag="poll-timeout", attempts=180}`.

Poll exhaustion is not Temporal's TIMED_OUT status or proof of 180 elapsed
seconds. Runtime deadlines, exchange errors and sleep errors propagate. The
calling compatibility wrapper must classify poll exhaustion and non-NOT_FOUND
errors as BROKEN before product assertions; it must not convert them into an
expected product failure. The existing CAPI assertion/result ordering must be
reviewed against actual call sites when replacing the helper.

History/result/activity/failure projection remains explicit caller code using
the three plugin exports; this example does not fetch history pages or decode
payload data. Other `jq` call sites still require `exec`.

Run the offline tests with:

```sh
lua tests/temporal_poll.lua examples/lib/temporal_poll.lua
```

These tests use in-process callbacks and are not CAPI acceptance or evidence of
a supporting stable Sigil/plugin installation.

## Bounded Lua companion

[`lib/temporal.lua`](lib/temporal.lua) is the new opt-in companion. It leaves
`lib/temporal_poll.lua` and its legacy sentinel behavior unchanged. Copy the
new file into the calling scenario's `lib/` directory. Every caller declares
`wasm.temporal`, including a caller using only `decode_json`: Sigil traces the
module's literal WASM dependency during capability checking. Lazy loading does
not bypass that check. The direct `sigil.json.decode` builtin example above,
in contrast, needs no plugin capability.

The companion reuses `run`/`wait_after_start` and adds bounded `history`,
`result`, and explicit `decode_json`. All requests preserve their WIT field
names; helper options use underscore names. An illustrative success-contract
caller, after constructing one Start request with its two ordered payloads:

```lua
local temporal = require("lib.temporal")
local workflow, err = temporal.run(start_request, {
  max_attempts = 180, interval_millis = 1000,
})
if err ~= nil then error("Temporal Start/wait infrastructure or helper error") end
expect(workflow.status.number == 2) -- measured workflow COMPLETED, not poll exhaustion
if workflow.status.number ~= 2 then return end -- do not mask a product failure with result retrieval

local result, result_error = temporal.result({
  profile = start_request.profile,
  namespace = start_request.namespace,
  ["workflow-id"] = start_request["workflow-id"],
}, {
  max_pages = 8, max_events = 32, max_bytes = 1048576, timeout_millis = 65000,
})
if result_error ~= nil then error("Temporal result retrieval error") end
if result.tag ~= "completed" then error("No completed result was observed") end
-- Empty payloads is valid completion without a return value, not missing completion.
for index = 1, #result.payloads do
  local value, decode_error = temporal.decode_json(result.payloads[index], { max_bytes = 65536 })
  if decode_error ~= nil then error("Temporal payload encoding/shape error") end
  -- Inspect the application's expected value; false and sigil.json.null are valid.
end
```

Set an explicit scenario `budget.max_seconds`. Attempt counts are not elapsed
seconds; host deadlines still win. `checkpoint`, if supplied, may throw for
caller cancellation/earlier deadlines but must not mutate requests or perform
RPCs/sleep. The helper snapshots bounded Start/selector/option tables before
callbacks without copying immutable payload byte strings. It never retries
Start, suppresses a host exception, or follows another run. Describe/History
still select by workflow ID, not the Start run ID.

Returned plugin errors preserve their exact table. Helper errors instead have
`kind="companion"` with codes such as `poll-exhausted` or `page-exhausted`,
never a fabricated workflow status/server status/effect. A scenario/adjudicator
must not turn exhaustion or `not-completed` into a successful result or an
expected product failure. Handle such gaps before product assertions, preserving
the project's established infrastructure-versus-product classification.

`history` derives the two flags from its required `filter`; `result` always
selects close-event History. Both require explicit page/event/byte/time limits,
stop on repeated binary tokens, and return no partial success. Ordered raw
payloads and metadata are preserved. JSON decoding is optional, exactly one
layer, and rejects unsupported encodings/numbers rather than guessing.
See [the complete contract](../docs/lua-companion-design.md) for inclusive
limits, precedence, raw-data ownership and all returned shapes.

`just check` runs the old 25 polling tests and the new recording/bounds tests.
For the real Sigil JSON/sandbox boundary, first prepare an isolated project via
normal official install/add/sync with Temporal 0.1.0 and no RPC routes. Then run:

```sh
just companion-host-check /absolute/path/to/pinned-sigil /absolute/path/to/project
```

This separate target verifies the Linux Sigil 0.35.0 binary against
`scripts/release-tools.json` before and after execution. It does not acquire a
plugin, add a grant, or modify the project config/lock. The project's already
synced plugin cache must be selected through its normal `SIGIL_DATA_DIR` and
`SIGIL_CACHE_DIR`. The test declares `wasm.temporal` and exercises the full
helper's decoder under ordinary `sigil run`, without an RPC. It covers exact
large integers, null/shape, single-layer strings, invalid JSON/UTF-8/numbers,
input limits and metatable rejection in the real restricted sandbox. RPC
sequencing/exception/limit tests remain offline recordings; these checks are
not live Temporal behavior, full CAPI acceptance, or publication authority.
