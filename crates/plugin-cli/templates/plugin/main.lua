-- {{ID}} — минимальный плагин darmark.
-- Плагин исполняется в отдельном процессе (песочница): доступны только host.*,
-- md.*, json.*; нет os/io/require/load. Точка входа — on_activate(ctx).

function on_activate(ctx)
  -- ctx.subscribe("document:changed", function(name, payload) ... end) вернёт отписку.
  host.show_message("Плагин {{ID}} активирован")
end

function on_deactivate(ctx)
  -- Освобождение ресурсов/отписки при выключении/перезагрузке.
end
