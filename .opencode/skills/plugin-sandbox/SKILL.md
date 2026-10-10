---
name: plugin-sandbox
description: >
  Механика песочницы плагинов darmark: схема plugin.json, permissions и согласие пользователя, лимиты watchdog,
  карантин, канонические формулировки границы. Используй, когда нужно проверить или объяснить контракт плагина,
  понять что доступно плагину и что нет, разобраться в поведении при превышении лимитов, или верифицировать
  манифест перед загрузкой. Не дублирует роль plugin-developer — здесь только механика и контракты.
---

## Схема plugin.json

Манифест — JSON-файл рядом с `main.lua`.

**Обязательные поля:**

| Поле | Тип | Валидация |
|------|-----|-----------|
| `id` | string | Строчные латинские буквы, цифры, `-`, `_`. Без путей и пробелов. |
| `name` | string | Отображаемое имя плагина. |
| `version` | string | SemVer. |
| `api_version` | integer | ≥ 1 и ≤ `HOST_API_VERSION` (текущая = 1). |
| `entry` | string | Путь к `.lua`-файлу относительно корня плагина. Без путей (`../` запрещён). |

**Условные поля:**

| Поле | Тип | Описание |
|------|-----|----------|
| `permissions` | string[] | Массив permissions из H2_PERMISSIONS. |
| `contributes` | object | Вклады: `views`, `commands`, `statusbar`, `toolbar`, `settings`. |

## Permissions и модель согласия

**H2_PERMISSIONS (доступны в текущей версии):**

| Permission | Что даёт |
|------------|----------|
| `document:read` | Чтение текста документа через `host.get_document_*` |
| `document:write` | `host.apply_edit` |
| `view:create` | Создание view |
| `view:modify` | `host.set_view_content` (HTML санитизируется на хосте) |
| `ui:statusbar` | Доступ к статусбару |

**Формулировки границы:**

- **ISOLATION_NOTICE**: «Плагин исполняется в отдельном процессе: его сбой или зависание не затрагивают редактор. Это изоляция отказов, а не защита данных.»
- **DOCUMENT_ACCESS_NOTICE**: «Плагин с доступом к документу (document) читает его содержимое полностью, включая конфиденциальный текст.»

**Известные, но не входящие в H2** (отклоняются с `PermissionNotInH2`): `ui:menu`, `ui:sidebar`, `ui:toolbar`.

**Не реализуются вообще** (отклоняются с `UnsupportedPermission`): `network`, `filesystem:*`.

## Лимиты watchdog

| Лимит | Значение | Поведение при превышении |
|-------|----------|---------------------------|
| `PROGRESS_TIMEOUT_MS` | 400 мс | Нет события от child за бюджет → `TerminateJobObject` |
| `DEADLINE_MS` | 1500 мс | Абсолютный wall-clock дедлайн на invocation (не сбрасывается событиями) |
| `MAX_PLUGIN_SOURCE_BYTES` | 1 МиБ | Лимит размера `.lua`-исходника |

**Карантин:** 3 падения подряд → авто-отключение плагина. Состояние per-plugin живёт в хосте. Пользователь может включить обратно вручную после исправления.

## Песочница Lua

**Разрешенные стандартные библиотеки:** `string`, `table`, `math`, `utf8`.

**Убранные стандартные библиотеки:** `os`, `io`, `require`, `package`, `debug`, `coroutine`, `load`, `loadfile`, `dofile`, `collectgarbage`, `rawget`, `rawset`.

**Нативно в child:**
- `host.*` — RPC к GUI-хосту: `get_document_len`, `get_document_version`, `get_document_range`, `get_document_text`, `apply_edit`, `set_view_content`, `get_setting`, `set_setting`, `show_message`, `export_html`, `log`, `plugin_id`.
- `md.*` — рендер Markdown: `to_html`, `to_html_mapped` (md-core линкуется в child).
- `json.*` — сериализация: `encode`, `decode`.
- `print` — перенаправлен в лог.

**Точки входа:**
- `on_activate(ctx)` — вызывается при активации плагина.
- `on_deactivate(ctx)` — вызывается при деактивации.
- `on_event(name, payload)` — глобальный обработчик событий.
- `ctx.subscribe(name, handler)` / `ctx.unsubscribe(name, handler)` — подписки на события.

## Контракт ошибок host-call

- **Успех:** `(value, nil)`
- **Отказ:** `(nil, {code, message, permission?, data?})`
- **Permission-отказ:** приходит значением (второй элемент возвращаемого кортежа), а не Lua-исключением. Проверяй явно.

## Источник формулировок

Единый источник канонических формулировок — `crates/plugin-proto/src/notices.rs`. Не переписывай текст при переносе в другие сущности.
