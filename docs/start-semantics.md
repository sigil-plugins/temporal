# Interpreting Start results

Temporal 0.1.0 preserves the upstream `started` boolean. It does **not** expose
whether the server assigned that field, and `effect = "applied"` is not proof
that this invocation created a new execution. Do not use their combination as
a duplicate-start detector.

## What the pinned contract establishes

The source of truth is Temporal API v1.63.5, commit
`3ebdff42a9f07ac484b415fe8ff0b483b4ce3340`, recorded in
[vendor provenance](../vendor/provenance.json). Its
[StartWorkflowExecutionResponse](../vendor/temporal-api/temporal/api/workflowservice/v1/request_response.proto)
declares `bool started = 3`, without `optional`. The upstream comment says true
means a new workflow was started; `run_id` may identify a started **or used**
execution. `status` is execution status, not creation proof.

Under protobuf's [implicit-presence rules](https://protobuf.dev/programming-guides/field_presence/),
an unset scalar and a scalar assigned its default encode identically. A normal
proto3 encoder omits false. Inspecting the raw wire could distinguish an
explicitly encoded false from an omitted field, but could not distinguish a
server that deliberately assigned false from one that never assigned it.
Changing the WIT to `option<bool>` would therefore not recover server intent.

The [plugin implementation](../src/client.rs) directly forwards the decoded
boolean. It classifies every successfully decoded Start response with a
nonempty run ID as `effect = "applied"`, including the existing-execution
fixture. It does not turn `started = false` into an error or make another call.

| Result | Safe interpretation |
| --- | --- |
| Success, `started = true` | The server's response reports a new workflow. |
| Success, `started = false` | The decoded boolean is false; it does not distinguish a deliberate false from a server that omitted the field. |
| Success, `effect = "applied"` | Successful Start outcome under this plugin's classification, possibly an existing execution; not independent creation proof. |
| Error, `effect = "unknown"` | Whether the mutation happened is unresolved. Do not automatically retry Start. |

These are response semantics, not an assertion that every Temporal server
version implements the field identically. In particular, these fixtures do not
establish which older servers populate `started`, nor adjudicate a particular
live CAPI Start as new or duplicate.

## Caller guidance and the CAPI guard

The guard reported in findings #7:

```lua
if response.started == false and response.effect ~= "applied" then
  -- reject duplicate
end
```

cannot reject **any successful Start response** in 0.1.0. Success always carries
`effect = "applied"`, including the false and existing-execution cases. This
guard is not a weaker duplicate check; it is ineffective on the success path.
Handle typed errors separately and retain the no-retry rule for unknown effects.

If the assertion only needs a usable workflow execution, accept the successful
result and observe its state through Describe/History. If it specifically needs
proof of creation by this invocation, establish that as a separate contract:
qualify the deployed server's `started` behavior, retain unique workflow and
request identities, and correlate server evidence for the returned run ID.
The plugin's current Describe/History operations select by workflow ID, not
run ID, so their success alone does not prove the returned run was newly
created. Never reissue Start merely to resolve this question. Until suitable
evidence exists, keep a new-execution claim unproven rather than treating
`effect = "applied"` as the missing evidence.

## Regression evidence and scope

[Response fixtures](../conformance/responses/README.md) now cover omitted,
textproto-assigned false, wire-explicit false, and true. Pinned libprotoc 35.1
encodes the official-schema examples; a labelled proto2 fragment supplies the
legal wire-explicit false representation that normal proto3 encoding omits.
The fragment's field identity is checked against the pinned generated descriptor.
No plugin encoder is used to produce these oracles. This is independent codec
evidence, not independent discovery of schema tags or live server acceptance.

Both binding and client regressions verify the boolean projection; client
regressions additionally check `effect = applied`, exactly one exchange, and
the ineffective CAPI guard. Existing error/effect regression tests remain in
place. No WIT, production behavior, host grant, or compatibility range changes
are made by this adjudication. A new creation-evidence enum would require a
separate versioned design with an explicit unknown state, not a fabricated
presence bit on this implicit-presence field.
