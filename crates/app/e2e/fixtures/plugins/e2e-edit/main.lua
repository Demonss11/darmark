-- E2E-фикстура плагинной правки документа (BUG-002, часть A).
--
-- on_activate подписывается на command:invoked; по команде e2e-edit.mark
-- заменяет весь текст документа детерминированным маркером. Это проверяет
-- сквозной путь: apply_edit → document:changed → document-updated →
-- pull document_snapshot → обновление редактора и предпросмотра.
-- doc_id приходит в payload команды; нужны document:read (len) и document:write.
function on_activate(ctx)
  ctx.subscribe("command:invoked", function(name, payload)
    if payload.command_id == "e2e-edit.mark" then
      local id = payload.doc_id
      if not id then return end
      host.apply_edit(id, 0, host.get_document_len(id), "# E2E-EDIT-MARKER")
    end
  end)
end
