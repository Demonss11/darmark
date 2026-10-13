# PLUGIN API — референс Lua-плагинов ПЕНАЛ

> **Язык:** Lua **5.5** (mlua `lua55` + vendored). Референс написан под 5.5, а не скопирован
> из материалов под 5.1/5.4. Нормативная архитектура — `docs/DESIGN_DOC.md`, ADR-0021/0022/0023.
> Образцы — каталог `examples/plugins/` в репозитории.

Плагин — это каталог `%APPDATA%/darmark/plugins/<id>/` с манифестом `plugin.json` и `.lua`-файлом.

---

## 1. Манифест `plugin.json`

```json
{
  "id": "word-count",
  "name": "Word Count",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua",
  "permissions": ["document:read", "ui:statusbar"],
  "contributes": {
    "views": [{ "kind": "wordcount", "title": "Статистика", "tier": 1 }],
    "commands": [{ "id": "wordcount.count", "title": "Посчитать слова", "keybinding": "Ctrl+Alt+W" }],
    "statusbar": [{ "id": "wordcount.status", "text": "Слова: {n}" }],
    "toolbar": [{ "id": "wordcount.btn", "title": "Статистика", "command": "wordcount.count" }],
    "settings": { "type": "object", "properties": { "ignore_code": { "type": "boolean", "default": true } } }
  }
}
```

- `id` — `[a-z0-9_-]+`; `entry` — имя `*.lua` без путей и `..`.
- `api_version` — целое ≥ 1; плагин с `api_version > HOST_API_VERSION` (сейчас 1) отвергается.
- Пустые обязательные поля (`name`, `version`) — ошибка.
- **Лимит исходника:** размер `.lua`-файла ≤ **1 МиБ** (`MAX_PLUGIN_SOURCE_BYTES`).

### Разрешения

| Разрешение | Что даёт |
|---|---|
| `document:read` | `get_document_len`, `get_document_range`, `get_document_version`, `get_document_text` |
| `document:write` | `apply_edit` |
| `view:create` | регистрация тир-1 view (`contributes.views`) — **гейтит регистрацию**: без согласия вкладка не появляется |
| `view:modify` | `set_view_content` |
| `ui:statusbar` | `show_message` |

> `view:create` гейтит регистрацию `contributes.views` (deny-by-default); запись HTML требует отдельного `view:modify`.

- Не поддерживаются: `ui:menu`/`ui:sidebar`/`ui:toolbar` (отвергаются), `filesystem:*`/`network` (не предоставляются).
- Проверка — на хосте, до вызова; `permission_denied` приходит **значением** (см. §3).

---

## 2. Точки входа

```lua
function on_activate(ctx) end      -- обязательна; вызывается после загрузки
function on_deactivate(ctx) end    -- опциональна; best-effort при остановке
function on_event(name, payload) end              -- опциональна; все события
function on_action(view_id, action, payload) end  -- опциональна; клики из своего view
```

`on_activate`/`on_deactivate` получают `ctx` — единственный фасад состояния плагина:

| `ctx` | Значение |
|---|---|
| `plugin_id`, `api_version` | идентификатор плагина и версия контракта (сейчас 1) |
| `subscribe(name, handler)` | подписка на событие; возвращает функцию-`unsubscribe` |
| `unsubscribe(name, handler)` | снять подписку вручную |

Подписчик: `handler(name, payload)`; при деактивации отпишитесь вручную. Глобальные `on_event`/`on_action` вызываются дополнительно.

---

## 3. host-функции

Контракт ошибок: функции, зависящие от разрешения, возвращают **два значения**:
`(value, err)`, где при успехе `err = nil`, при отказе `value = nil`, а `err` — таблица
`{ code, message, permission? , data? }`. `permission_denied` приходит значением, обработчик
продолжает работу. `host.log`, `md.*`, `json.*` ошибок разрешений не несут и возвращают одно значение.

### Документ

