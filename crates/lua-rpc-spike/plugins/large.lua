-- large.lua — один вызов, тянущий весь документ (замер передачи).
function on_activate(ctx)
  local text = host.get_document_text(1)
  host.log("info", "large done, len=" .. #text)
end
