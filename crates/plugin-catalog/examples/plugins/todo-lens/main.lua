-- todo-lens: плагин H2. Находит задачи Markdown (- [ ] / - [x]) в документе,
-- показывает их в тир-1 view «Задачи» с прогрессом и позволяет переключать
-- отметку done кликом (правка только символа маркера через host.apply_edit).
-- Goto-line не реализован: у плагина нет доступа к курсору/прокрутке (§6.3),
-- поэтому номер строки отображается рядом с задачей, но не является ссылкой.
-- Чтение — окнами через host.get_document_range (основной путь, §4.2).

local WINDOW = 65536
local VIEW = "todo-lens:tasks"

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

-- Поиск задач построчно. Паттерн: отступ, маркер списка, [x]/[X]/[ ], пробелы, текст.
-- Возвращает таблицу задач: line (1-based), done, text, status_pos (байтовая позиция
-- символа статуса в документе — правим только его при переключении),
-- heading (текст ближайшего заголовка ATX выше задачи или nil), heading_level (1-6)
-- и heading_seq (номер заголовка — ключ группировки; nil для задач до первого заголовка).
-- Предполагаем LF-переводы строк (в документах darmark по умолчанию LF).
local function find_tasks(text)
  local tasks = {}
  local pos = 0 -- байтовое смещение начала текущей строки
  local line_num = 0
  local current_heading = nil
  local current_level = nil
  local current_heading_seq = nil -- порядковый номер заголовка (растёт на каждом)
  local heading_seq = 0
  for line in text:gmatch("[^\n]*") do
    line_num = line_num + 1
    -- Отслеживаем последний заголовок ATX (# ... ######).
    -- ВАЖНО: используем `#+`, а не `#{1,6}` — в Lua-шаблонах нет квантификатора
    -- {n,m}, поэтому `#{1,6}` искал литерал «{1,6}» и заголовки не находились.
    -- Ограничение «не больше 6 решёток» проверяем отдельно (CommonMark: 7+ — не
    -- заголовок, а обычный абзац). Учитываем и закрывающие решётки (### Текст ###).
    local hashes, heading_text = line:match("^(#+)%s+(.*)$")
    if hashes and #hashes <= 6 then
      -- Убираем хвостовые пробелы/CR и закрывающие решётки в стиле CommonMark.
      heading_text = heading_text:gsub("%s+$", ""):gsub("%s+#+%s*$", "")
      current_level = #hashes
      current_heading = heading_text
      -- Отдельный номер на каждый заголовок: одинаковый текст у двух заголовков
      -- (например, «## Прочее» и «# Прочее») — это РАЗНЫЕ группы, а не одна.
      heading_seq = heading_seq + 1
      current_heading_seq = heading_seq
    end
    local indent, status, sep, text_part = line:match("^(%s*[-*+])%s+%[([ xX])%](%s+)(.*)$")
    if indent then
      local done = status == "x" or status == "X"
      -- CRLF: убираем хвостовой \r, иначе он попадёт в отображаемый HTML.
      text_part = text_part:gsub("\r$", "")
      -- Позиция '[' в строке (в байтах — Lua-строки байтовые).
      -- Plain-режим (4-й аргумент true): искомое — литерал, поэтому экранировать не нужно.
      -- (Был баг: plain + '%[' искал подстроку '%[' — процент+скобка, — которой нет, → nil.)
      local bracket = line:find("[", 1, true)
      -- Защита: bracket может не найтись на нештатном вводе — тогда строку пропускаем.
      if bracket then
        -- status_pos — 0-based байтовое смещение символа статуса в документе (в таком виде его
        -- ждёт apply_edit). bracket — 1-based позиция '[' внутри строки: 0-based смещение '['
        -- в документе = pos + bracket - 1, а статус стоит на байт правее → pos + bracket.
        local status_pos = pos + bracket
        tasks[#tasks + 1] = {
          line = line_num,
          done = done,
          text = text_part,
          status_pos = status_pos,
          heading = current_heading,
          heading_level = current_level,
          heading_seq = current_heading_seq,
        }
      end
    end
    pos = pos + #line + 1 -- +1 на '\n'
  end
  return tasks
end

local function esc(s)
  return (s:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"):gsub('"', "&quot;"))
end

-- HTML панели. Оформление — ТОЛЬКО классами приложения (style.css): санитайзер
-- `md-core` вырезает inline-`style` (кроме `text-align` на td/th), поэтому
-- `.pv-head`, `.pv-note`, `.tl-progress`, `.tl-item`, `.tl-check`, `.tl-text`,
-- `.tl-ln`, `.tl-empty` берут стили из фронтенда. `<button>` запрещён
-- (DROP_CONTENT_TAGS) — элементы-кнопки несут `role="button"`.
-- Задачи группируются по заголовкам Markdown (ближайший выше по документу).
local function render(tasks, done_count)
  local total = #tasks
  local pct = total > 0 and math.floor(done_count / total * 100 + 0.5) or 0
  local filled = math.floor(pct / 10)
  local bar = string.rep("#", filled) .. string.rep("-", 10 - filled)
  local html = {
    '<div class="pv-head"><b>Задачи</b> <span class="pv-note">todo-lens · ' .. done_count .. ' из ' .. total .. '</span></div>',
    '<div class="tl-progress" role="progressbar">' .. bar .. ' ' .. done_count .. '/' .. total .. ' (' .. pct .. '%)</div>',
  }
  if total == 0 then
    html[#html + 1] = '<div class="tl-empty">Нет задач «- [ ]» в документе.</div>'
  else
    -- Группируем задачи по заголовкам, сохраняя порядок появления.
    -- Ключ — номер заголовка (heading_seq), а не его текст: два заголовка с
    -- одинаковым текстом («## Прочее» и «# Прочее») дают РАЗНЫЕ группы.
    -- 0 — задачи до первого заголовка («Без раздела»).
    local groups = {}
    local group_order = {}
    for _, t in ipairs(tasks) do
      local key = t.heading_seq or 0
      if not groups[key] then
        groups[key] = { heading = t.heading, level = t.heading_level, tasks = {} }
        group_order[#group_order + 1] = key
      end
      groups[key].tasks[#groups[key].tasks + 1] = t
    end

    for _, key in ipairs(group_order) do
      local group = groups[key]
      local group_done = 0
      for _, t in ipairs(group.tasks) do
        if t.done then group_done = group_done + 1 end
      end

      -- Заголовок группы: уровень заголовка несёт класс tl-h{n} (data-* санитайзер
      -- срезает — выживают только data-p-*; размер задаётся в CSS).
      if group.heading then
        html[#html + 1] = '<div class="tl-heading tl-h' .. (group.level or 0) .. '">'
          .. '<span class="tl-heading-text">' .. esc(group.heading) .. '</span>'
          .. '<span class="tl-group-progress">' .. group_done .. '/' .. #group.tasks .. '</span>'
          .. '</div>'
      else
        html[#html + 1] = '<div class="tl-heading tl-heading-none">'
          .. '<span class="tl-heading-text">Без раздела</span>'
          .. '<span class="tl-group-progress">' .. group_done .. '/' .. #group.tasks .. '</span>'
          .. '</div>'
      end

      -- Задачи группы
      for _, t in ipairs(group.tasks) do
        html[#html + 1] = '<div class="tl-item' .. (t.done and ' done' or '') .. '" role="button"'
          .. ' data-p-todo-lens-action="toggle"'
          .. ' data-p-todo-lens-payload="' .. t.line .. '"'
          .. ' title="Переключить задачу на строке ' .. t.line .. '">'
          .. '<span class="tl-check">' .. (t.done and "✓" or "") .. '</span>'
          .. '<span class="tl-text">' .. esc(t.text) .. '</span>'
          .. '<span class="tl-ln">:' .. t.line .. '</span>'
          .. '</div>'
      end
    end
  end
  return table.concat(html, "\n")
end

local last_doc_id = nil

local function refresh(doc_id)
  last_doc_id = doc_id
  local text = read_all(doc_id)
  if not text then return end
  local tasks = find_tasks(text)
  local done = 0
  for _, t in ipairs(tasks) do if t.done then done = done + 1 end end
  local ok, err = host.set_view_content(VIEW, render(tasks, done))
  if not ok then
    host.log("warn", "set_view_content: " .. (err and err.message or "?"))
  end
  host.show_message("Задачи: " .. done .. " из " .. #tasks)
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
  if action == "toggle" and last_doc_id then
    local line = tonumber(payload)
    if not line then return end
    local text = read_all(last_doc_id)
    if not text then return end
    local tasks = find_tasks(text)
    local task
    for _, t in ipairs(tasks) do
      if t.line == line then task = t break end
    end
    if not task then return end
    local new_status = task.done and " " or "x"
    -- Правим только символ статуса (1 байт) — остальное содержимое строки не трогаем.
    local ok, err = host.apply_edit(last_doc_id, task.status_pos, task.status_pos + 1, new_status)
    if ok then
      -- Перечитываем документ и обновляем панель сразу, не дожидаясь события
      -- document:changed (событие придёт от хоста, но лишний цикл чтения не нужен).
      local new_text = read_all(last_doc_id)
      if new_text then
        local new_tasks = find_tasks(new_text)
        local done = 0
        for _, t in ipairs(new_tasks) do if t.done then done = done + 1 end end
        host.set_view_content(VIEW, render(new_tasks, done))
        host.show_message("Задачи: " .. done .. " из " .. #new_tasks)
      end
    else
      host.show_message("Не удалось переключить: " .. (err and err.message or "отказ"))
      host.log("warn", "apply_edit: " .. (err and err.code or "?"))
    end
  end
end

function on_deactivate(ctx) end
