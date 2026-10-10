# ROADMAP — darmark

> Дорожная карта реализации. Нормативная архитектура — `docs/DESIGN_DOC.md`; трассировка решений
> и замеров — §16 DESIGN_DOC и `docs/adr/*`. ADR по решениям предшествуют реализации.

---

## H1 — ядро приложения (1–2 месяца), без плагинного кода

1. **Фаза 0:** DESIGN_DOC + контракты (список e2e «нельзя ломать» — §13.2).
2. **Фаза 1:** Rust `state.rs`/`error.rs`, `open/new/save/close_document`; TS `ids.ts`, `tauri.ts`.
3. **Фаза 2:** `update_document`/`render_document`; `renderSeq` и `lastRenderedHtml` удалены из `main.ts`.
4. **Фаза 3:** TS `docStore.ts` (чистый, без DOM) + `editorView.ts`.
5. **Фаза 4:** `viewRegistry.ts`, `ViewContext`-фасад, `previewView.ts` (тир 1).
6. **Фаза 5:** `layout.ts` + `paneHost.ts` (1–2 панели); inspector/scrollsync → `linkController`.
7. **Фаза 6:** `main.ts` = композиционный корень (~120 строк); README + ADR.
8. **Переименование в `darmark`** (D8): `tauri.conf.json` (`identifier`, `productName`), `Cargo.toml`, `package.json`, `%APPDATA%`-путь, e2e-фикстуры и стартовый текст.
9. **CI + size-gate** — до конца H1.

**Гейт каждой фазы:** рабочее приложение + `cargo test` + `npm run build` + соответствующие e2e.

---

## H2 — плагинная система (2–3 месяца)

1. `plugin-api` (Lua-биндинги host-функций) + child-хост (`mlua`, набор StdLib, Job Object, per-call watchdog).
2. Манифест, сканирование `plugins/`, валидация, permissions-UI.
3. Событийная шина, обратная маршрутизация тир-1.
4. Эталонные плагины: `word-count`, `export-html`, `format-selection`.
5. Менеджер плагинов в UI: включить/выключить/перезагрузить/разрешения.
6. `darmark --debug-plugin <path>` (stdio-транспорт, dev).

---

## H3 — экосистема (4+ недель) — ЧАСТИЧНО СДЕЛАНА

> **Статус:** Фаза 1 (шаблон + `test`) и Фаза 2a (локальный каталог) — сделаны; Фаза 2b (удалённый
> реестр) отложена на неопределённый срок; Фаза 3 (расширение API) — итеративно, не отдельная фаза;
> Фаза 4 — разобрана (подсветка → идеи IDEA-014; Rust-undo — планируется после стабилизации).
> Детали и статус — ниже; решения — ADR-0026/0027/0028.

1. Реестр плагинов — **Фаза 2 закрыта**: **2a (локальный каталог) сделана** — `plugins/index.json`
   + команды CLI `darmark-plugin list/install/link/remove/update` (ADR-0026, `docs/PLUGIN_API.md` §6);
   **2b (удалённый реестр с Ed25519) — отложена на неопределённый срок** (отдельный ADR, номер по
   назначению, при реальной потребности).
2. Template-репозиторий плагина + `darmark plugin test <path>` — **Фаза 1 сделана**:
   `crates/plugin-cli` (`darmark-plugin init` — шаблон; `darmark-plugin test` — манифест/entry/песочница).
3. Расширение API — **итеративно по реальным запросам** (не отдельная фаза); до публичного
   распространения — зафиксировать политику совместимости `HOST_API_VERSION`.
4. Rust-undo (D5) — **планируется** (решение D5 принято), после стабилизации, с дизайном/ADR
   (rev-семантика). Подсветка синтаксиса — в идеях (`IDEA-014`), когда-нибудь (отдельный ADR).
