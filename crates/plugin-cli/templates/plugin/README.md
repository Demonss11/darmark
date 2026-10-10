# {{NAME}}

Шаблон плагина darmark (id: `{{ID}}`).

## Структура

- `plugin.json` — манифест: id, имя, версия, точка входа, разрешения.
- `main.lua` — код плагина (точка входа `on_activate`).
- `README.md` — этот файл.

## Проверка

Запустите из каталога плагина:

```text
darmark-plugin test .
```

## Установка

Скопируйте каталог в `%APPDATA%\darmark\plugins\` — плагин появится в панели «Плагины»
при следующем запуске darmark.

Подробности — `docs/PLUGIN_GUIDE.md` и `docs/PLUGIN_API.md`.
