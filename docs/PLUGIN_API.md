# PLUGIN API — референс Lua-плагинов darmark

> **Язык:** Lua **5.5** (mlua `lua55` + vendored). Референс написан под 5.5, а не скопирован
> из материалов под 5.1/5.4. Нормативная архитектура — `docs/DESIGN_DOC.md`, ADR-0021/0022/0023.
> Образцы — каталог `plugins/` в репозитории.

Плагин — это каталог `%APPDATA%/darmark/plugins/<id>/` с манифестом `plugin.json` и `.lua`-файлом.
Плагин исполняется в **отдельном процессе** и не видит DOM: единственный доступ к приложению —
host-функции и события (notify-only + pull).

Заготовку плагина создаёт **`darmark-plugin init <dir>`**, а проверяет **`darmark-plugin test <dir>`**
(манифест + entry + прогон в песочнице) — см. §6.

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

> Про `view:create`: вкладка регистрируется только у **включённого** плагина, у которого
> `view:create` входит в эффективный набор прав (`manifest ∩ granted`, deny-by-default). Без
> согласия на `view:create` `contributes.views` не регистрируется (вкладки нет), хотя плагин
> загружается и работает. Запись HTML всё равно требует отдельного `view:modify`.

- Известные, но **не поддерживаемые** (`ui:menu`, `ui:sidebar`, `ui:toolbar`) отвергаются с сообщением
  «не реализовано».
- `filesystem:*`/`network` **не предоставляются**: декларация отвергается валидатором.
- Проверка — **на хосте, до вызова**. Плагин без нужного разрешения получает `permission_denied`
  **значением**, а не исключением.

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
| `plugin_id` | id плагина (строка) |
| `api_version` | версия контракта (сейчас 1) |
| `subscribe(name, handler)` | подписка на событие; возвращает функцию-`unsubscribe` |
| `unsubscribe(name, handler)` | снять подписку вручную |

Подписчик вызывается как `handler(name, payload)`. `dispose`/деактивация обязаны отписаться
(иначе утечка). Глобальные `on_event`/`on_action` вызываются дополнительно, если объявлены.

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
- `get_document_range` — основной путь: окно ограничено лимитом кадра (**1 МиБ**). Для больших
  документов читайте **циклом** окон (см. пример `word-count` в §7).
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

`host.log`/`print` в GUI-хосте выводятся в stderr приложения; в dev-режиме
(`--serve-plugin`/`--debug-plugin`) — кадром `ToHost::Log` в назначенный stdio; в `--self-test` —
в stderr.

**Настройки плагина** (`get_setting`/`set_setting`) — постоянное хранилище **per-plugin**:
каждый плагин читает и пишет только свои настройки (scope по `plugin_id`, подставляется хостом).
Значение — произвольный JSON (число, строка, массив, объект, `null`); отсутствующий ключ
возвращает `nil` (`(nil, nil)`, не ошибку). Данные лежат в
`%APPDATA%/darmark/plugin-settings/<plugin_id>.json` и переживают перезапуск приложения.

- Лимиты: ключ ≤ 256 Б; одно значение ≤ 64 КиБ; файл настроек плагина ≤ 256 КиБ. Пустой/слишком
  длинный ключ → `bad_args`, значение/файл сверх лимита → `too_large` (значением, не паникой).
- UI-схема `contributes.settings` **пока не рендерится**: манифест её объявляет, но панель
  настроек плагина в менеджере не строится — плагин читает/пишет значения программно.

`export_html`: `true` — записано, `false` — пользователь отменил диалог; `filesystem:write`
не требуется (согласие даёт сам диалог).

### Lua-нативные удобства (без JSON-прослойки)

```lua
md.to_html(text, opts?)        -> string   -- обёртка над md-core
md.to_html_mapped(text, opts?) -> string
json.encode(value)             -> string
json.decode(s)                 -> value
```

---

## 4. События

Модель **notify-only + pull**: событие не несёт содержимое документа, только `doc_id`+`rev`;
текст плагин читает сам. Каталог событий:

| Событие | payload |
|---|---|
| `document:changed` | `{ doc_id, rev }` |
| `document:opened` | `{ doc_id, path }` |
| `document:closed` | `{ doc_id, path }` (`path` всегда `null` — путь закрытого документа хосту неизвестн) |
| `command:invoked` | `{ command_id, doc_id }` |

`document:changed` по одному `doc_id` **коалесцируется по `rev`** (доставляется последняя
ревизия за тик). Команды из `contributes.commands` исполняются событием `command:invoked`
(кнопкой «Выполнить» в менеджере плагинов).

---

## 5. Особенности Lua 5.5

- **Явные глобалы.** Есть декларация `global x`; в скопе с явной декларацией неявные глобалы
  запрещены — опечатка становится ошибкой компиляции, а не тихим `nil`. Локальные — `local`.
  Точки входа (`on_activate`, …) — глобальные функции.
- **Компактные массивы.** Крупные массивы заметно компактнее — плагин, читающий документ в
  массив строк, укладывается в лимит памяти.
- **Внешние строки.** Текст документа отдаётся без копирования в Lua-кучу (память не управляется
  сборщиком Lua).
