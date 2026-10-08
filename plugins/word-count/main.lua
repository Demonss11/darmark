-- word-count: эталонный плагин H2 (§6.5 DESIGN_DOC).
-- При каждом изменении документа считает слова и показывает их в статусбаре.
-- Чтение — окнами через host.get_document_range (основной путь, §4.2): полная длина
-- может превышать потолок одного окна (1 МиБ), поэтому читаем циклом.

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

local function count_words(text)
  local n = 0
  for _ in text:gmatch("%S+") do n = n + 1 end
  return n
end

function on_activate(ctx)
  -- Подписчик получает (name, payload); doc_id лежит в payload.
  ctx.subscribe("document:changed", function(name, payload)
    local id = payload.doc_id
    local text = read_all(id)
    if text then host.show_message("Слова: " .. count_words(text)) end
  end)
end

function on_deactivate(ctx) end
