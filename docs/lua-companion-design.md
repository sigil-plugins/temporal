# Bounded Temporal Lua companion

Status: proposed implementation contract for bn-31f0; requires lead approval
before implementation. This document does not add an API to the released WASM.

## Decision and scope

Add `examples/lib/temporal.lua`, copied by projects into their scenario `lib/`
and loaded as `require("lib.temporal")`. It composes the existing three
`wasm.temporal` exports. The calling scenario still declares `wasm.temporal`
and owns its grants and monotonic `budget.max_seconds`. No WIT, host capability,
grant expansion, networking, credentials, shell, or plugin release change is
needed to implement this helper.

Reuse the algorithm and names `wait_after_start` and `run` from
`examples/lib/temporal_poll.lua`, rather than adding another polling abstraction.
Add only bounded `history`, terminal `result`, and explicit `decode_json`.
Leave the old module and its 25 compatibility tests unchanged: its returned
`poll-timeout` sentinel must not silently become an error for existing callers.
New companion callers opt into the namespaced error contract below. Port the
existing polling cases into the new module's tests as well.

History pagination is explicit project-side composition, not a hidden loop or
new export in the component. `result` does not start, describe, or poll a
workflow: waiting and result retrieval remain separate caller decisions.

## API and data shapes

The public functions are exactly:

```lua
value, err = temporal.wait_after_start(describe_request, wait_options)
value, err = temporal.run(start_request, wait_options)
value, err = temporal.history(selector, history_options)
value, err = temporal.result(selector, result_options)
value, err = temporal.decode_json(payload, json_options)
```

`start_request` and `describe_request` retain their existing WIT-shaped fields,
including hyphenated keys. Requests are not mutated. `selector` contains only
`profile`, `namespace`, and `["workflow-id"]`. There is no `run-id` selection.
Reject unknown selector/option keys and invalid values before any RPC.

Options are required tables, except `json_options`, which may be omitted.
Numeric options are finite Lua integers in the inclusive ranges below. Missing
required fields, unknown fields, and out-of-range values produce
`invalid-option`, not clamping or guessed defaults.

| Options | Required fields | Optional fields |
| --- | --- | --- |
| `wait_options` | `max_attempts` (1..180), `interval_millis` (0..60000) | `checkpoint`; `describe_timeout_millis` (1..10000, default 10000, used by `run` only) |
| `history_options` | `filter` (`close-event` or `all-events`), `max_pages` (1..128), `max_events` (0..16384), `max_bytes` (0..16777216), `timeout_millis` | `checkpoint` |
| `result_options` | Same limits as History, without `filter`; uses `close-event` | `checkpoint` |
| `json_options` | None | `max_bytes` (0..2097152, default 2097152) |

`timeout_millis` is 1..65000 for close-event and 1..10000 for all-events.
`wait_after_start` uses its request's `["timeout-millis"]`; reject a supplied
`describe_timeout_millis` option there instead of silently ignoring it.
`run` creates the Describe request from the Start profile/namespace/workflow ID
and the optional Describe timeout. It passes the Start request through exactly,
including the caller's one request ID. Validate all helper options before Start.

`checkpoint`, when supplied, is a zero-argument caller function. Normal return
means continue; cancellation or an earlier caller-owned deadline is signalled
by throwing. It must not perform RPCs or sleep. No `now`, wall-clock arithmetic,
deadline timestamp, or fake cancellation primitive is supplied by the helper.

Successful results:

- `wait_after_start` and `run`: the unmodified Describe response when its exact
  status number is anything other than RUNNING (1), including unknown numbers.
  Neither function labels that outcome successful workflow completion.
- `history`: `{ events = <ordered raw event list>, pages = N, bytes = N }`,
  only after a final empty next-page token. No truncated success is returned.
- `result`: `{ tag = "completed", payloads = <ordered raw payload list>,
  event = <raw completion event>, history = <history result> }`, or
  `{ tag = "not-completed", history = <history result> }`. An empty payload
  list under `completed` means completed with zero payloads; it is not absent
  completion. `not-completed` means no completed event was found in the bounded,
  fully traversed close-event response, not a synthesized workflow status.
