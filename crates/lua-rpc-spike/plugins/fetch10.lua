-- fetch10.lua — 10 последовательных крупных round-trip (устойчивая пропускная способность).
function on_activate(ctx)
  local n = 0
  for i = 1, 10 do
    n = n + #host.get_document_text(1)
  end
  host.log("info", "fetched " .. n)
end