```lua
host.get_document_len(doc_id)                -> (integer, err)
host.get_document_range(doc_id, start, len)  -> (string, err)   -- ОСНОВНОЙ путь чтения
host.get_document_version(doc_id)            -> (integer, err)
host.get_document_text(doc_id)               -> (string, err)   -- вспомогательный
host.apply_edit(doc_id, start, stop, text)   -> (boolean, err)  -- document:write
```

- `start`/`stop`/`len` — **байтовые** смещения UTF-8 (как `data-md` в ядре). `end` подрезается
  вниз до границы символа; `start` вне границы символа → ошибка `invalid_range`.
- `get_document_range` — основной путь: окно ограничено лимитом кадра (1 МиБ). Для больших документов — цикл окон (см. §7).
- `apply_edit` заменяет `[start, stop)` на `text`; `rev` растёт только при реальной смене (эхо-защита).

### Представление (тир-1)

```lua
host.set_view_content(view_id, html)  -> (boolean, err)   -- view:modify
```

`view_id = "<plugin_id>:<kind>"` (kind из `contributes.views`). HTML **санитизируется на хосте**
(`md-core`, allowlist `data-p-*`); плагин не может протолкнуть сырой HTML. Обратная
маршрутизация: элемент с `data-p-<plugin_id>-action="foo"` (и опциональным
`data-p-<plugin_id>-payload="…"`) в HTML view → клик → `on_action(view_id, "foo", payload)`.

### Хост

```lua
host.log(level, message)      -- "debug"|"info"|"warn"|"error"; print() идёт сюда же
host.show_message(text)       -- (boolean, err)   -- ui:statusbar; уведомление в статусбар
host.export_html(html)        -- (boolean, err)   -- нативный диалог сохранения; без filesystem:write
host.get_setting(key)         -- (value, err)     -- постоянная настройка плагина; нет ключа → nil
host.set_setting(key, value)  -- (boolean, err)   -- записать настройку (произвольный JSON)
```

`host.log`/`print` → stderr (в GUI — stderr приложения; в dev-режиме — кадром `ToHost::Log`).

**Настройки плагина** (`get_setting`/`set_setting`) — per-plugin хранилище (JSON, `%APPDATA%/darmark/plugin-settings/<id>.json`); отсутствующий ключ → `nil`. Лимиты: ключ ≤ 256 Б, значение ≤ 64 КиБ, файл ≤ 256 КиБ; превышение → `too_large`.

`export_html`: `true` — записано, `false` — отмена; `filesystem:write` не требуется.

### Lua-нативные удобства (без JSON-прослойки)

`md.to_html(text, opts?)`, `md.to_html_mapped(text, opts?)` — обёртки над `md-core`; `json.encode(value)`, `json.decode(s)`.

---

## 4. События

Модель **notify-only + pull**: событие не несёт содержимое документа, только `doc_id`+`rev`;
текст плагин читает сам. Каталог событий:

| Событие | payload |
|---|---|
| `document:changed` | `{ doc_id, rev }` |
| `document:opened` / `document:closed` | `{ doc_id, path }` (`path` в `closed` всегда `null`) |
| `command:invoked` | `{ command_id, doc_id }` |

`document:changed` коалесцируется по `rev`; команды из `contributes.commands` → `command:invoked`.

---

## 5. Особенности Lua 5.5

- **Явные глобалы:** `global x`; неявные в скопе с декларацией запрещены (опечатка → ошибка компиляции). Точки входа (`on_activate`, …) — глобальные функции.
- **Компактные массивы:** крупные массивы строк укладываются в лимит памяти.
- **Внешние строки:** текст документа отдаётся без копирования в Lua-кучу.
- **`table.create(narr, nhash, nelem)`** — предвыделение для больших массивов.

Библиотеки: `BASE` (с чистки), `STRING`, `TABLE`, `MATH`, `UTF8`. Недоступны: `OS`, `IO`, `PACKAGE`, `COROUTINE`, `DEBUG`; из `BASE` удалены `load`, `loadfile`, `dofile`, `collectgarbage`, `rawget`, `rawset`; `print` → `host.log("info", …)`.

