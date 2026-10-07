Ревью выполнено по коммиту `b612bd9` («h1») — это §H1 Фаза 1 из `docs/ROADMAP.md`.

# Summary

Фаза 1 H1 сделана по плану: `DocumentStore` переехал в Rust (`state.rs`, `error.rs`), IPC-команды `new/open/save/close_document` заменили `read_file`/`write_file`, фронт получил `ids.ts` и типизированные обёртки с `{ code, message }`. Замороженные контракты (§13.2 DESIGN_DOC) не нарушены: id, стартовый текст, сообщение с «МБ», debounce 120 мс на месте, `md-core` не тронут, новых зависимостей и секретов нет. Блокеров не нашёл; есть 5 предложений и мелочи — главные про утечку документов в сторе и про то, что `save_document` правит состояние до успешной записи на диск.

Проверить командами не смог: сессия read-only, `cargo test`/`cargo check`/`npm run build` не запускаются. Выводы — статический анализ диффа и чтение кода; в отчёте автора гейты заявлены зелёными.

# Findings

## Suggestion

**1. Документы не закрываются — утечка в `DocumentStore`**
`crates/app/src/main.ts:176` и `:200` (`applySnapshot(await newDocument(""))`, `applySnapshot(await openDocument(selected))`), обёртка `closeDocument` в `crates/app/src/tauri.ts:43` не вызывается нигде.

Что не так: каждый «Новый»/«Открыть» создаёт в сторе новый документ, а предыдущий остаётся с текстом (до 10 МБ) и в `order`. `close_document` есть, но фронт его не зовёт.
Почему важно: память растёт на каждое открытие за сессию; в Фазе 5 вкладки получат «фантомные» документы, которые пользователь не открывал.
Как исправить: перед созданием/открытием закрывать текущий, например в `newFile`/`openFile` после успеха —
```ts
if (currentId) void closeDocument(currentId);
applySnapshot(await newDocument(""));
```
либо в Rust: `open_document` переиспользует документ с тем же `path`.

**2. `save_document` меняет стор до фактической записи файла**
`crates/app/src-tauri/src/lib.rs:115-131` — `doc.path = Some(p.clone())` и `doc.text = text.clone()` выполняются в блоке, который отпускает мьютекс ещё до `write_text`.

Что не так: при ошибке записи (нет прав, диск занят, файл под блокировкой) стор уже считает, что у документа новый путь и новый текст.
Почему важно: после неудачного «Сохранить как» `currentPath` в UI остаётся старым, а `doc.path` в Rust — новым. Следующий Ctrl+S пойдёт в путь, который только что не записался, без диалога. Расхождение стора и UI, которое трудно заметить.
Как исправить: сначала писать, потом мутировать:
```rust
let target = { /* только чтение: path из аргумента или doc.path */ };
tauri::async_runtime::spawn_blocking(move || write_text(&target, &body)).await??;
let mut guard = lock_store(&store);
if let Some(doc) = guard.get_mut(&id) {
    if let Some(p) = explicit_path { doc.path = Some(p); }
    doc.text = text;
}
```

**3. Стартовый документ создаётся асинхронно — гонка в e2e `smoke`**
`crates/app/src/main.ts:344-350` (`bootstrap` через `await newDocument(START_TEXT)`) и `crates/app/e2e/steps/smoke.steps.js:11-14`.

Что не так: `#editor` существует с момента разбора HTML, поэтому `waitForExist` проходит мгновенно, а `getValue()` может выполниться до ответа IPC и увидеть пустое поле. Раньше `editor.value` заполнялся синхронно в модуле, окно гонки было меньше.
Почему важно: флейк «поле редактора пусто» на быстрой машине/медленном старте; падение будет выглядеть как регресс продукта.
Как исправить: в шаге дождаться непустого значения —
```js
await browser.waitUntil(async () => (await $("#editor").getValue()).length > 0, { timeout: 8000 });
```
Контракты (§13.2) это не ломает.

**4. `saveFile`/`saveAs` молча выходят без документа**
`crates/app/src/main.ts:210` и `:223` — `if (!currentId) return;`.

Что не так: если `bootstrap` упал на `newDocument` (например, IPC не поднялся), `currentId === null` и Ctrl+S/кнопка «Сохранить» не делают ничего и ничего не сообщают.
Почему важно: пользователь печатает, жмёт Ctrl+S, считает, что сохранил. Тихая потеря работы.
Как исправить: `flash("Документ не создан — сохранение недоступно")` вместо `return`.

