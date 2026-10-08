-- Фикстура Фазы 5: on_activate пишет контент в своё тир-1 view.
-- Тест проверяет порядок в PluginHost::set_enabled: contributed view обязаны быть
-- зарегистрированы ДО старта child, иначе host.set_view_content → unknown_view.
function on_activate(ctx)
  host.set_view_content("viewplugin:main", "<b>ready</b>")
end

function on_deactivate(ctx) end
