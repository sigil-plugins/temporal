-- Offline recording tests, not real-host/server acceptance.
local script = assert(arg[1])
local cases = 0
local function check(name, test)
  test(); cases = cases + 1; print("PASS " .. name)
end
local function copy(t)
  local out = {}; for k, v in next, t do out[k] = v end; return out
end
local selector = { profile = "workflow", namespace = "fixture", ["workflow-id"] = "fixture-1" }
local describe = copy(selector); describe["timeout-millis"] = 10000
local function payload(data, metadata)
  return { data = data or "", metadata = metadata or {{ key = "encoding", value = "json/plain" }} }
end
local start = copy(describe)
start["workflow-type"], start["task-queue"], start["request-id"] = "Fixture", "fixture", "one-id"
start.payloads = { payload("{}"), payload("{}") }
local started = { ["run-id"] = "run-1", status = { number = 1 }, started = true, effect = "applied" }
local running = { ["run-id"] = "run-1", status = { number = 1 } }
local completed = { ["run-id"] = "run-1", status = { number = 2 } }
local not_found = { kind = "server-status", server = { code = 5, class = "not-found" } }
local wait = { max_attempts = 180, interval_millis = 1000 }
local function opts(filter)
  return { filter = filter or "all-events", max_pages = 128, max_events = 16384,
    max_bytes = 16777216, timeout_millis = filter == "close-event" and 65000 or 10000 }
end
local function result_opts()
  local out = opts("close-event"); out.filter = nil; return out
end
local function event(n, details)
  return { ["event-id"] = 9007199254740993, ["task-id"] = math.mininteger,
    ["event-time"] = { seconds = 1700000000, nanos = 123456789 },
    ["event-type"] = { number = n }, details = details or { tag = "other" } }
end
local function page(events, token)
  return { events = events or {}, ["next-page-token"] = token or "" }
