-- Project-side compatibility helper, not an extra component export.
-- The calling scenario must declare wasm.temporal. Its monotonic scenario
-- deadline still applies; 180 attempts is not a wall-clock guarantee.
local temporal = require("wasm.temporal")

local M = {}

-- Use only after a successful start of this same workflow ID. Direct describe
-- callers must not reinterpret NOT_FOUND as a retryable result.
function M.wait_after_start(request)
  for attempt = 1, 180 do
    local value, err = temporal["describe-workflow-execution"](request)
    if err ~= nil then
      if err.kind ~= "server-status" or err.server == nil
          or err.server.code ~= 5 or err.server.class ~= "not-found" then
        return nil, err
      end
    elseif value.status.number ~= 1 then -- WORKFLOW_EXECUTION_STATUS_RUNNING
      return value, nil
    end

    if attempt < 180 then
      sigil.sleep(1)
    end
  end
  return { tag = "poll-timeout", attempts = 180 }, nil
end

-- The caller supplies its request ID once, before this function. Never retry
-- start, regenerate that ID, or convert an error into a failed expectation.
function M.run(request)
  local started, err = temporal["start-workflow-execution"](request)
  if err ~= nil then
    return nil, err
  end
  if started == nil then
    error("Temporal start returned no result")
  end
  return M.wait_after_start({
    profile = request.profile,
    namespace = request.namespace,
    ["workflow-id"] = request["workflow-id"],
    ["timeout-millis"] = 10000,
  })
end

return M
