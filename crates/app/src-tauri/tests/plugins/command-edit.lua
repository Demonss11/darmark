-- Фикстура Фазы 5: команда вставляет маркер в документ, чтобы тест убедился,
-- что run_plugin_command доставил command:invoked вместе с doc_id текущего документа.
function on_activate(ctx)
  ctx.subscribe("command:invoked", function(name, payload)
    if payload.command_id == "test.command" then
      host.apply_edit(payload.doc_id, 0, 0, "C")
    end
  end)
end

function on_deactivate(ctx) end
