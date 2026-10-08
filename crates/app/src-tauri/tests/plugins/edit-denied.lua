-- Фикстура Фазы 2: без document:write apply_edit обязан вернуть ошибку-**значение**
-- ({code, message, permission}), а не исключение Lua — on_activate не прерывается (§4.4 TZ-H2).
function on_activate(ctx)
  local ok, err = host.apply_edit("doc-1", 0, 0, "X")
  assert(ok == nil, "ok должен быть nil при отказе")
  assert(type(err) == "table", "ожидалась таблица ошибки, получено " .. type(err))
  assert(err.code == "permission_denied", "ожидался permission_denied, получено " .. tostring(err.code))
  assert(err.permission == "document:write", "ожидался permission document:write")
  host.log("info", "denied обработан как значение")
end
