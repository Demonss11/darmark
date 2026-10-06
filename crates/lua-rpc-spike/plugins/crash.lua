-- crash.lua — объявленный крах child; parent обязан пережить.
function on_activate(ctx)
  host.crash()
end