**5. `resetRenderState()` в `openFile` стоит до `await`**
`crates/app/src/main.ts:196-200`.

Что не так: между `resetRenderState()` и `applySnapshot(await openDocument(selected))` есть окно, в котором ввод в редактор порождает рендер старого текста; `applySnapshot` затем ставит `dirty = false`, стирая факт правки.
Почему важно: редкий, но реальный сценарий потери правки и мигания предпросмотра старым HTML. Паттерн унаследован от старого `readFile`, но строки переписаны — сейчас удобно починить.
Как исправить: гасить debounce и `renderSeq++` сразу после `applySnapshot`, а не до него; либо блокировать ввод до ответа IPC.

**6. Коммит смешивает две задачи**
`crates/lua-rpc-spike/Cargo.toml`, `crates/lua-rpc-spike/src/lib.rs`, `crates/lua-rpc-spike/src/bin/quarantine_parent.rs`, `crates/lua-rpc-spike/run-d16.ps1`, `docs/adr/0021-plugin-isolation-process.md`, `docs/proto/FINDINGS.md`, `reviews/архив/*F37-F38*`.

Что не так: область `tasks/TZ-H1.md` — `crates/app/src-tauri/*` и `crates/app/src/*`, а в том же коммите лежит закрытие F37/F38 из `TZ-proto-lua-host` (карантин, `Quarantine`, `permission_notices`). Сам код прототипа выглядит согласованным (`threshold.max(1)`, сброс серии успехом, `permission_notices` покрыт тестом), претензий к нему нет.
Почему важно: ревью и откат одной задачи тянут за собой другую; `git log` по H1 перестаёт быть трассируемым.
Как исправить: разносить такие изменения отдельными коммитами (или отдельной веткой от `main`), H1 — только `crates/app/*` и правки `KODA.md`/`AGENTS.md`/`tasks/TZ-H1.md`.

## Nice to have

**7. `AGENTS.md` — сломанная разметка**
`AGENTS.md:57` — `**прочитай целиком** соответствующий \`tasks/TZ-*.md` без закрывающего бэктика и без точки: незакрытый inline-code ломает остаток строки. `AGENTS.md:4` — висячий пробел в конце строки (появился в этом диффе). Восстановить закрывающий бэктик и убрать пробел.

**8. `capabilities/default.json:4` описывает удалённые команды**
В `description` осталось «свои команды read_file/write_file/render_markdown доступны по умолчанию». Команд `read_file`/`write_file` больше нет — заменить на `new_document`/`open_document`/`save_document`/`close_document`/`render_markdown`.

**9. Артефакт сборки в git**
`crates/app/dist/index.html` (в диффе — новый хэш `index-kxWOlTxP.js`) при `dist/` в `.gitignore`. Ссылается на бандл, которого в репо нет, и обновляется на каждой сборке. `git rm --cached crates/app/dist/index.html`; на сборку не влияет — `frontendDist` пересобирается через `beforeBuildCommand`.

**10. Дублирование конструктора документа**
`crates/app/src-tauri/src/state.rs:128-137` и `:148-157` — `create` и `insert_loaded` отличаются только `path`, и оба делают `get(&id).expect("документ только что вставлен")` сразу после `insert`. Короче и без `expect`:
```rust
let snapshot = doc.snapshot();
self.order.push(id.clone());
self.docs.insert(id, doc);
snapshot
```

**11. Мёртвые заделы под будущие фазы**
`crates/app/src/ids.ts:14-21` (`asDocumentId`, `asPaneId`, `asViewId`, `newPaneId`, `newViewId`) и `closeDocument` в `tauri.ts` не используются нигде. ТЗ §3.3 их требует, `.kodarules` — «не создавай неиспользуемое»; конфликт решён в пользу ТЗ, но `noUnusedLocals` такие экспорты не поймает, так что мёртвый код проживёт до Фаз 3–4. Стоит держать в списке техдолга рядом с `#[allow(dead_code)]` в `state.rs`.

**12. Копирование текста на каждом открытии/сохранении**
`state.rs:104-118` (`snapshot()` клонирует `text`) плюс `lib.rs:130` (`doc.text = text.clone()`). На документе 10 МБ это лишние 10 МБ на каждый open и два клона на save. Для Фазы 1 допустимо; при переходе на `update_document` отдать текст в снапшот по значению, без клона.
