# IDEA-004 — Тонкий host + набор адаптеров/провайдеров

> **Статус:** обсуждение перед спайком (спайк не начат)
> **Дата создания:** 08.10.2026
> **Дата пересмотра:** 09.10.2026
> **Источник:** `ideas/UI-IDEAS.md` IDEA-004
> **Следующий шаг:** предложение подходов → дизайн → одобрение на спайк
> **Признак устаревания:** если спайк не начнётся до 31.12.2026, документ может быть удалён или перенесён в `ideas/IDEA-004-DEFERRED.md`
> **Связь с ROADMAP:** задача относится к H3 (после закрытия H2); см. `docs/ROADMAP.md` раздел H3

---

## 1. Контекст

IDEA-004 — это архитектурная идея по доведению плагинной системы от «Lua + узкий host-API» до платформы, где host максимально тонкий, а функциональность плагинам дают подключаемые **адаптеры** (мосты к хостовым подсистемам) и **провайдеры** (тир-1 `HtmlViewProvider`, команды/меню, будущие engines).

**Текущий статус IDEA-004:** `research (спайки) → adr`
**Порядок:** начинать после закрытия вехи H2. H2 закрыт (Фазы 1–6 реализованы, нормативная спецификация — `docs/DESIGN_DOC.md` и ADR). Спайк IDEA-004 можно начинать.

---

## 2. Текущее состояние системы (As-Is)

### 2.1 Архитектура

```
PluginHost (facade)
├── PluginManager (lifecycle, quarantine)
│   └── PluginRuntime (per-plugin)
│       └── Supervisor (process management)
│           ├── Child process (darmark-plugin-host.exe)
│           ├── Job Object (RSS limit + KILL_ON_JOB_CLOSE)
│           ├── Writer thread (stdin)
│           └── Reader thread (stdout)
├── EventBus (notify-only + pull)
├── PluginViews (tier-1 view registry)
├── SettingsStore (config.json)
└── HostServices (DocumentServices)
```

### 2.2 Host-функции (захардкожены)

| Функция | Метод host-call | Permission |
|---|---|---|
| `host.get_document_len` | `get_document_len` | `document:read` |
| `host.get_document_version` | `get_document_version` | `document:read` |
| `host.get_document_range` | `get_document_range` | `document:read` |
| `host.get_document_text` | `get_document_text` | `document:read` |
| `host.apply_edit` | `apply_edit` | `document:write` |
| `host.set_view_content` | `set_view_content` | `view:modify` |
| `host.show_message` | `show_message` | `ui:statusbar` |
| `host.export_html` | `export_html` | — (native dialog) |
| `host.get_setting` | `get_setting` | — (not implemented) |
| `host.set_setting` | `set_setting` | — (not implemented) |

### 2.3 Ключевые файлы

| Файл | Рôle |
|---|---|
| `crates/plugin-host/src/bindings.rs` | Реализация `host.*` функций, `HostApi` trait |
| `crates/plugin-host/src/main.rs` | stdio loop, `handle_invoke`, `RpcHost` |
| `crates/plugin-host/src/sandbox.rs` | Lua 5.5 с ограниченным StdLib |
| `crates/app/src-tauri/src/plugins/supervisor.rs` | Процессы, Job Object, watchdog, permission check |
| `crates/app/src-tauri/src/plugins/services.rs` | `DocumentServices` — реализация `HostServices` |
| `crates/app/src-tauri/src/plugins/views.rs` | `PluginViews` — реестр тир-1 представлений |
| `crates/app/src-tauri/src/plugins/manager.rs` | Сканирование, загрузка, quarantine |
| `crates/app/src-tauri/src/plugins/bus.rs` | EventBus (notify-only + pull) |
| `crates/app/src-tauri/src/plugins/settings.rs` | `SettingsStore` (config.json) |
| `crates/plugin-proto/src/envelope.rs` | Протокол: `ToChild`, `ToHost`, `PluginError` |
| `crates/plugin-proto/src/manifest.rs` | Манифест плагина, валидация permissions |
| `crates/plugin-proto/src/frame.rs` | Кадр: u32 LE length + JSON |

### 2.4 Ограничения (DESIGN_DOC)

| ID | Ограничение |
|---|---|
| D2 | Preview — встроенный тир-1 провайдер на хосте, не плагин |
| D6 | Размер базы ≤ 6 МБ; «нулевая база» |
| D16 | Изоляция плагинов — отдельный дочерний процесс; GUI-хост без `mlua` |

---

## 3. Целевое состояние (To-Be)

### 3.1 Концепция

```
Текущее:  Supervisor (хардкод host-fn) → DocumentServices (хардкод методов) → DocumentStore

Целевое:  Supervisor (тонкий: транспорт + dispatch)
          ↓
          AdapterRegistry (реестр адаптеров)
          ├── DocumentAdapter (document:read/write)
          ├── StatusbarAdapter (ui:statusbar)
          └── ... (будущие)
          ↓
          ProviderRegistry (реестр провайдеров)
          ├── HtmlViewProvider
          ├── CommandProvider
          └── ... (будущие)
```