- `decode_json`: the single decoded value, which may be `false`, an integer,
  a string, a structured value, or `sigil.json.null`. Never test success by
  truthiness: inspect `err`; JSON null is not Lua nil.

Returned plugin errors are passed through as `nil, err`, preserving the same
table and every kind/code/operation/effect/server field. Companion failures use
a distinct table: `{ kind = "companion", code = CODE, operation = NAME,
attempts = N, pages = N }`; counters apply only where relevant. Codes are
`invalid-option`, `poll-exhausted`, `page-exhausted`, `event-exhausted`,
`byte-exhausted`, `node-exhausted`, `repeated-token`, `malformed-result`, and
`unsupported-encoding`. Do not attach fake `server`, `status`, or `effect`
fields. Do not include identifiers, payloads, tokens, or error-source text.
No helper failure returns partial success data.

## Polling and deadline/cancellation precedence

`run` sends Start exactly once. Any returned error stops immediately, regardless
of effect classification; it never retries, regenerates a request ID, or calls
Describe after an unsuccessful Start. A thrown host error escapes unchanged.
On successful Start, call the same internal loop as `wait_after_start`.
Direct `wait_after_start` callers explicitly attest that this same workflow ID
has just had a successful Start; ordinary Describe callers should use the raw
plugin rather than reinterpret NOT_FOUND as retryable.

Only completed Describe responses with status RUNNING (1), or typed post-start
NOT_FOUND (`kind == "server-status"`, `server.code == 5`,
`server.class == "not-found"`) continue the loop. All other returned errors
propagate, and all other status numbers return unchanged. There are at most
`max_attempts` Describe calls and `max_attempts - 1` sleeps, each of exactly
`interval_millis / 1000` seconds. Never sleep after the final allowed attempt.
An unfinished last attempt yields `poll-exhausted`, not Temporal TIMED_OUT (4),
server DEADLINE_EXCEEDED, failed expectation, or completed workflow data.

No `pcall` or `xpcall` surrounds RPCs, `sigil.sleep`, JSON decode, or callbacks.
Apply boundary ordering consistently:

1. Call `checkpoint` before each RPC and sleep; any exception stops execution.
2. An exception from RPC/sleep escapes immediately, unchanged.
3. A returned non-retryable plugin error propagates unchanged before invoking
   another callback or manufacturing a local exhaustion error. This includes
   infrastructure, cancellation, and deadline errors.
4. After a successful call, retryable post-start NOT_FOUND, or sleep, invoke
   `checkpoint` before considering a result or local limit. Thus an observed
   caller cancellation/deadline takes precedence over local exhaustion/success.
5. Enforce local limits and then process the successful result. No subsequent
   RPC occurs after exhaustion or cancellation.

The host's sticky failure state and scenario deadline remain authoritative even
if a caller catches an error outside this module. An attempt bound is not a
wall-clock promise: 180 attempts does not mean 180 seconds. The helper cannot
interrupt an in-flight host RPC or detect a deadline during a purely local step
ahead of the host's own checks. Do not advertise independent timeout machinery.

## Canonical History traversal and resource accounting

Derive `["wait-new-event"]` and `["skip-archival"]` from `filter`: both true
for close-event, both false for all-events. These flags are not public options.
Begin with empty binary token `""`. Pass each returned token byte-for-byte to
the next call; do not UTF-8 decode, base64 decode, concatenate pages' tokens,
or auto-resume in a later helper invocation. Each page causes exactly one RPC;
server errors, including NOT_FOUND, are never retried by History.

Retain events in page/event order, preserving raw values. Track every nonempty
next token and reject any repeat, including A -> B -> A, before another RPC.
Empty pages with a fresh nonempty token continue within the same bounds. An
empty token completes traversal, including on the final allowed page. A
nonempty token on that page yields `page-exhausted`. Validate shape/node bounds
first, then check repeated-token and event/byte limits before page exhaustion,
in that order, so simultaneous local
failures have a deterministic code. Returned plugin errors/host exceptions
still have the higher precedence described above.