end
local function fixture(handlers)
  handlers = handlers or {}
  local counts = { start = 0, describe = 0, history = 0, sleep = 0, decode = 0, modules = 0 }
  local calls, exports = {}, {}
  for short, name in next, { start = "start-workflow-execution", describe = "describe-workflow-execution",
    history = "get-workflow-execution-history" } do
    exports[name] = function(request)
      counts[short] = counts[short] + 1; calls[#calls + 1] = { name = short, request = request }
      return assert(handlers[short], "unexpected " .. short)(request, counts[short])
    end
  end
  local env = {
    require = function(name)
      assert(name == "wasm.temporal"); counts.modules = counts.modules + 1; return exports
    end,
    sigil = {
      sleep = function(seconds)
        counts.sleep = counts.sleep + 1; if handlers.sleep then handlers.sleep(seconds) end
      end,
      json = { decode = function(data, options)
        counts.decode = counts.decode + 1
        return assert(handlers.decode, "unexpected decode")(data, options)
      end },
    },
  }
  setmetatable(env, { __index = _G })
  return assert(loadfile(script, "t", env))(), counts, calls
end
local function denied(value, err, code, operation)
  assert(value == nil and err.kind == "companion" and err.code == code, code)
  assert(err.operation == operation and err.effect == nil and err.server == nil and err.status == nil)
end
check("exact API and lazy WASM acquisition", function()
  local m, calls = fixture(); local n = 0
  for key in next, m do
    assert(({ run = true, wait_after_start = true, history = true, result = true, decode_json = true })[key])
    n = n + 1
  end
  assert(n == 5 and calls.modules == 0)
end)
check("one Start, exact request, no run-ID selection", function()
  local m, calls = fixture({
    start = function(r)
      assert(r["request-id"] == "one-id" and r["workflow-id"] == start["workflow-id"])
      assert(r.payloads[1].data == start.payloads[1].data)
      assert(r.payloads[2].metadata[1].value == start.payloads[2].metadata[1].value)
      return started
    end,
    describe = function(r)
      assert(r["workflow-id"] == selector["workflow-id"] and r["run-id"] == nil)
      assert(r["timeout-millis"] == 9000); return completed
    end,
  })
  local options = copy(wait); options.describe_timeout_millis = 9000
  assert(m.run(start, options) == completed and calls.start == 1 and calls.describe == 1 and calls.sleep == 0)
end)
check("false started and long output run ID preserve success", function()
  local response = copy(started); response.started = false; response["run-id"] = string.rep("r", 1024)
  local terminal = copy(completed); terminal["run-id"] = response["run-id"]
  local m = fixture({ start = function() return response end, describe = function() return terminal end })
  assert(m.run(start, wait) == terminal)
end)
check("Start and Describe stay on captured values despite checkpoint mutation", function()
  local request=copy(start); request.payloads={payload("{}"),payload("[]")}
  local options=copy(wait)
  options.checkpoint=function()
    request["workflow-id"]="changed"; request.profile="changed"; request["request-id"]="changed"
    request.payloads[1].data="changed"; request.payloads[2].metadata[1].value="changed"
  end
  local m,calls=fixture({
    start=function(r)
      assert(r["workflow-id"]==selector["workflow-id"] and r.profile==selector.profile)
      assert(r["request-id"]=="one-id" and r.payloads[1].data=="{}")
      assert(r.payloads[2].metadata[1].value=="json/plain"); return started
    end,
    describe=function(r)
      assert(r["workflow-id"]==selector["workflow-id"] and r.profile==selector.profile); return completed
    end,
  })
  assert(m.run(request,options)==completed and calls.start==1 and calls.describe==1)
end)
check("only RUNNING and exact post-start NOT_FOUND retry", function()
  local m, calls = fixture({ describe = function(_, n)
    if n == 1 then return nil, not_found end; return n == 2 and running or completed
  end, sleep = function(seconds) assert(seconds == 1) end })
  assert(m.wait_after_start(describe, wait) == completed and calls.describe == 3 and calls.sleep == 2)
end)
for _, n in next, { 0, 2, 3, 4, 5, 6, 7, 777 } do
  check("terminal/unknown number " .. n, function()
    local terminal = { ["run-id"] = "run-1", status = { number = n } }
    local m, calls = fixture({ describe = function() return terminal end })
    local options = copy(wait); options.max_attempts = 1
    assert(m.wait_after_start(describe, options) == terminal and calls.describe == 1 and calls.sleep == 0)
  end)
end
for _, n in next, { 1, 3, 4, 5, 7, 8, 13, 14, 16, 99 } do
  check("nonretryable server identity " .. n, function()
    local err = { kind = "server-status", server = { code = n, class = "unknown" } }
    local m, calls = fixture({ describe = function() return nil, err end })
    local value, actual = m.wait_after_start(describe, wait)
    assert(value == nil and actual == err and calls.describe == 1 and calls.sleep == 0)
  end)
end
for _, response in next, { "running", "not-found" } do
  for _, limit in next, { 1, 180 } do
    check("bounded exhaustion " .. response .. " " .. limit, function()
      local m, calls = fixture({ describe = function()
        if response == "running" then return running end; return nil, not_found
      end })
      local options = copy(wait); options.max_attempts = limit
      local value, err = m.wait_after_start(describe, options)
      denied(value, err, "poll-exhausted", "wait_after_start")
      assert(err.attempts == limit and calls.describe == limit and calls.sleep == limit - 1)
    end)
  end
end
check("unknown mutation and infrastructure never retry", function()
  local err = { kind = "infrastructure", effect = "unknown" }
  local m, calls = fixture({ start = function() return nil, err end })
  local value, actual = m.run(start, wait)
  assert(value == nil and actual == err and calls.start == 1 and calls.describe == 0)
  m, calls = fixture({ describe = function() return nil, err end })
  value, actual = m.wait_after_start(describe, wait)
  assert(value == nil and actual == err and calls.describe == 1 and calls.sleep == 0)
end)
for _, boundary in next, { "rpc", "sleep", "before", "after" } do
  check("exception propagation " .. boundary, function()
    local marker, checkpoints = {}, 0
    local options = copy(wait); options.max_attempts = boundary == "sleep" and 2 or 1
    options.checkpoint = function()
      checkpoints = checkpoints + 1
      if boundary == "before" or (boundary == "after" and checkpoints == 2) then error(marker) end
    end
    local m, calls = fixture({
      describe = function() if boundary == "rpc" then error(marker) end; return running end,
      sleep = function() error(marker) end,
    })
    local ok, err = pcall(m.wait_after_start, describe, options)
    assert(not ok and err == marker and calls.describe <= 1)
  end)
end
check("returned plugin error precedes cancellation checkpoint", function()
  local err, n = { kind = "infrastructure" }, 0
  local options = copy(wait); options.checkpoint = function() n = n + 1; assert(n == 1) end
  local m = fixture({ describe = function() return nil, err end })
  local value, actual = m.wait_after_start(describe, options)
  assert(value == nil and actual == err and n == 1)
end)
check("invalid options cause zero RPCs", function()
  for key, values in next, { max_attempts = {0,181,1.0,math.huge,false},
    interval_millis = {-1,60001,0.5,"1"}, checkpoint = {true,{}},
    describe_timeout_millis = {0,10001}, unknown = {1} } do
    for _, bad in next, values do
      local options = copy(wait); options[key] = bad
      local m, calls = fixture()
      local value, err = m.run(start, options)
      denied(value, err, "invalid-option", "run"); assert(calls.start == 0)
    end
  end
  local m = fixture(); local options = copy(wait); options.describe_timeout_millis = 1
  local value, err = m.wait_after_start(describe, options)
  denied(value, err, "invalid-option", "wait_after_start")
end)
for _, filter in next, {"all-events","close-event"} do
  check("canonical flags and exact binary token " .. filter, function()
    local first, second, token = event(777), event(888), "\0token\255"
    local m, calls = fixture({ history = function(r,n)
      assert(r.filter == filter and r["wait-new-event"] == (filter == "close-event"))
      assert(r["skip-archival"] == (filter == "close-event") and r["run-id"] == nil)
      assert(r["next-page-token"] == (n == 1 and "" or token))
      assert(r["timeout-millis"] == (filter == "close-event" and 65000 or 10000))
      return n == 1 and page({first},token) or page({second})
    end })
    local value, err = m.history(selector,opts(filter))
    assert(err == nil and value.events[1] == first and value.events[2] == second and calls.history == 2)
    assert(value.events[1]["event-id"] == 9007199254740993)
  end)
end
check("empty pages and inclusive page limit", function()
  for _, limit in next, {1,128} do for _, final in next, {true,false} do
    local m,calls = fixture({ history = function(_,n) return page({}, final and n == limit and "" or tostring(n)) end })
    local options = opts(); options.max_pages = limit
    local value,err = m.history(selector,options)
    if final then assert(value.pages == limit and #value.events == 0)
    else denied(value,err,"page-exhausted","history") end
    assert(calls.history == limit)
  end end
end)
for _, tokens in next, {{"A","A"},{"A","B","A"}} do
  check("cycle tokens " .. #tokens, function()
    local m,calls = fixture({ history = function(_,n) return page({},tokens[n]) end })
    local options = opts(); options.max_pages = #tokens
    local value,err = m.history(selector,options)
    denied(value,err,"repeated-token","history"); assert(calls.history == #tokens)
  end)
end
check("History never retries NOT_FOUND", function()
  local m,calls = fixture({ history = function() return nil,not_found end })
  local value,err = m.history(selector,opts()); assert(value == nil and err == not_found and calls.history == 1)
end)
check("History checkpoint takes precedence after final response", function()
  for _,token in next, {"","next"} do
    local marker,n = {},0; local options = opts(); options.max_pages = 1
    options.checkpoint = function() n=n+1; if n==2 then error(marker) end end
    local m,calls = fixture({ history = function() return page({},token) end })
    local ok,err = pcall(m.history,selector,options)
    assert(not ok and err==marker and calls.history==1)
  end
end)
check("History options closed, bounded and no invented run selector", function()
  for key,values in next, {filter={"unknown",false},max_pages={0,129},max_events={-1,16385},
    max_bytes={-1,16777217},timeout_millis={0,10001,65001},
    ["wait-new-event"]={false},["skip-archival"]={false},["next-page-token"]={""}} do
    for _,bad in next,values do
      local options=opts(); options[key]=bad; local m,calls=fixture()
      local value,err=m.history(selector,options); denied(value,err,"invalid-option","history")
      assert(calls.history==0)
    end
  end
  local wrong=copy(selector); wrong["run-id"]="invented"; local m=fixture()
  local value,err=m.history(wrong,opts()); denied(value,err,"invalid-option","history")
end)
check("exact byte accounting and aliases preserve every occurrence", function()
  local p=payload("\0\255",{{key="encoding",value="raw"},{key="x",value="y"}})
  local done=event(2,{tag="workflow-completed",value={p,p}}); done["event-type"].label="LABEL"
  local failed=event(3,{tag="workflow-failed",value={{message="m",source="s",["activity-type"]="a"}}})
  local activity=event(10,{tag="activity-scheduled",value="Act"})
  local expected=#"LABEL"+#"workflow-completed"+2*(2+#"encoding"+#"raw"+2)+#"workflow-failed"+3+#"activity-scheduled"+3
  for _,delta in next,{0,-1} do
    local m=fixture({history=function() return page({done,failed,activity}) end})
    local options=opts(); options.max_bytes=expected+delta
    local value,err=m.history(selector,options)
    if delta==0 then assert(value.bytes==expected and value.events[1]==done and done.details.value[1]==p)
    else denied(value,err,"byte-exhausted","history") end
    assert(p.data=="\0\255")
  end
end)
check("zero and inclusive event/byte bounds", function()
  local options=opts(); options.max_events=0; options.max_bytes=0
  local m=fixture({history=function() return page() end}); assert(m.history(selector,options).bytes==0)
  m=fixture({history=function() return page({},"x") end})
  local value,err=m.history(selector,options); denied(value,err,"byte-exhausted","history")
  m=fixture({history=function() return page({event(777)}) end}); options.max_bytes=100
  value,err=m.history(selector,options); denied(value,err,"event-exhausted","history")
  options.max_events=1; assert(#m.history(selector,options).events==1)
end)
local function hostile() error("metamethod executed") end
local malformed={
  sparse=function() return page({[2]=event(777)}) end,
  foreign=function() return page({x=event(777)}) end,
  oversized=function() return page({[math.maxinteger]=event(777)}) end,
  record=function() local p=page(); p.extra=true; return p end,
  cycle=function() local p=page(); p.events[1]=p; return p end,
  metatable=function() return setmetatable(page(),{__index=hostile,__pairs=hostile}) end,
  protected=function() return setmetatable(page(),{__metatable=false}) end,
  nested=function() return page({event(2,{tag="workflow-completed",value={setmetatable(payload("x"),{})}})}) end,
  payload=function() return page({event(2,{tag="workflow-completed",value=payload("x")})}) end,
  variant=function() return page({event(777,{tag="unknown"})}) end,
  recursive=function() local n={message="x"}; n.cause=n; return page({event(3,{tag="workflow-failed",value={n}})}) end,
  token=function() return page({},string.rep("x",65537)) end,
}
for name,make in next,malformed do
  check("malformed " .. name, function()
    local m,calls=fixture({history=function() return make() end})
    local value,err=m.history(selector,opts()); denied(value,err,"malformed-result","history")
    assert(calls.history==1)
  end)
end
check("per-shape event/metadata/failure bounds", function()
  for _,kind in next,{"events","metadata","failure"} do for _,extra in next,{0,1} do
    local p,values
    values={}
    if kind=="events" then
      for n=1,4096+extra do values[n]=event(777) end; p=page(values)
    elseif kind=="metadata" then
      for n=1,32+extra do values[n]={key=tostring(n),value=""} end
      p=page({event(2,{tag="workflow-completed",value={payload("",values)}})})
    else
      for n=1,16+extra do values[n]={message=""} end
      p=page({event(3,{tag="workflow-failed",value=values})})
    end
    local m=fixture({history=function() return p end}); local value,err=m.history(selector,opts())
    if extra==0 then assert(value and err==nil) else denied(value,err,"malformed-result","history") end
  end end
end)
check("aggregate nodes count aliases and stop within pages", function()
  local values,shared={},event(777); for n=1,4096 do values[n]=shared end
  local m,calls=fixture({history=function(_,n) return page(values,tostring(n)) end})
  local value,err=m.history(selector,opts()); denied(value,err,"node-exhausted","history")
  assert(calls.history==3 and err.pages==3)
end)
check("exact aggregate 100000-node boundary includes selector and options", function()
  -- Input records:10. Page:3. Completion event:11. Other event:10.
  -- Each empty payload:3. 10+3+11+10+33322*3 = 100000.
  for _,extra in next,{false,true} do
    local values,shared={},payload("",{}); for n=1,33322 do values[n]=shared end
    local other=event(777); if extra then other["event-type"].label="" end
    local response=page({event(2,{tag="workflow-completed",value=values}),other})
    local m=fixture({history=function() return response end})
    local options=opts(); options.max_pages=1
    local value,err=m.history(selector,options)
    if extra then denied(value,err,"node-exhausted","history")
    else assert(value and err==nil) end
  end
end)
check("checkpoints cannot widen validated limits or change the captured selector", function()
  local options=opts(); options.max_pages=1
  local selected=copy(selector)
  options.checkpoint=function()
    options.max_pages=128; options.max_bytes=math.maxinteger; selected["workflow-id"]="changed"
    setmetatable(options,{__index=hostile})
  end
  local m,calls=fixture({history=function(r)
    assert(r["workflow-id"]==selector["workflow-id"]); return page({},"next")
  end})
  local value,err=m.history(selected,options)
  denied(value,err,"page-exhausted","history"); assert(calls.history==1)
end)
check("completed zero/one/multiple ordered raw payloads", function()
  for _,values in next,{{},{payload("")},{payload("false"),payload("\0\255")}} do
    local done=event(2,{tag="workflow-completed",value=values})
    local m=fixture({history=function() return page({done}) end})
    local value,err=m.result(selector,result_opts())
    assert(err==nil and value.tag=="completed" and value.payloads==values and value.event==done)
    assert(value.history.events[1]==done)
  end
end)
check("late completion versus non-completion", function()
  local done=event(2,{tag="workflow-completed",value={}})
  local m,calls=fixture({history=function(_,n) return n==1 and page({},"next") or page({done}) end})
  assert(m.result(selector,result_opts()).tag=="completed" and calls.history==2)
  for _,number in next,{3,4,21,27,28,777} do
    m=fixture({history=function() return page({event(number)}) end})
    assert(m.result(selector,result_opts()).tag=="not-completed")
  end
  m=fixture({history=function() return page() end})
  assert(m.result(selector,result_opts()).tag=="not-completed")
end)
check("completion mismatches and multiple terminal events", function()
  local done=event(2,{tag="workflow-completed",value={}})
  for _,events in next,{{event(2)},{event(777,{tag="workflow-completed",value={}})},
    {done,done},{done,event(4)},{event(3),event(3)}} do
    local m=fixture({history=function() return page(events) end})
    local value,err=m.result(selector,result_opts()); denied(value,err,"malformed-result","result")
  end
end)
check("earlier completion does not hide later error or exhaustion", function()
  local done=event(2,{tag="workflow-completed",value={}}); local err={kind="infrastructure"}
  local m=fixture({history=function(_,n) if n==1 then return page({done},"next") end; return nil,err end})
  local value,actual=m.result(selector,result_opts()); assert(value==nil and actual==err)
  m=fixture({history=function() return page({done},"next") end})
  local options=result_opts(); options.max_pages=1
  value,actual=m.result(selector,options); denied(value,actual,"page-exhausted","result")
end)
check("single decode preserves false/raw bytes and acquires no plugin", function()
  for _,expected in next,{false,9007199254740993,"JSON-looking text",{}} do
    local original=payload('"raw"')
    local m,calls=fixture({decode=function(data,options) assert(data==original.data and options.max_bytes==99); return expected end})
    local value,err=m.decode_json(original,{max_bytes=99})
    assert(value==expected and err==nil and calls.decode==1 and calls.modules==0 and original.data=='"raw"')
  end
end)
check("unsupported encoding or malformed payload never decodes", function()
  for _,metadata in next,{{},{{key="encoding",value="binary/plain"}},
    {{key="encoding",value="json/plain"},{key="encoding",value="json/plain"}}} do
    local m,calls=fixture(); local value,err=m.decode_json(payload("{}",metadata))
    denied(value,err,"unsupported-encoding","decode_json"); assert(calls.decode==0)
  end
  for _,input in next,{setmetatable(payload("{}"),{__metatable=false}),
    payload("{}",{[2]={key="encoding",value="json/plain"}}),{data=1,metadata={}}} do
    local m,calls=fixture(); local value,err=m.decode_json(input)
    denied(value,err,"malformed-result","decode_json"); assert(calls.decode==0)
  end
end)
check("decode exception identity, no fallback or second pass", function()
  local marker={}; local m,calls=fixture({decode=function() error(marker) end})
  local original=payload("not json"); local ok,err=pcall(m.decode_json,original)
  assert(not ok and err==marker and calls.decode==1 and original.data=="not json")
end)
print("PASS " .. cases .. " offline bounded companion cases")
