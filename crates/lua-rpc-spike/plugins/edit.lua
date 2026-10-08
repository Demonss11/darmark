-- edit.lua — write-путь через round-trip.
function on_activate(ctx)
  local ok = host.apply_edit(1, 0, 5, "hello")
  host.log("info", "apply_edit ok=" .. tostring(ok))
end
