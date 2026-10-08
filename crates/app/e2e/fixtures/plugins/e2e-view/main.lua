-- E2E-фикстура плагинного тир-1 view (Фаза 4 TZ-H2) и статусбара (§11.1 п.5).
--
-- on_activate публикует HTML с маркером "start"; клик по
-- data-p-e2e-view-action="ping" приходит в on_action и заменяет маркер на
-- "pong" — это детерминированная проверка обратной маршрутизации (§9.4).
-- <button> ядро вырезает вместе с содержимым, поэтому интерактив — <span>.
--
-- Команда e2e-view.say через command:invoked пишет сообщение в статусбар
-- (host.show_message; нужно право ui:statusbar) — проверка per-plugin элемента.
local VIEW = host.plugin_id .. ":e2e-view"

function on_activate(ctx)
  host.set_view_content(
    VIEW,
    '<span data-p-e2e-view-action="ping">ping</span><span id="e2e-marker">start</span>'
  )
  ctx.subscribe("command:invoked", function(_name, payload)
    if payload.command_id == "e2e-view.say" then
      host.show_message("привет из e2e")
    end
  end)
end

function on_action(view_id, action, payload)
  if action == "ping" then
    host.set_view_content(
      VIEW,
      '<span data-p-e2e-view-action="ping">ping</span><span id="e2e-marker">pong</span>'
    )
  end
end
