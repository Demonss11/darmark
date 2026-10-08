-- Фикстура Фазы 2: write-путь через round-trip. Правка обязана поднять rev документа.
function on_activate(ctx)
  local ok = host.apply_edit("doc-1", 0, 0, "X")
  host.log("info", "edit ok=" .. tostring(ok))
end
