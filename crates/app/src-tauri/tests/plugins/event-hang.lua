-- Фикстура Фазы 2: активация успешна, но обработчик события зависает. Проверяет, что отказ
-- invocation в рантайме снимает supervisor и переводит плагин из Active (§3.2 ревью).
function on_activate(ctx) end

function on_event(name, payload)
  while true do end
end
