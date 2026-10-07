-- Фикстура Фазы 6: каталог плагина для `--serve-plugin`.
-- `on_activate` намеренно не делает host-call (как hello.lua): активация не требует dev-хоста.
-- `on_event` подтверждает, что событие дошло уже после активации (маркер для теста).

local activated = false

function on_activate(ctx)
  assert(ctx.api_version == 1, "api_version должен быть 1")
  activated = true
end

function on_event(name, payload)
  assert(activated, "on_event до on_activate — активация не прошла")
  last_event = name
end

function on_deactivate(ctx) end
