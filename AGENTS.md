# Temporal plugin development

Use isolated Maw workspaces. Keep the repository root on main.
Track this work in Sigil bone bn-1vg7 and its named children.
Workers never edit Bones, merge, close workspaces, push, tag or publish.
The lead owns review, integration, cleanup and external handoffs.

Use apply_patch for source edits, cargo add for dependencies, and just check
before commits. Keep existing changes outside assigned files intact.
The versioned WIT and conformance/contract.json are authoritative.
No raw networking, credentials, authority selection, WASI, Tokio, retry,
automatic pagination or host shell process is permitted in this component.
Only the three measured WorkflowService operations are exported.

Keep protobuf sources and generated bindings tied to exact upstream and tool
identities. Use independent protoc fixtures as encoder/decoder oracles.
Preserve exact payload bytes, numeric/time values, enum numbers and error/effect
classification. Host infrastructure faults remain sticky in Sigil.

Local checks are not official provenance, a stable Sigil release, or CAPI acceptance.
Do not weaken requirements or create an installable release without those gates.
