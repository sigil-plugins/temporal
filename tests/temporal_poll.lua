-- Standalone, offline adapter tests. No Sigil, service, network or credentials.
local script = assert(arg[1], "usage: lua tests/temporal_poll.lua <adapter.lua>")
local cases = 0
local function check(name, test)
  test()
  cases = cases + 1
  print("PASS " .. name)
end

local function fixture(describe, start, sleep)
  local counts = { describe = 0, start = 0, sleep = 0 }
  package.loaded["wasm.temporal"] = {
    ["describe-workflow-execution"] = function(request)
      counts.describe = counts.describe + 1
      return describe(request, counts.describe)
    end,
    ["start-workflow-execution"] = function(request)
      counts.start = counts.start + 1
      return start(request)
    end,
  }
  _G.sigil = { sleep = function(seconds)
    assert(seconds == 1)
    counts.sleep = counts.sleep + 1
    if sleep then sleep() end
  end }
  return assert(loadfile(script))(), counts
end

local request = {
  profile = "workflow", namespace = "fixture", ["workflow-id"] = "fixture-1",
  ["request-id"] = "caller-owned-id", ["timeout-millis"] = 10000,
}
local running = { ["run-id"] = "run-1", status = { number = 1 } }
local completed = { ["run-id"] = "run-1", status = { number = 2 } }
local not_found = {
  kind = "server-status", server = { code = 5, class = "not-found" },
}

check("run starts once and selects latest run by workflow ID", function()
  local adapter, calls = fixture(function(r)
    assert(r.profile == request.profile and r.namespace == request.namespace)
    assert(r["workflow-id"] == request["workflow-id"])
    assert(r["run-id"] == nil and r["timeout-millis"] == 10000)
    return completed, nil
  end, function(r)
    assert(r == request and r["request-id"] == "caller-owned-id")
    return { ["run-id"] = "run-1" }, nil
  end)
  local value, err = adapter.run(request)
  assert(value == completed and err == nil)
  assert(calls.start == 1 and calls.describe == 1 and calls.sleep == 0)
end)

check("only completed RUNNING and post-start NOT_FOUND responses retry", function()
  local adapter, calls = fixture(function(_, count)
    if count == 1 then return nil, not_found end
    if count == 2 then return running, nil end
    return completed, nil
  end)
  assert(adapter.wait_after_start(request) == completed)
  assert(calls.describe == 3 and calls.sleep == 2)
end)

check("attempt exhaustion stays distinct from Temporal TIMED_OUT", function()
  local adapter, calls = fixture(function() return running, nil end)
  local value, err = adapter.wait_after_start(request)
  assert(value.tag == "poll-timeout" and value.attempts == 180 and err == nil)
  assert(value.status == nil and calls.describe == 180 and calls.sleep == 179)
end)

check("180 NOT_FOUND responses also exhaust the attempt budget", function()
  local adapter, calls = fixture(function() return nil, not_found end)
  assert(adapter.wait_after_start(request).tag == "poll-timeout")
  assert(calls.describe == 180 and calls.sleep == 179)
end)

for _, status in ipairs({ 0, 2, 3, 4, 5, 6, 7, 777 }) do
  check("terminal or unknown workflow status remains exact: " .. status, function()
    local expected = { status = { number = status } }
    local adapter, calls = fixture(function() return expected, nil end)
    assert(adapter.wait_after_start(request) == expected)
    assert(calls.describe == 1 and calls.sleep == 0)
  end)
end

for _, status in ipairs({ 1, 3, 4, 7, 8, 13, 14, 16, 99 }) do
  check("non-NOT_FOUND server failure propagates: " .. status, function()
    local expected = { kind = "server-status", server = { code = status, class = "unknown" } }
    local adapter, calls = fixture(function() return nil, expected end)
    local value, err = adapter.wait_after_start(request)
    assert(value == nil and err == expected and calls.describe == 1 and calls.sleep == 0)
  end)
end

check("infrastructure result does not become poll exhaustion", function()
  local expected = { kind = "infrastructure", code = "host-failure" }
  local adapter, calls = fixture(function() return nil, expected end)
  local value, err = adapter.wait_after_start(request)
  assert(value == nil and err == expected and calls.describe == 1 and calls.sleep == 0)
end)

check("host exception escapes without pcall or retry", function()
  local expected = {}
  local adapter, calls = fixture(function() error(expected) end)
  local ok, err = pcall(adapter.wait_after_start, request)
  assert(not ok and err == expected and calls.describe == 1 and calls.sleep == 0)
end)

check("sleep or monotonic scenario deadline escapes without retry", function()
  local expected = {}
  local adapter, calls = fixture(function() return running, nil end, nil,
    function() error(expected) end)
  local ok, err = pcall(adapter.wait_after_start, request)
  assert(not ok and err == expected and calls.describe == 1 and calls.sleep == 1)
end)

check("ambiguous start is never retried or followed by describe", function()
  local expected = { kind = "infrastructure", effect = "unknown" }
  local adapter, calls = fixture(function() error("unexpected describe") end,
    function() return nil, expected end)
  local value, err = adapter.run(request)
  assert(value == nil and err == expected and calls.start == 1 and calls.describe == 0)
end)

print("PASS " .. cases .. " offline polling cases")
