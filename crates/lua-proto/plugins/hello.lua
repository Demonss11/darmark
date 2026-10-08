-- hello.lua — минимальный плагин для гейта P1.
-- Ожидаемое: запись "hello" в лог хоста, код возврата 0.

function on_activate(ctx)
  host.log("info", "hello")
end

function on_deactivate(ctx)
  host.log("info", "bye")
end
