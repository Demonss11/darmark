-- Фикстура Фазы 2: «залипание» внутри on_activate. Снять должен прогресс-таймаут watchdog.
-- host-call до цикла нужен, чтобы документ существовал и вызов прошёл.
function on_activate(ctx)
  local _ = host.get_document_version("doc-1")
  while true do end
end
