-- ping.lua — много мелких round-trip вызовов (замер латентности).
function on_activate(ctx)
  local last = 0
  for i = 1, 2000 do
    last = host.get_document_version(1)
  end
  host.log("info", "ping done, last=" .. last)
end
