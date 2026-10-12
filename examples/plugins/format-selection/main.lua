-- format-selection: эталонный плагин H2 (Фаза 5).
-- По команде оборачивает весь документ в **…** (Markdown-жирный).
-- Выделение плагину в H2 недоступно (ViewContext без selection), поэтому команда
-- применяется ко всему тексту — отсюда название кнопки «Жирный (весь документ)».
-- doc_id приходит в payload команды.
-- Нужны document:read (чтение текста) и document:write (apply_edit).
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
    if payload.command_id == "format-selection.bold" then
      local id = payload.doc_id
      if not id then return end
      local text = read_all(id)
      if not text then return end
      -- Диапазон в байтах: `#text` — байтовая длина прочитанного текста.
      host.apply_edit(id, 0, #text, "**" .. text .. "**")
    end
  end)
end

function on_deactivate(ctx) end
