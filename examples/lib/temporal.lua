-- Project-side companion; no new component exports or authority.
-- RPC callers declare wasm.temporal and retain an explicit scenario deadline.
local M = {}
local exports
local function rpc(name, request)
  if exports == nil then exports = require("wasm.temporal") end
  return exports[name](request)
end

local function plain(value)
  return type(value) == "table" and getmetatable(value) == nil
end

local function failure(code, operation, attempts, pages)
  return nil, {
    kind = "companion", code = code, operation = operation,
    attempts = attempts, pages = pages,
  }
end

local function integer(low, high)
  return { kind = "integer", low = low, high = high }
end
local function string_value(low, high, pattern)
  return { kind = "string", low = low or 0, high = high or 16777216, pattern = pattern }
end
local function record(fields, optional)
  return { kind = "record", fields = fields, optional = optional or {} }
end
local function list(item, maximum)
  return { kind = "list", item = item, maximum = maximum }
end
local function state()
  return { nodes = 0, bytes = 0, active = {}, code = "malformed-result" }
end

-- Schema-directed traversal only. The sandbox intentionally removes rawget.
-- After rejecting ANY metatable, indexing cannot dispatch a metamethod.
-- next visits entries without pairs/ipairs callbacks or a temporary key list.
local walk
walk = function(value, schema, budget, depth)
  budget.nodes = budget.nodes + 1
  if budget.nodes > 100000 then
    budget.code = "node-exhausted"
    return false
  end
  if schema.kind == "string" then
    if type(value) ~= "string" or #value < schema.low or #value > schema.high
        or (schema.pattern and not string.match(value, schema.pattern)) then return false end
    budget.bytes = budget.bytes + #value
    return true
  elseif schema.kind == "integer" then
    return math.type(value) == "integer" and value >= schema.low and value <= schema.high
  elseif schema.kind == "boolean" or schema.kind == "function" then
    return type(value) == schema.kind
  end
  if not plain(value) or depth > 8 or budget.active[value] then return false end
  budget.active[value] = true
  if schema.kind == "variant" then
    local tag = value.tag
    if type(tag) ~= "string" or schema.cases[tag] == nil then return false end
    schema = schema.cases[tag]
  end
  if schema.kind == "list" then
    local count, largest = 0, 0
    for key, child in next, value do
      if math.type(key) ~= "integer" or key < 1 or key > schema.maximum then return false end
      count = count + 1
      if key > largest then largest = key end
      if not walk(child, schema.item, budget, depth + 1) then return false end
    end
    if count ~= largest then return false end
  else
    for key, child in next, value do
      local field = schema.fields[key]
      if field == nil or not walk(child, field, budget, depth + 1) then return false end
    end
    for key in next, schema.fields do
      if not schema.optional[key] and value[key] == nil then return false end
    end
  end
  budget.active[value] = nil
  return true
end

local text = string_value()
local id = string_value(1, 255)
local profile = string_value(1, 64, "^[a-z][a-z0-9_-]*$")
local i64 = integer(math.mininteger, math.maxinteger)
local proto_enum = record({ number = integer(-2147483648, 2147483647), label = text }, { label = true })
local payload = record({
  data = string_value(0, 2097152),
  metadata = list(record({ key = string_value(0, 8192), value = string_value(0, 8192) }), 32),
})
local failure_node = record({ message = text, source = text, ["activity-type"] = text },
  { source = true, ["activity-type"] = true })
