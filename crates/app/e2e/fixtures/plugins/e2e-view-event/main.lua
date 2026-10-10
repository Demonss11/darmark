-- E2E-фикстура тир-1 view, наполняемого ТОЛЬКО по подписке (хост-фикс reannounce).
--
-- В отличие от e2e-view, on_activate не пишет HTML сразу: он лишь подписывается на
-- document:opened/changed и наполняет вкладку в обработчике — как реальные
-- word-count/todo-lens. Поэтому после reload/включения/смены прав вкладка останется
-- пустой, если хост не переиздаст плагину последний document:opened.
local VIEW = host.plugin_id .. ":e2e-view"

local function render()
  host.set_view_content(VIEW, '<span id="e2e-marker">start</span>')
end

function on_activate(ctx)
  ctx.subscribe("document:opened", function() render() end)
  ctx.subscribe("document:changed", function() render() end)
end

function on_deactivate(ctx) end
