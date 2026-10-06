-- chatty.lua — бесконечно дёргает дешёвый host-вызов. Прогресс-таймаут сбрасывается на
-- каждом ответе и никогда не срабатывает; снять плагин должен абсолютный дедлайн (F35).
function on_activate(ctx)
  while true do
    host.get_document_version("doc")
  end
end
