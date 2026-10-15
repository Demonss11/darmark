-- Фикстура P3 (M6): подсчёт слов через host.get_document_text. Проверяет, что
-- host-call работает в продуктовом runtime и результат корректный.
function on_activate(ctx)
  local text = host.get_document_text("doc-1")
  local words = 0
  for _ in string.gmatch(text, "%S+") do words = words + 1 end
  host.log("info", "count: " .. tostring(words) .. " слов")
end
