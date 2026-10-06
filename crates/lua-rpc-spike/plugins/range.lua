-- range.lua — берёт из документа только окно (range-API), а не весь текст.
-- Демонстрирует выигрыш по памяти модели «child на плагин» (F22/F30).
function on_activate(ctx)
  local total = host.get_document_len("doc")
  local win = host.get_document_range("doc", 0, 4096)
  host.log("info", "range done, total=" .. total .. " window=" .. #win)
end
