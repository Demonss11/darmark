-- hog.lua - аллокация больших строк в цикле; удержание ссылки, чтобы память не освободилась.
-- Ожидание: лимит памяти Job Object срабатывает (аллокация падает → MemoryError/abort),
-- RSS процесса не растёт выше лимита, parent жив (P9/M11).
function on_activate(ctx)
  local keep = {}
  local i = 0
  while true do
    i = i + 1
    keep[i] = string.rep("x", 1024 * 1024) -- 1 МиБ на итерацию
  end
end