Aggregate `max_bytes` counts the length of every string-valued field occurrence
in each returned page, including metadata keys/values, payload data, labels,
failure text, activity names, and the returned token. Count repeated occurrences
even if Lua interns a string. Count incoming page strings once before retaining
that page; do not count keys of the fixed WIT record representation. `max_events`
counts events, not only completed events. On excess, discard the new page and
return no accumulated success. Limits are inclusive; zero allows only zero
counted events/bytes, and still consumes a page/RPC if requested.

Accounting must walk only the known WIT page/event/variant/payload/metadata/
failure-node shapes, never generic recursive traversal of arbitrary Lua data.
Before traversing a table, reject any metatable (including protected ones).
Use raw access, not metamethod-dispatched access or `pairs`/`ipairs` callbacks.
Records admit only their declared fields; lists must have only dense integer
keys 1..N, with no holes, foreign keys, or indices beyond their explicit bound.
Use an active-path identity set to reject cycles. Shared acyclic tables are
allowed, but count each occurrence: aliases must not bypass accounting.

Enforce a fixed aggregate ceiling of 100000 visited value occurrences per
helper invocation, counting each table/scalar value at the root or reached
through a declared record field/list entry. Stop before visiting occurrence
100001 with `node-exhausted`; enumeration itself must consume this finite
budget rather than materialize all keys first. Validate unknown record keys
within that record's fixed field-count bound and reject immediately. Enforce
at most eight nested tables (page root depth one), rejecting deeper/cyclic/
wrong-shaped data with `malformed-result`. These are closed hard limits, not
caller-widenable options. Existing per-shape bounds additionally apply: at most
4096 events per page, 32 metadata entries per payload, 16 failure nodes per
failure list, and 65536 bytes per page token. Every payload list is bounded by
the remaining node budget. The projected failure chain is already a flat WIT
list; do not follow invented recursive `cause` fields in Lua.

Apply the same bounded shape validation to caller-supplied `decode_json`
payloads and to response fixtures/mocks. Option/selector validation also uses
closed plain-table shapes. A malformed object must fail closed before its
metamethod can run or its contents can cause unbounded work. These validation
limits are helper failures, not a claim that the server sent malformed data.

These are exact logical retention limits, not an exact Lua heap-byte metric.
At most 128 pages, 16384 events, 100000 visited value occurrences and 16 MiB of
returned string occurrences may be accepted; token tracking reuses those
strings rather than copying them.
One additional response can exist transiently while being checked, bounded by
the plugin/host's existing per-response ceilings. Lua table overhead, metadata
entry counts and transient host conversion are additionally bounded by the
existing host memory/fuel limits, never described as covered by `max_bytes`.
Do not duplicate raw payload strings or create concatenated history dumps.

## Result extraction, bytes, and JSON

`result` performs a complete bounded close-event `history` traversal before
returning any payloads. A completion must have event type number 2 and
`details.tag == "workflow-completed"`; its variant value is the whole payload
list, never a single payload. Reject a completion/tag mismatch or more than one
known terminal event (even identical duplicates) as `malformed-result`; no
first-win or last-win policy. Known close-event numbers are 2, 3, 4, 21, 27 and
28 in the pinned schema; unknown numbers remain raw data and never imply
completion.
Preserve failure chains and other raw event details in the returned History.
An empty completed list is valid. Failed/timed-out/cancelled/terminated/
continued-as-new outcomes are not a completed result and must not trigger a
Start or implicit follow-up to another run.

WIT `list<u8>` is a binary Lua string at this host boundary. `payload.data` is
the original bytes, not inherently UTF-8, JSON, base64, or a decoded object.
Metadata values are also byte strings. Preserve payload ordering, duplicates,
zero-length data, NUL/non-UTF-8 bytes, metadata order and exact numeric/time
fields. Helpers never modify returned payload/event tables; reference sharing
with their enclosing History is documented, not presented as a deep copy.

`decode_json` is a separate opt-in call for one payload. Require exactly one
metadata entry named `encoding`, with bytes exactly `json/plain`; reject missing,
duplicate or different encodings with `unsupported-encoding`. Validate the
payload/metadata shape before decode; malformed shape yields `malformed-result`.
Allow other metadata without changing or interpreting it. Call
`sigil.json.decode(payload.data, { max_bytes = limit })` exactly once and return
that value. Its exceptions propagate unchanged; there is no retry, fallback,
base64 handling, automatic second decode, or conversion to a workflow outcome.

