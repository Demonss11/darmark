-- hang.lua — вызывает host-функцию и зацикливается (имитация «залипания» внутри host-вызова,
-- F32). Parent не получает прогресса; справиться должен per-call watchdog, а не грубый CPU-лимит.
function on_activate(ctx)
  local _ = host.get_document_version("doc")
  while true do end
end
