-- Фикстура Фазы 2: бесконечно дёргает дешёвый host-call. Прогресс-таймаут сбрасывается на
-- каждом ответе — снять обязан абсолютный дедлайн invocation (F35).
function on_activate(ctx)
  while true do
    host.get_document_version("doc-1")
  end
end