### 3.2 Принципы

1. **Тонкое ядро** — Supervisor только транспорт, Job/watchdog, dispatch
2. **Адаптеры** — мосты к хостовым подсистемам (документ, файлы, диалоги, статусбар, события, экспорт)
3. **Провайдеры** — тир-1 `HtmlViewProvider`, команды/меню, будущие engines
4. **Декларация** — манифест (`contributes`) для объявления, Rust-трейты для реализации
5. **Permissions** — навешиваются на адаптер, а не на хардкод host-функции

---

## 4. Конечная цель спайка

**Спайк `crates/adapter-spike` должен доказать, что host может быть «тонким»** — минимальное ядро (транспорт, Job/watchdog, dispatch) + реестр адаптеров/провайдеров, где:

| Критерий | Целевое состояние |
|---|---|
| **Расширяемость** (главный) | Новый плагинный функционал добавляется без правок ядра — через реестр адаптеров |
| **Совместимость** | Текущие плагины работают без изменений, `HOST_API_VERSION` остаётся `1` |
| **Форма** | Гибрид: манифест (`contributes`) для декларации, Rust-трейты для реализации |
| **Минимальный набор** | 2 адаптера (DocumentAdapter, StatusbarAdapter) + 1 провайдер (HtmlViewProvider) |
| **Метрики** | Комбинация: точек расширения (ключевая), латентность, размер child/GUI ≤6 МБ |
| **Ограничения** | Замороженные контракты §13.2 и e2e зелёные |

---

## 5. Согласованные решения

### Q1 — Главный критерий успеха спайка

**Решение:** Все в комплексе, с приоритетом на **расширяемость** — это суть IDEA-004. Остальное — ограничения, которые нельзя нарушить.

### Q2 — Минимальный набор адаптеров/провайдеров

**Решение:** 2 адаптера + 1 провайдер:
- **DocumentAdapter** — `document:read` + `document:write`
- **StatusbarAdapter** — `ui:statusbar`
- **HtmlViewProvider** — `view:create` + `view:modify`

### Q3 — Совместимость с HOST_API_VERSION

**Решение:** Полная совместимость — текущие плагины работают без изменений, `HOST_API_VERSION` остаётся `1`. Мажорное изменение — отдельный ADR.

### Q4 — Форма описания адаптеров/провайдеров

**Решение:** Гибрид — манифест (`contributes`) для декларации (что плагин предоставляет), Rust-трейты для реализации (как адаптер работает).

### Q5 — Критерий «тонкости» host

**Решение:** Комбинация метрик, с ключевой метрикой **число точек расширения** — сколько адаптеров/провайдеров можно добавить без правок ядра. Остальные — ограничения (не ухудшить).

### Q6 — Функционал DocumentAdapter

**Решение:** Read + Write — достаточно, чтобы доказать, что адаптер покрывает оба права (read/write), и показать, как `apply_edit` работает через адаптер.

### Q7 — Форма реестра адаптеров

**Решение:** Гибрид — базовые адаптеры (Document, Statusbar) зарегистрированы в коде, но архитектура позволяет добавлять новые через манифест. Это соответствует текущей модели (contributes.views).

### Q8 — Привязка permission-модели к адаптерам

**Решение:** Атрибут на адаптере — каждый адаптер имеет метод `required_permission()`, диспетчер проверяет перед вызовом. Это сохраняет текущую модель (permission на метод), но переносит его в адаптер.

### Q9 — Измерение «тонкости» host

**Решение:** Все метрики, с ключевой **число точек расширения** — это прямо измеряет расширяемость, которая является целью IDEA-004. Остальные — ограничения (не ухудшить).

### Q10 — План спайка

**Решение:** Минимальный прототип — перенести 2 адаптера + 1 провайдер, замерить метрики. Это достаточно, чтобы доказать паттерн, и не раздувать спайк.

---

## 6. Следующие шаги

1. **Предложить 2-3 подхода** к реализации спайка с рекомендацией
2. **Представить дизайн** спайка (архитектура, компоненты, потоки данных)
3. **Написать дизайн-документ** спайка
4. **Получить одобрение** на реализацию спайка

**Приоритет:** P2 (после IDEA-002/003, см. `ideas/UI-IDEAS.md`)
**Условие старта:** H2 закрыт — спайк можно начинать
**Дедлайн:** 31.12.2026 (если спайк не начнётся — документ удаляется или откладывается)

---

## 7. Ссылки

- `ideas/UI-IDEAS.md` — IDEA-004 (исходная идея)
- `docs/DESIGN_DOC.md` — нормативная спецификация архитектуры (§0–§17)
- `docs/adr/0021-plugin-isolation-process.md` — изоляция плагинов (отдельный процесс)
- `docs/adr/0022-viewprovider-contract.md` — контракт ViewProvider (два тира)
- `docs/ROADMAP.md` — дорожная карта (H1/H2/H3)
- `docs/proto/FINDINGS.md` — результаты прототипов
