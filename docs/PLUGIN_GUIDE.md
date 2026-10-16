# Как подключить свой Lua-плагин в ПЕНАЛ

Пошаговое руководство: как положить `.lua`-плагин, включить его, править и перезагружать
без пересборки приложения. Полный перечень host-функций и формат манифеста — в скилле ассистента
**«Создать плагин»**; контракт и песочница — ADR-0021/0023.

Плагин исполняется в **отдельном процессе**: его сбой или зависание не роняют приложение,
но доступа к DOM нет — только host-функции и события.

---

## 1. Куда класть плагин

Плагин — каталог `%APPDATA%\darmark\plugins\<id>\` с `plugin.json` (манифест) и `main.lua` (код).

Запустите ПЕНАЛ один раз (папка создастся), либо откройте вручную: `Win+R` → `%APPDATA%\darmark\plugins`.

Каталог плагинов переопределяется env `DARMARK_PLUGINS_DIR`.

---

## 2. Минимальный плагин за 2 файла

```json
{
  "id": "hello",
  "name": "Hello",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua",
  "permissions": ["ui:statusbar"]
}
```

```lua
function on_activate(ctx)
  host.show_message("Привет из плагина hello!")
end
```

Сохраните файлы, запустите ПЕНАЛ — плагин появится в панели «Плагины» (включён по умолчанию).

> Быстрее всего: `darmark-plugin init hello` — разворачивает шаблон с подстановкой id/имени (см. §7).

---

## 3. Правка и перезагрузка

1. Измените `main.lua`, сохраните.
2. В панели «Плагины» нажмите **«Перезагрузить»**.

Плагин запустится заново с новым кодом — перезапускать ПЕНАЛ не нужно. При ошибке статус станет
«ошибка» с текстом. После 3 падений подряд плагин уходит в **карантин** — включите его вручную.

---

## 4. Пример: события + статусбар

```lua
function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local len = host.get_document_len(payload.doc_id)
    if len then
      local text = host.get_document_range(payload.doc_id, 0, len)
      local n = 0
      for _ in text:gmatch("%S+") do n = n + 1 end
      host.show_message("Слова: " .. n)
    end
  end)
end
```

Ключевое: `document:changed` не несёт текст — плагин читает его сам через `get_document_range`.
`ctx.subscribe` возвращает функцию-отписку; при деактивации отпишитесь.

Большие документы читают циклом окон: один вызов `get_document_range` отдаёт не больше 1 МиБ, поэтому
текст собирают по частям.

```lua
local WINDOW = 65536

local function read_all(id)
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
```

Тот же приём целиком — в эталонном примере `crates/plugin-catalog/examples/plugins/word-count`
репозитория.

---

## 5. Разрешения

Проверка прав — на хосте, **до** вызова функции; набор — deny-by-default, без объявленного
разрешения функция не сработает. Функции с разрешением возвращают два значения `(value, err)`:
при отказе `value = nil`, а `err` — таблица `{ code = "permission_denied", message, permission }`,
то есть отказ приходит **значением**, а не исключением.

| Разрешение | Что открывает | Функции |
|---|---|---|
| `document:read` | чтение текста | `get_document_len`, `get_document_range`, `get_document_version`, `get_document_text` |
| `document:write` | правку документа | `apply_edit` |
| `view:create` | появление своей вкладки | регистрация `contributes.views` (без согласия вкладка не появляется) |
| `view:modify` | запись HTML во вкладку | `set_view_content` |
| `ui:statusbar` | сообщения в статусбар | `show_message` |

Разрешения не нужны: `host.log`, `host.get_setting`, `host.set_setting`, `host.export_html`,
`md.to_html`, `md.to_html_mapped`, `json.encode`, `json.decode`.

`ui:menu`/`ui:sidebar`/`ui:toolbar` отложены и манифестом сейчас **отвергаются**;
`filesystem:*` и `network` не предоставляются вовсе. Экспорт HTML — через `host.export_html`
(нативный диалог сохранения, без прав на файловую систему).

---

## 6. Своё представление

Плагин может добавить вкладку в панель предпросмотра. Нужны разрешения `view:create` и `view:modify`.

`plugin.json`:

```json
"contributes": {
  "views": [{ "kind": "my-preview", "title": "Моё представление", "tier": 1 }]
}
```

```lua
local VIEW = "<id>:my-preview"   -- view_id = "<id_плагина>:<kind>"

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local text = host.get_document_range(payload.doc_id, 0, host.get_document_len(payload.doc_id))
    local html = "<p>Символов: " .. #text .. "</p>"
      .. '<span data-p-my-preview-action="ping">нажми меня</span>'
    host.set_view_content(VIEW, html)
  end)
end

function on_action(view_id, action, payload)
  if action == "ping" then
    host.set_view_content(view_id, "<p>Поймал клик!</p>")
  end
end
```

Клик по `data-p-<plugin_id>-action="..."` → `on_action`. HTML санитизируется хостом (allowlist,
скрипты/`javascript:` вырезаются).

---

## 7. Отладка

```text
darmark-plugin test <path> [--json] [--verbose]                  # манифест, entry и прогон в песочнице
darmark-plugin init <dir> [--id <id>] [--name <name>] [--force]  # развернуть шаблон плагина
```

`test` прогоняет плагин в песочнице без открытого документа — host-вызовы при этом отклоняются;
код выхода `0` — успех, `1` — ошибки; `--json` печатает отчёт машинным форматом, `--verbose` —
подробности. `init` разворачивает `plugin.json`, `main.lua` и `README.md`, подставляя `--id` и
`--name`; `--force` перезаписывает существующий каталог.

Каталог доступных плагинов (`plugins/index.json`, команды `list`, `install`, `link`, `remove`,
`update`, выбор каталогов `--repo`/`--dest`) — ADR-0026 §2.

---

## 8. Частые ошибки

| Симптом | Причина / что делать |
|---|---|
| Плагина нет в списке | Ошибка манифеста: `id`, `entry`, `api_version`, размер исходника > 1 МиБ |
| Статус «ошибка»: `permission_denied` | Не хватает разрешения — добавьте в `permissions` |
| `set_view_content` не работает | Нет `view:modify` или `view_id` не `<id>:<kind>` |
| Клик в view не срабатывает | Атрибут `data-p-<id>-action`, объявите `on_action` |
| Сообщение не видно | Нужно `ui:statusbar` |
| Зависает | Watchdog снимает долгие вызовы |
| Карантин | 3 падения подряд — включите вручную после исправления |

---

## 9. Границы системы

**Можно:** читать/править документ, показывать сообщения, строить представление, подписываться
на события, экспортировать HTML через нативный диалог, хранить настройки (`get_setting`/`set_setting`).

**Нельзя:** сеть, файлы напрямую, DOM редактора, нативные панели/меню, `os`/`io`/`require`/`load`/`coroutine`.

---

## См. также

- `docs/adr/0023-plugin-runtime.md` — архитектура плагинной системы (тиры view, sandbox)
- `docs/adr/0026-local-plugin-catalog.md` — каталог плагинов и CLI `darmark-plugin`
- `docs/adr/0027-host-api-version-policy.md` — версии host-API и депрекация
- Эталонные примеры — `crates/plugin-catalog/examples/plugins/` репозитория
- Навык `plugin-sandbox` — механика песочницы: permissions, лимиты, карантин
