-- Фикстура P3: бесконечный цикл без host-call'ов. Watchdog обязан снять child
-- по прогресс-таймауту (нет кадров от child за бюджет).
function on_activate(ctx)
  while true do end
end
