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

## H3 — экосистема (4+ недель)

1. Реестр плагинов: git-репозиторий + `index.json` + подпись Ed25519.
2. Template-репозиторий плагина + `darmark plugin test <path>`.
3. Расширение API по реальным запросам.
4. Опционально: Rust-undo (D5), редактор с подсветкой синтаксиса (отдельный ADR).
