-- Фикстура Фазы 2: аварийный крах child. GUI обязан пережить (изоляция процесса, D6).
function on_activate(ctx)
  host.log("info", "crash: падаю")
  host.crash()
end