Sigil's decoder preserves signed 64-bit integer literals, including values above
2^53, and rejects fractional/exponent forms and out-of-range integers. It also
rejects invalid UTF-8, duplicate object keys, malformed/trailing JSON and its
own depth/node/memory limits. Null remains `sigil.json.null`; object/array shape
is retained by `sigil.data` values. A JSON string containing JSON-looking text
remains a string. Callers that deliberately want another layer must explicitly
decode again outside this helper. Raw payload data remains accessible unchanged
after a successful or failed decode. The 2 MiB decode ceiling may be lower than
a valid raw payload: raw transport success does not promise decoder acceptance.

## Identity and observability limits

Keep every individual host operation visible in the normal Sigil trace. The
helper neither logs payloads nor substitutes one synthetic operation for its
RPCs. Attempt/page counts are local accounting, not evidence of server receipt.
No helper catches sticky host failures or turns them into PASS/FAIL assertions.

`started` is the upstream implicit-presence bool. Every successful Start
currently carries `effect = "applied"`, including an existing-execution result.
The CAPI guard `started == false and effect ~= "applied"` rejects no successful
Start. Neither value is independent creation proof. Describe/History select
the latest run by workflow ID and History returns no run ID. The companion
does not invent run-ID pinning, creation evidence, or a consistency guarantee
across calls. Record/correlate server evidence separately if the scenario's
contract requires a newly created, exact execution.

## Implementation acceptance matrix (bn-31f0)

Use offline fake exports with exact call recording, then a real Sigil scenario
for boundary/JSON behavior. Keep evidence categories separate from live CAPI
acceptance; no paid service or external Temporal mutation is required here.

| Area | Required regressions |
| --- | --- |
| Legacy compatibility | Existing 25 temporal_poll tests unchanged; original module bytes unchanged. |
| Poll | Reuse all existing status/error cases; max 1 and 180 attempts; exact sleep count; invalid options cause zero Start/Describe calls; one Start with unchanged request ID; unknown-effect Start never retries/describes. |
| Exhaustion | Distinct namespaced errors, no workflow/server status/effect fields, no partial result; terminal status on final attempt wins over unused attempt bound. |
| Cancellation/deadline | Checkpoint before first RPC, between attempts/pages, after successful final response and before local exhaustion; sleep/RPC/checkpoint exceptions escape by identity; plugin infrastructure/server deadline errors preserve identity and precedence. |
| History | Both canonical flag combinations; exact timeout lowering; 1 and 128 pages; empty terminal page; empty intermediate page; exact binary token; immediate repeat and A/B/A; final-page success vs page exhaustion; never retry any History error. |
| Retention | Inclusive event/byte/node boundaries and zero limits; every WIT string field counted; multiple metadata/payload entries; one over returns no partial result; raw data is not copied or mutated; cycles/metatables/sparse lists/foreign keys/oversized indices/deep mock records fail closed without callbacks; shared acyclic aliases count on every occurrence; flat failure list max16 preserved. |
| Result | Completion with zero/one/multiple payloads; completion only on later page; no completion; each known non-completed terminal; unknown event numbers; multiple/mismatched/conflicting completion data; limits/errors after an earlier completion still prevent success. |
| JSON | Explicit json/plain only; metadata anomalies; raw bytes unchanged; object/array/null/false/integer/string roots; empty and binary data rejection; signed-i64 extrema and above-2^53 exactness; fractions/exponents/overflow rejected; escaped JSON string decoded once; duplicate keys/trailing JSON rejected; lowered input limit and host allocation/deadline failure propagate. |
| Isolation | No pcall/xpcall, shell, logging, grants, automatic Start retry or new WASM exports; real Sigil trace exposes each export and explicit decode without payload disclosure. |

Implementation must update the example caller to show separately checking
`err`, then terminal status/result tag, then optional payload decoding. Document
that `not-completed` and helper exhaustion cannot be silently treated as a
product PASS/FAIL by the adjudicator. Run existing `just check`, added Lua tests,
and targeted real-host boundary tests before requesting review. No new plugin
release or CAPI prerequisite gate is implied by this design approval.
