-- delta.lua — читает окно и правит документ дельтой (range + apply_edit): минимальный
-- набор host-вызовов без полной копии документа (F22).
function on_activate(ctx)
  local rev = host.get_document_version("doc")
  local win = host.get_document_range("doc", 0, 64)
  local ok = host.apply_edit("doc", 0, 5, "HELLO")
  host.log("info", "delta done, rev=" .. rev .. " win=" .. #win .. " edit=" .. tostring(ok))
end
