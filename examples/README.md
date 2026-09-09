# Project-side polling example

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
