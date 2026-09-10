-- Real Sigil decoder/sandbox boundary, no RPC or live Temporal acceptance.
local temporal = require("lib.temporal")
local function payload(data)
  return { data = data, metadata = {{ key = "encoding", value = "json/plain" }} }
end
return {
  title = "Temporal companion real JSON and plain-table boundary",
  priority = "P0",
  budget = { max_seconds = 10 },
  policy = { capabilities = { "wasm.temporal" } },
  run = function()
    local value, err = temporal.decode_json(payload('{"n":9007199254740993,"null":null,"a":[],"o":{}}'))
    expect(err == nil)
    expect(value.n == 9007199254740993)
    expect(value.null == sigil.json.null)
    expect(sigil.data.kind(value.a) == "sequence")
    expect(sigil.data.kind(value.o) == "mapping")
    local false_value, false_error = temporal.decode_json(payload("false"))
    expect(false_value == false and false_error == nil)
    expect(temporal.decode_json(payload("9223372036854775807")) == math.maxinteger)
    expect(temporal.decode_json(payload("-9223372036854775808")) == math.mininteger)
    expect(temporal.decode_json(payload('"{\\"nested\\":1}"')) == '{"nested":1}')
    for _, raw in ipairs({
      "0.5", "1e0", "9223372036854775808", "-9223372036854775809",
      '{"x":1,"x":2}', '{} trailing', "", "\255",
    }) do
      local original = payload(raw)
      local ok = pcall(temporal.decode_json, original)
      expect(not ok)
      expect(original.data == raw)
    end
    local small_ok = pcall(temporal.decode_json, payload("{}"), { max_bytes = 1 })
    expect(not small_ok)
    local at_limit = temporal.decode_json(payload("{}"), { max_bytes = 2 })
    expect(sigil.data.kind(at_limit) == "mapping")
    local bad = setmetatable(payload("{}"), {
      __index = function() error("should not run") end,
      __pairs = function() error("should not run") end,
      __metatable = false,
    })
    local denied, bad_error = temporal.decode_json(bad)
    expect(denied == nil)
    expect(bad_error.kind == "companion" and bad_error.code == "malformed-result")
    expect(getmetatable(bad) == false)
    local sparse = { data = "{}", metadata = { [2] = { key = "encoding", value = "json/plain" } } }
    local absent, shape_error = temporal.decode_json(sparse)
    expect(absent == nil and shape_error.code == "malformed-result")
    expect(rawget == nil and rawset == nil)
  end,
}