local details = { kind = "variant", cases = {
  ["workflow-completed"] = record({ tag = text, value = list(payload, 100000) }),
  ["workflow-failed"] = record({ tag = text, value = list(failure_node, 16) }),
  ["activity-scheduled"] = record({ tag = text, value = text }),
  other = record({ tag = text }),
} }
local event = record({
  ["event-id"] = i64, ["task-id"] = i64,
  ["event-time"] = record({ seconds = i64, nanos = integer(0, 999999999) }),
  ["event-type"] = proto_enum, details = details,
})
local page_schema = record({
  events = list(event, 4096), ["next-page-token"] = string_value(0, 65536),
})
local selector_schema = record({ profile = profile, namespace = id, ["workflow-id"] = id })
local describe_request = record({
  profile = profile, namespace = id, ["workflow-id"] = id, ["timeout-millis"] = integer(1, 10000),
})
local start_request = record({
  profile = profile, namespace = id, ["workflow-id"] = id,
  ["workflow-type"] = id, ["task-queue"] = id,
  ["request-id"] = string_value(1, 64, "^[!-~]+$"),
  ["timeout-millis"] = integer(1, 10000), payloads = list(payload, 2),
})
local output_id = string_value(1, 4096)
local describe_response = record({ ["run-id"] = output_id, status = proto_enum })
local start_response = record({
  ["run-id"] = output_id, status = proto_enum, started = { kind = "boolean" }, effect = text,
})
local checkpoint_type = { kind = "function" }
local wait_fields = {
  max_attempts = integer(1, 180), interval_millis = integer(0, 60000), checkpoint = checkpoint_type,
}
local wait_schema = record(wait_fields, { checkpoint = true })
local run_schema = record({
  max_attempts = wait_fields.max_attempts, interval_millis = wait_fields.interval_millis,
  checkpoint = checkpoint_type, describe_timeout_millis = integer(1, 10000),
}, { checkpoint = true, describe_timeout_millis = true })
local history_fields = {
  filter = text, max_pages = integer(1, 128), max_events = integer(0, 16384),
  max_bytes = integer(0, 16777216), timeout_millis = integer(1, 65000),
  checkpoint = checkpoint_type,
}
local history_schema = record(history_fields, { checkpoint = true })
local result_schema = record({
  max_pages = history_fields.max_pages, max_events = history_fields.max_events,
  max_bytes = history_fields.max_bytes, timeout_millis = history_fields.timeout_millis,
  checkpoint = checkpoint_type,
}, { checkpoint = true })
local json_schema = record({ max_bytes = integer(0, 2097152) }, { max_bytes = true })

local function valid(value, schema, budget)
  return walk(value, schema, budget, 1)
end
local function snapshot(value, schema)
  local saved = {}
  for key in next, schema.fields do saved[key] = value[key] end
  return saved
end
local function checkpoint(options)
  if options.checkpoint then options.checkpoint() end
end
local function retryable(err)
  return plain(err) and err.kind == "server-status" and plain(err.server)
    and err.server.code == 5 and err.server.class == "not-found"
end

local function poll(request, options, operation, budget)
  for attempt = 1, options.max_attempts do
    checkpoint(options)
    local value, err = rpc("describe-workflow-execution", request)
    if err ~= nil and not retryable(err) then return nil, err end
    checkpoint(options)
    if err == nil then
      if not walk(value, describe_response, budget, 1) then
        return failure(budget.code, operation, attempt)
      end
      if value.status.number ~= 1 then return value, nil end
    end
    if attempt == options.max_attempts then
      return failure("poll-exhausted", operation, attempt)
    end
    checkpoint(options)
    sigil.sleep(options.interval_millis / 1000)
    checkpoint(options)
  end
end

function M.wait_after_start(request, options)
  local budget = state()
  if not valid(options, wait_schema, budget) or not valid(request, describe_request, budget) then
    return failure("invalid-option", "wait_after_start")
  end
  return poll(snapshot(request, describe_request), snapshot(options, wait_schema),
    "wait_after_start", budget)
end

function M.run(request, options)
  local budget = state()
  if not valid(options, run_schema, budget) or not valid(request, start_request, budget) or #request.payloads ~= 2 then
    return failure("invalid-option", "run")
  end
  options = snapshot(options, run_schema)
  -- Capture the bounded Start before calling user checkpoints. Scalar strings
  -- are immutable; copy only the two payload/metadata table shapes, not bytes.
  local saved = snapshot(request, start_request)
  saved.payloads = {}
  for index = 1, 2 do
    local input = request.payloads[index]
    local entries = {}
    for item = 1, #input.metadata do
      entries[item] = { key = input.metadata[item].key, value = input.metadata[item].value }
    end
    saved.payloads[index] = { data = input.data, metadata = entries }
  end
  request = saved
  local describe = {
    profile = request.profile, namespace = request.namespace,
    ["workflow-id"] = request["workflow-id"],
    ["timeout-millis"] = options.describe_timeout_millis or 10000,
  }
  checkpoint(options)
  local started, err = rpc("start-workflow-execution", request)
  if err ~= nil then return nil, err end
  checkpoint(options)
  if not valid(started, start_response, budget) or started.effect ~= "applied" then
    return failure(budget.code, "run")
  end
  return poll(describe, options, "run", budget)
