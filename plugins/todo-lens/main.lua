-- todo-lens: плагин H2. Находит задачи Markdown (- [ ] / - [x]) в документе,
-- показывает их в тир-1 view «Задачи» с прогрессом и позволяет переключать
-- отметку done кликом (правка только символа маркера через host.apply_edit).
-- Goto-line не реализован: у плагина нет доступа к курсору/прокрутке (§6.5),
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
-- символа статуса в документе — правим только его при переключении).
-- Предполагаем LF-переводы строк (в документах darmark по умолчанию LF).
local function find_tasks(text)
  local tasks = {}
  local pos = 0 -- байтовое смещение начала текущей строки
  local line_num = 0
  for line in text:gmatch("[^\n]*") do
    line_num = line_num + 1
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

-- HTML панели. Без <button> (запрещён санитайзером, DROP_CONTENT_TAGS) и без
-- inline-style (разрешён только text-align на td/th): элемент задачи — <div role="button">.
-- Прогресс показываем текстом: inline-style для width санитайзер срезает.
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
    for _, t in ipairs(tasks) do
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
