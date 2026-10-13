# Исправление рассинхрона предпросмотра при возврате на вкладку — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Устранить баг P1: при возврате на ранее отрендеренную вкладку редактор показывает новый документ, а предпросмотр остаётся со старым HTML.

**Architecture:** `RenderResult.changed` вычисляется в хосте **per-document** (`Document.cached`, ключ `(mapped, rev)`), а `preview.innerHTML` — один общий на приложение. При возврате на документ A его кэш совпадает → `changed:false` → `previewView.applyRender` не трогает DOM, хотя в DOM лежит HTML документа B. Фикс — в единственной точке применения (`previewView.applyRender`): применять, если `res.changed` **или** `res.html` отличается от фактически отображённого HTML. Хост не меняем (его per-document кэш — валидный контракт; `html` всегда передаётся в `RenderResult`).

**Tech Stack:** TypeScript (vanilla, strict), Vite, Tauri 2; E2E — Cucumber + WebdriverIO.

**Spec:** Диагностика тестировщика (раунд Space Bunny Free) + root-cause в `crates/app/src-tauri/src/state.rs` и `crates/app/src/previewView.ts`. Отдельный ADR не требуется (баг-фикс без смены контрактов).

## Global Constraints

- `previewView.applyRender` — **единственное** место `preview.innerHTML = html` (ТЗ-H1, AGENTS). Не вводить второе.
- `RenderResult` всегда содержит `html: string`, `rev: u64`, `changed: bool` — контракт не менять.
- Существующие Rust-тесты `crates/app/src-tauri/src/state.rs` менять запрещено; хост не трогаем.
- Строгий TS: `cd crates/app; npm run build` (`tsc && vite build`) обязан проходить.
- E2E: UI-проверка требует пересборки release-бинарника (`npx tauri build --no-bundle`) — бинарник эмбедит фронтенд.
- Комментарии на русском, отступ 2 пробела; `rustfmt`/`clippy` для Rust (не затрагивается).

---

### Task 1: Применять HTML в предпросмотре, когда он отличается от отображённого

**Files:**
- Modify: `crates/app/src/previewView.ts:72-87` (функция `applyRender` в `createPreviewView`)

**Interfaces:**
- Consumes: `RenderResult { html: string; rev: number; changed: boolean }` из `crates/app/src/docStore.ts`.
- Produces: ничего нового наружу; поведение `applyRender` — «применить, если изменилось».

- [ ] **Step 1: Зафиксировать последний применённый HTML**

В `createPreviewView` (рядом с другими локальными переменными замыкания вида, перед `function applyRender`) добавить:

```ts
    // HTML, реально лежащий в `previewEl.innerHTML`. `res.changed` хоста
    // считается per-document, а предпросмотр один на всё приложение: при
    // возврате на ранее отрендеренную вкладку `changed` может быть `false`,
    // хотя в DOM остался HTML другого документа. Поэтому применяем ещё и по
    // фактическому различию HTML.
    let lastHtml = "";
```

- [ ] **Step 2: Изменить условие применения в `applyRender`**

Заменить текущее тело условия:

```ts
    function applyRender(res: RenderResult, source: string): void {
      if (res.changed) {
        previewEl.innerHTML = res.html;
```

на:

```ts
    function applyRender(res: RenderResult, source: string): void {
      if (res.changed || res.html !== lastHtml) {
        previewEl.innerHTML = res.html;
        lastHtml = res.html;
```

Остальную часть тела (`try { tables.enhance(); } catch ...`, `resolveLocalImages`, `index.set`, `o.onIndexed`) оставить без изменений.

- [ ] **Step 3: Сборка (типы + бандл)**

Run: `cd crates/app; npm run build`
Expected: `tsc` без ошибок, `vite build` — успех.

- [ ] **Step 4: Коммит**

```bash
git add crates/app/src/previewView.ts
git commit -m "fix(preview): обновлять предпросмотр при возврате на вкладку"
```

---

### Task 2: Активировать E2E-регрессию на рассинхрон

**Files:**
- Modify: `crates/app/e2e/features/tabs-aria.feature` (сценарий, помеченный `@wip`, воспроизводящий рассинхрон)

**Interfaces:**
- Consumes: фикс из Task 1 (в release-бинарнике).
- Produces: зелёный регрессионный сценарий; пересобранный `target/release/darmark.exe`.

- [ ] **Step 1: Снять тег `@wip` с регрессионного сценария**

Найти в `crates/app/e2e/features/tabs-aria.feature` сценарий про рассинхрон редактора и предпросмотра при возврате на вкладку (помечен `@wip`) и убрать тег `@wip`, оставив остальные теги сценария.

- [ ] **Step 2: Пересобрать release-бинарник**

Run: `cd crates/app; npx tauri build --no-bundle`
Expected: сборка успешна; `target/release/darmark.exe` обновлён (mtime — новее правки `previewView.ts`).

- [ ] **Step 3: Прогон регрессионного сценария**

Run: `cd crates/app; npx wdio run e2e/wdio.conf.js --spec e2e/features/tabs-aria.feature`
Expected: PASS (ранее сценарий падал).

- [ ] **Step 4: Полный прогон E2E**

Run: `cd crates/app; npm run test:e2e`
Expected: все фичи PASS (0 failing).

- [ ] **Step 5: Коммит**

```bash
git add crates/app/e2e/features/tabs-aria.feature
git commit -m "test(e2e): регрессия на рассинхрон предпросмотра при возврате на вкладку"
```
