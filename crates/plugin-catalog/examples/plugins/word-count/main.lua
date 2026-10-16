-- word-count: плагин H2, расширен до тир-1 панели статистики.
-- При каждом изменении документа считает слова/символы/строки/абзацы/заголовки,
-- обновляет view «Статистика» и показывает краткое сообщение в статусбаре.
-- Кнопка «Вставить сводку» (data-p-word-count-action="insert-template") дописывает
-- итоговую сводку в конец документа через host.apply_edit (нужен document:write;
-- при отказе показываем сообщение, а не роняем обработчик).
-- Чтение — окнами через host.get_document_range (основной путь, §4.2): полная длина
-- может превышать потолок одного окна (1 МиБ), поэтому читаем циклом.

local WINDOW = 65536
local VIEW = "word-count:stats"

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

-- Подсчёт статистики текста: слова, символы (utf8), без пробелов, строки,
-- абзацы (непустые строки вне заголовков/цитат/таблиц/списков), заголовки,
-- ≈ минут чтения (200 слов/мин, округление до ближайшего, минимум 1 при словах > 0).
local function analyze(text)
  local words = 0
  for _ in text:gmatch("%S+") do words = words + 1 end
  local chars = utf8.len(text) or #text
  -- %S/%s сопоставляют по одному БАЙТУ, а не символу: для кириллицы счёт по %S
  -- завышал бы результат вдвое. Считаем пробелы (ASCII, 1 байт) и вычитаем из
  -- числа символов — «без пробелов» остаётся согласованным с chars.
  local spaces = 0
  for _ in text:gmatch("%s") do spaces = spaces + 1 end
  local no_spaces = chars - spaces
  local lines = 0
  for _ in text:gmatch("[^\n]*") do lines = lines + 1 end
  local paragraphs = 0
  for line in text:gmatch("[^\n]+") do
    if not (line:match("^%s*#") or line:match("^%s*>") or line:match("^%s*|")
        or line:match("^%s*[-*+]") or line:match("^%s*%d+[.)]")
        or line:match("^%s*[-=][-=%s]*$")) then
      paragraphs = paragraphs + 1
    end
  end
  local heads = 0
  for line in text:gmatch("[^\n]+") do
    if line:match("^%s*#{1,6}%s") then heads = heads + 1 end
  end
  local reading_min = math.floor(words / 200 + 0.5)
  if reading_min < 1 and words > 0 then reading_min = 1 end
  return {
    words = words,
    chars = chars,
    no_spaces = no_spaces,
    lines = lines,
    paragraphs = paragraphs,
    heads = heads,
    reading_min = reading_min,
  }
end

-- HTML панели. Без <button> (запрещён санитайзером, DROP_CONTENT_TAGS) и без
-- inline-style (разрешён только text-align на td/th): кнопка — <div role="button">.
local function render(s)
  return table.concat({
    '<div class="pv-head"><b>Статистика</b> <span class="pv-note">word-count</span></div>',
    '<div class="wc-grid">',
    '<div><span>слов</span><b>' .. s.words .. '</b></div>',
    '<div><span>символов</span><b>' .. s.chars .. '</b></div>',
    '<div><span>без пробелов</span><b>' .. s.no_spaces .. '</b></div>',
    '<div><span>строк</span><b>' .. s.lines .. '</b></div>',
    '<div><span>абзацев</span><b>' .. s.paragraphs .. '</b></div>',
    '<div><span>заголовков</span><b>' .. s.heads .. '</b></div>',
    '<div><span>≈ чтения</span><b>' .. s.reading_min .. ' мин</b></div>',
    '</div>',
    '<div class="wc-btn" role="button" data-p-word-count-action="insert-template" title="Вставить сводку в конец документа">Вставить сводку</div>',
  }, "\n")
end

local last_doc_id = nil

local function refresh(doc_id)
  last_doc_id = doc_id
  local text = read_all(doc_id)
  if not text then return end
  local s = analyze(text)
  local ok, err = host.set_view_content(VIEW, render(s))
  if not ok then
    host.log("warn", "set_view_content: " .. (err and err.message or "?"))
  end
  host.show_message("Слова: " .. s.words)
end

function on_activate(ctx)
  -- Подписчик получает (name, payload); doc_id лежит в payload.
  ctx.subscribe("document:changed", function(name, payload)
    refresh(payload.doc_id)
  end)
  -- При открытии нового документа панель тоже должна обновиться.
  ctx.subscribe("document:opened", function(name, payload)
    refresh(payload.doc_id)
  end)
end

function on_action(view_id, action, payload)
  if action == "insert-template" then
    if not last_doc_id then
      host.show_message("Нет активного документа")
      return
    end
    local text = read_all(last_doc_id)
    if not text then return end
    local s = analyze(text)
    local snippet = "\n\n---\n\n_Сводка: " .. s.words .. " слов, " .. s.chars
      .. " символов, ≈ " .. s.reading_min .. " мин чтения._\n"
    -- Вставка в конец документа: у плагина нет доступа к курсору (§6.3),
    -- поэтому сводка дописывается в конец по байтовой длине текста.
    local ok, err = host.apply_edit(last_doc_id, #text, #text, snippet)
    if ok then
      host.show_message("Сводка вставлена")
    else
      -- document:write может быть отказано значением (permission_denied) —
      -- показываем понятное сообщение, а не роняем обработчик.
      host.show_message("Не удалось вставить: " .. (err and err.message or "отказ"))
      host.log("warn", "apply_edit: " .. (err and err.code or "?"))
    end
  end
end

function on_deactivate(ctx) end