- **`table.create(narr, nhash, nelem)`** — создать таблицу с предвыделением (для больших массивов).

Стандартные библиотеки: `BASE` (с чистки), `STRING`, `TABLE`, `MATH`, `UTF8`. **Недоступны:**
`OS`, `IO`, `PACKAGE`, `COROUTINE`, `DEBUG`; из `BASE` удалены `load`, `loadfile`, `dofile`,
`collectgarbage`, `rawget`, `rawset` (D17); `print` перенаправлен в `host.log("info", …)`.

---

## 6. Отладка

```text
darmark-plugin test <path>          # проверка плагина: манифест + entry + прогон в песочнице
darmark-plugin init <dir>           # создать шаблон плагина (plugin.json, main.lua, README.md)
darmark-plugin-host --self-test <plugin.lua>   # загрузить+активировать, лог в stderr
darmark-plugin-host --serve-plugin <path>      # load+activate и stdio-конверт (dev-хост)
darmark --debug-plugin <path>                  # запуск плагина без %APPDATA% (debug-сборка)
```

### CLI разработчика `darmark-plugin` (Фаза 1 H3)

Отдельный бинарник `darmark-plugin` (крейт `plugin-cli`) — для проверки и создания плагинов вне GUI.

- **`darmark-plugin test <path> [--json] [--verbose]`** — `<path>` это каталог плагина или `.lua`-файл.
  Последовательно проверяет: **манифест** (`plugin.json`: id/name/version/`api_version`/`entry`/permissions),
  **entry** (существует, читается, ≤ 1 МиБ, UTF-8, объявлена `on_activate`) и **прогон в песочнице**
  (реальный `load` + `on_activate` + `on_deactivate` в изолированном Lua через `plugin-host`).
  Код выхода `0` — все проверки пройдены, `1` — есть ошибки (ранний провал манифеста/entry
  пропускает последующие шаги). `--json` — машинночитаемый отчёт одним объектом, `--verbose` —
  подробности в stderr. host-call'ы в этом режиме отклоняются (документа нет) — это проверка
  загрузки/активации, а не полноценный прогон с документом.
- **`darmark-plugin init <dir> [--id <id>] [--name <name>] [--force]`** — разворачивает шаблон
  (`plugin.json`, `main.lua`, `README.md`) с подстановкой `id`/имени из имени каталога (или
  `--id`/`--name`). Без `--force` существующие файлы не перезаписываются.

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

- **`install`** копирует каталог `<repo>/<path>` → `<dest>/<id>` (манифест перечитывается и
  валидируется перед копированием). Существующая цель без `--force` — ошибка; с `--force` —
  удаляется и копируется заново.
- **`link`** создаёт **junction** на Windows (`mklink /J`, без прав администратора) или symlink на
  Unix — источник общий, правки в репозитории сразу видны в приложении. Источник канонизируется
  (`canonicalize`).
- **`remove`** идемпотентен: удаляет `<dest>/<id>` — ссылку/junction как точку монтирования
  (`fs::remove_dir`, цель не затрагивается), обычный каталог — целиком; если плагина нет —
  «нечего удалять» и код выхода `0`.
- **`update`** переустанавливает копии, у которых версия в источнике отличается от установленной;
  ссылки пропускаются (источник общий). Без `<id>` — все установленные плагины каталога.

**Разрешение путей по умолчанию:**

- `--repo` — явный аргумент → env **`DARMARK_PLUGIN_REPO`** → ближайший предок (включая CWD) с
  `plugins/index.json` → `<cwd>/plugins`. Принимается и каталог-репозиторий (`<dir>/plugins/index.json`),
  и сам каталог плагинов (`<dir>/index.json`).
- `--dest` — явный аргумент → env **`DARMARK_PLUGINS_DIR`** → `%APPDATA%/darmark/plugins` (тот же
  каталог, что сканирует GUI-хост).

Инвариант сборки: `darmark-plugin` линкует `plugin-host` **как библиотеку** (mlua), но GUI-хост
`darmark` — нет (D6/ADR-0021; CI проверяет `cargo tree -p darmark`).

В `--serve-plugin`/`--debug-plugin` конверт идёт поверх **назначенного stdio**: host-call'ы
плагина обслуживает разработчик (mock-хост). Формат кадра — `u32 LE длина || JSON`.
В dev-режиме плагин загружается и активируется **до** входа в цикл (кадров `done` для этих
синтетических шагов нет), а `on_deactivate` при EOF не вызывается — это отладочный прогон,
а не полноценный жизненный цикл хоста.

---

## 7. Пример (эталон `plugins/word-count`)

```lua
local WINDOW = 65536

local function read_all(id)          -- окна: полная длина может превышать лимит одного окна
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

function on_deactivate(ctx) end
```

---

## См. также

- `docs/PLUGIN_GUIDE.md` — пошаговое руководство по написанию плагинов
- `docs/adr/0023-plugin-runtime.md` — архитектура плагинной системы
- `docs/DESIGN_DOC.md` — нормативная спецификация
- `docs/DESIGN_DOC.md` §13.2 — замороженные e2e-контракты
