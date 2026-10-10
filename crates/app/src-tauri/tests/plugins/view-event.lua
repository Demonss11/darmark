-- Фикстура: тир-1 view наполняется ТОЛЬКО по подписке на document:opened/changed,
-- а не в on_activate. Проверяет reannounce после (ре)старта плагина: без него вкладка
-- по подписке осталась бы пустой (хост не переиздаёт событие).
local VIEW = host.plugin_id .. ":main"

local function render()
  host.set_view_content(VIEW, '<span id="e2e-marker">start</span>')
end

function on_activate(ctx)
  ctx.subscribe("document:opened", function() render() end)
  ctx.subscribe("document:changed", function() render() end)
end

function on_deactivate(ctx) end
