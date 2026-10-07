-- Фикстура Фазы 1: проверяет, что sandbox голый, host.log работает, md.* доступен.
function on_activate(ctx)
  assert(os == nil, "os должен быть недоступен")
  assert(io == nil, "io должен быть недоступен")
  assert(require == nil, "require должен быть недоступен")
  assert(load == nil, "load должен быть недоступен")
  assert(rawget == nil, "rawget должен быть недоступен")
  assert(ctx.api_version == 1, "api_version должен быть 1")
  host.log("info", "hello: активирован")
  local html = md.to_html("# hello")
  assert(html:find("<h1"), "md.to_html не вернул заголовок")
end

function on_deactivate(ctx) end