---

## 6. Отладка

### CLI разработчика `darmark-plugin` (Фаза 1 H3)

Отдельный бинарник `darmark-plugin` (крейт `plugin-cli`) — для проверки и создания плагинов вне GUI.

- **`darmark-plugin test <path> [--json] [--verbose]`** — проверка манифеста, entry и прогона в песочнице; host-call'ы отклоняются (документа нет).
- **`darmark-plugin init <dir> [--id <id>] [--name <name>] [--force]`** — разворачивает шаблон (`plugin.json`, `main.lua`, `README.md`).

### Локальный каталог плагинов `plugins/index.json`

Каталог доступных плагинов — версионируемый `index.json` в каталоге плагинов репозитория
(`plugins/index.json`). Запись каталога несёт только `id`, относительный `path` (от каталога
репозитория) и `description`; `name`/`version`/`permissions` берутся из `plugin.json` при загрузке
каталога (не дублируются):

```json
{
  "schema_version": 1,
  "plugins": [
    { "id": "word-count", "path": "word-count", "description": "Счётчик слов/символов в статусбаре" }
  ]
}
```

При загрузке каталога для каждой записи проверяются: безопасность `id` и `path` (без `..`, `:`,
абсолютных путей), наличие и валидность `<repo>/<path>/plugin.json` (`Manifest::from_json`) и
совпадение `id` записи с `manifest.id`. `schema_version` обязан быть `1`.

```text
darmark-plugin list [--repo <dir>] [--json]                 # доступные плагины (таблица/JSON-массив)
darmark-plugin install <id> [--repo <dir>] [--dest <dir>] [--force]  # копия в каталог приложения
darmark-plugin link <id> [--repo <dir>] [--dest <dir>] [--force]     # ссылка на источник (разработка)
darmark-plugin remove <id> [--dest <dir>]                   # удалить установленное
darmark-plugin update [<id>] [--repo <dir>] [--dest <dir>]  # обновить копии до версии каталога
```

- **`install`** — копия `<repo>/<path>` → `<dest>/<id>` (с `--force` перезаписывает).
- **`link`** — junction/symlink на источник (разработка).
- **`remove`** — идемпотентен; удаляет `<dest>/<id>` или точку монтирования.
- **`update`** — переустанавливает копии с устаревшей версией; ссылки пропускаются.

**Разрешение путей:** `--repo`: явный → env `DARMARK_PLUGIN_REPO` → предок с `plugins/index.json` → `<cwd>/plugins`. `--dest`: явный → env `DARMARK_PLUGINS_DIR` → `%APPDATA%/darmark/plugins`.

В `--serve-plugin`/`--debug-plugin` — stdio-конверт (`u32 LE длина || JSON`), host-call'ы обслуживает разработчик (mock-хост); `on_deactivate` при EOF не вызывается.

---

## 7. Пример (эталон `examples/plugins/word-count`)

```lua
local WINDOW = 65536

local function read_all(id)          -- окна: лимит кадра — 1 МиБ
  local len = host.get_document_len(id)
  if not len then return nil end
  local parts, pos = {}, 0
  while pos < len do
    local chunk = host.get_document_range(id, pos, WINDOW)
    if not chunk or #chunk == 0 then break end
    parts[#parts + 1] = chunk
    pos = pos + #chunk
  end
  return table.concat(parts)
end

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local text = read_all(payload.doc_id)
    if text then
      local n = 0
      for _ in text:gmatch("%S+") do n = n + 1 end
      host.show_message("Слова: " .. n)
    end
  end)
end
```

---

## См. также

- `docs/PLUGIN_GUIDE.md` — пошаговое руководство по написанию плагинов
- `docs/adr/0023-plugin-runtime.md` — архитектура плагинной системы
- `docs/DESIGN_DOC.md` — нормативная спецификация
- `docs/DESIGN_DOC.md` §13.2 — замороженные e2e-контракты
