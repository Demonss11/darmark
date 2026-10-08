-- Фикстура Фазы 5 (BUG-002): плагин-наблюдатель. Ничего не делает с документом,
-- только рапортует в статусбар о каждом полученном событии шины. Тест фиксирует
-- семантику доставки: `command:invoked` уходит ВСЕМ активным плагинам (не только
-- владельцу команды), а правка чужого плагина порождает `document:changed`,
-- который тоже доставляется наблюдателю.
function on_activate(ctx)
  ctx.subscribe("command:invoked", function(name, payload)
    host.show_message("cmd:" .. tostring(payload.command_id))
  end)
  ctx.subscribe("document:changed", function(name, payload)
    host.show_message("changed:" .. tostring(payload.rev))
  end)
end

function on_deactivate(ctx) end