end

local function traverse(selector, options, filter, operation, budget)
  -- Nodes include input validation; max_bytes counts returned page strings only.
  budget.bytes = 0
  local events, seen, token = {}, {}, ""
  for page_number = 1, options.max_pages do
    checkpoint(options)
    local page, err = rpc("get-workflow-execution-history", {
      profile = selector.profile, namespace = selector.namespace,
      ["workflow-id"] = selector["workflow-id"], filter = filter,
      ["wait-new-event"] = filter == "close-event", ["skip-archival"] = filter == "close-event",
      ["next-page-token"] = token, ["timeout-millis"] = options.timeout_millis,
    })
    if err ~= nil then return nil, err end
    checkpoint(options)
    if not walk(page, page_schema, budget, 1) then
      return failure(budget.code, operation, nil, page_number)
    end
    token = page["next-page-token"]
    if token ~= "" and seen[token] then
      return failure("repeated-token", operation, nil, page_number)
    end
    if #events + #page.events > options.max_events then
      return failure("event-exhausted", operation, nil, page_number)
    end
    if budget.bytes > options.max_bytes then
      return failure("byte-exhausted", operation, nil, page_number)
    end
    for index = 1, #page.events do events[#events + 1] = page.events[index] end
    if token == "" then
      return { events = events, pages = page_number, bytes = budget.bytes }, nil
    end
    seen[token] = true
    if page_number == options.max_pages then
      return failure("page-exhausted", operation, nil, page_number)
    end
  end
end

local function history_options(selector, options, is_result, budget)
  if not valid(selector, selector_schema, budget)
      or not valid(options, is_result and result_schema or history_schema, budget) then return false end
  local filter = is_result and "close-event" or options.filter
  return (filter == "close-event" or filter == "all-events")
    and (filter ~= "all-events" or options.timeout_millis <= 10000)
end

function M.history(selector, options)
  local budget = state()
  if not history_options(selector, options, false, budget) then
    return failure("invalid-option", "history")
  end
  return traverse(snapshot(selector, selector_schema), snapshot(options, history_schema),
    options.filter, "history", budget)
end

local terminal = { [2] = true, [3] = true, [4] = true, [21] = true, [27] = true, [28] = true }
function M.result(selector, options)
  local budget = state()
  if not history_options(selector, options, true, budget) then return failure("invalid-option", "result") end
  local history, err = traverse(snapshot(selector, selector_schema), snapshot(options, result_schema),
    "close-event", "result", budget)
  if err ~= nil then return nil, err end
  local completion, terminal_count = nil, 0
  for index = 1, #history.events do
    local item = history.events[index]
    local number, tag = item["event-type"].number, item.details.tag
    if (number == 2) ~= (tag == "workflow-completed") then
      return failure("malformed-result", "result", nil, history.pages)
    end
    if terminal[number] then terminal_count = terminal_count + 1 end
    if terminal_count > 1 then return failure("malformed-result", "result", nil, history.pages) end
    if number == 2 then completion = item end
  end
  if completion == nil then return { tag = "not-completed", history = history }, nil end
  return {
    tag = "completed", payloads = completion.details.value, event = completion, history = history,
  }, nil
end

function M.decode_json(value, options)
  if options == nil then options = {} end
  local budget = state()
  if not valid(options, json_schema, budget) then return failure("invalid-option", "decode_json") end
  if not walk(value, payload, budget, 1) then
    return failure(budget.code, "decode_json")
  end
  local encoding, count = nil, 0
  for index = 1, #value.metadata do
    local entry = value.metadata[index]
    if entry.key == "encoding" then encoding, count = entry.value, count + 1 end
  end
  if count ~= 1 or encoding ~= "json/plain" then
    return failure("unsupported-encoding", "decode_json")
  end
  return sigil.json.decode(value.data, { max_bytes = options.max_bytes or 2097152 }), nil
end

return M
