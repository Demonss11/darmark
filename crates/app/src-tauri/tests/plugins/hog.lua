-- Фикстура P3: аллокация 1 МиБ в цикле с удержанием ссылки. Job Object RSS-лимит
-- обязан сработать: child снят (crashed/timeout), GUI жив (тест завершён).
function on_activate(ctx)
  local keep = {}
  local i = 0
  while true do
    i = i + 1
    keep[i] = string.rep("x", 1024 * 1024)
  end
end
