-- export-html: эталонный плагин H2 (Фаза 5).
-- По команде рендерит текущий документ в HTML и отдаёт его нативному диалогу хоста.
-- host.export_html не требует filesystem:write — согласие даёт сам диалог сохранения.
-- doc_id приходит в payload команды: у плагина нет своего «активного» документа.
-- Чтение — окнами (get_document_range), чтобы не упереться в потолок одного окна.

local WINDOW = 65536

local function read_all(id)
  local len = host.get_document_len(id)
  if not len then return nil end
  local parts = {}
  local pos = 0
  while pos < len do
    local chunk = host.get_document_range(id, pos, WINDOW)
    if not chunk or #chunk == 0 then break end
    parts[#parts + 1] = chunk
    pos = pos + #chunk
  end
  return table.concat(parts)
end

function on_activate(ctx)
  ctx.subscribe("command:invoked", function(name, payload)
    if payload.command_id == "export-html.export" then
      local id = payload.doc_id
      if not id then return end
      local text = read_all(id)
      if text then host.export_html(md.to_html(text)) end
    end
  end)
end

function on_deactivate(ctx) end
