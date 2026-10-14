# План: декомпозиция UI (третий проход)

**Goal:** уменьшить размер крупных модулей фронтенда `crates/app/src`, разложив самостоятельные хелперы/типы/константы по подмодулям, перенести оставшиеся верхнеуровневые файлы в доменные подпапки и разбить `style.css` — без изменения поведения.

**Architecture:** каждый крупный файл `X.ts` превращается в папку `X/` с `index.ts` (сохраняет путь импорта `./X` для потребителей) и подмодулями по ответственности. Публичный API модулей не меняется.

**Tech Stack:** vanilla TS, Vite, Tauri 2.

**Spec:** этот документ (бэклог-направления A/B/C согласованы с исполнителем).

## Global Constraints

- **Комментарии:** только один шапко-блок в начале файла. Внутри тела — ни одного комментария.
- **Поведение не меняется.** Публичные экспорты каждого модуля сохраняются.
- **Кодировка — только UTF-8 без BOM.** Правки делаются Python-скриптами, НЕ PowerShell `Get-Content` (он читает в ANSI и портит кириллицу).
- **Проверка каждой задачи:** `cd crates/app; npx tsc --noEmit` — чисто; `npm run build` — успех; `npm run knip` — exit 0.
- **Коммит на задачу**, стейджить через `git add -A`.
- Движение в порядке A → B → C. E2E (`npm run test:e2e`) — один раз в конце, перед финальным коммитом.

---

## Task 1: aiPanel → panels/aiPanel/

**Files:**
- Move: `src/panels/aiPanel.ts` → `src/panels/aiPanel/index.ts`
- Create: `src/panels/aiPanel/dom.ts`, `constants.ts`, `errors.ts`, `diff.ts`, `skills.ts`, `pluginProposal.ts`

**Интерфейсы (что выносится):**
- `dom.ts`: `byId`, `must`, `el`, `actionButton`, `nowTime`, `shortModel`
- `constants.ts`: `MAX_DOC_BYTES`, `STICK_THRESHOLD`, `AiDotState`
- `errors.ts`: `ErrorKind`, `ErrorAction`, `ERROR_TITLES`, `errorTitle`, `errorKind`, `errorCode`
- `diff.ts`: `MAX_DIFF_LINES`, `splitLines`, `appendDiffLine`, `renderDiffBody` (импорт `el` из `./dom`, `buildUnitMaps`/`bytesToUnits` из `../../core/mapping`, `AiDiff` из `../../core/tauri`)
- `skills.ts`: `FALLBACK_SKILLS` (импорт `AiSkill` из `../../core/tauri`)
- `pluginProposal.ts`: `AiPluginProposal`, `PluginCardState`, `PLUGIN_STATUS_LABELS`, `PLUGIN_FENCE`, `extractPluginProposal`, `parsePluginBlock`, `confirmDeletePlugin` (импорт `openDialog`/`Dialog` из `../../ui/dialog`, `PluginStatus` из `../../core/tauri`, `Json` из `../../viewRegistry`, `el`/`actionButton` из `./dom`)
- `index.ts`: `createAiPanel`, `AiPanel`, `AiPanelOptions`, `PLUGIN_SKILL_IDS`; относительные пути пересчитаны (`../core/*` → `../../core/*`, `../ui/dialog` → `../../ui/dialog`, `./logPanel` → `../logPanel`).

**Критерий:** файл `index.ts` ≤ ~1000 строк, `createAiPanel` не изменён; tsc/build/knip чисто.

## Task 2: pluginManager → plugin/pluginManager/

**Files:** Move `src/plugin/pluginManager.ts` → `src/plugin/pluginManager/index.ts`; создать подмодули.
**Вынести:** `labels.ts` (`STATUS_LABELS`, `PERMISSION_LABELS`, `permissionLabel`, `statusMessage`, `stateHint`, `documentBoundaryNotice`, `QUARANTINE_THRESHOLD`), `settings.ts` (`settingValue`, `renderSettingRow`), `icons.ts` (`infoIcon`, `chevronIcon`, `shieldIcon`), `render.ts` (`renderIdentity`, `renderInfoButton`, `renderInfoPopover`, `renderExpandButton`, `el`, `infoPopoverId`, `bodyId`).
**Критерий:** tsc/build/knip чисто.

## Task 3: logPanel → panels/logPanel/

**Files:** Move `src/panels/logPanel.ts` → `src/panels/logPanel/index.ts`; `types.ts` (`LogLevel`, `LogCategory`, `DockMode`, `DockState`, `LogPanelOptions`, `LogEntry`), `helpers.ts` (`byId`, `dayKey`, `clampHeight`, константы `MAX_ENTRIES`/`MIN_HEIGHT`/`MAX_HEIGHT`/`STICK_THRESHOLD`/`DAY_FORMATTER`).
**Критерий:** tsc/build/knip чисто. Потребители (`aiPanel`, `resourceMonitor`, `shell`, `main`) импортируют типы из `./logPanel` — путь сохраняется через `index.ts` (реэкспорт типов).

## Task 4: resourceMonitor → panels/resourceMonitor/

**Files:** Move → `src/panels/resourceMonitor/index.ts`; `metrics.ts` (типы `ProcessMetrics`/`MainMetrics`/`ChildMetrics`/`ResourceSnapshot`/`MonState`/`OrphanEntry`), `constants.ts` (все пороги), `format.ts` (`mbValue`, `formatMb`).
**Критерий:** tsc/build/knip чисто.

## Task 5: tables → panels/tables/

**Files:** Move → `src/panels/tables/index.ts`; `parse.ts` (`parseNumber`, `parseDate`, `toCellValue`, `detectColumnKind`, `cellText`, `headerCells`, `bodyRows`), `compare.ts` (`compareCells`, `rowMatches`, `apply`, регэкспы `NUM_RE`/`DATE_RE`), `menu.ts` (`MenuController`, `createMenuController`, `makeTools`).
**Критерий:** tsc/build/knip чисто.

## Task 6: inspector → panels/inspector/

**Files:** Move → `src/panels/inspector/index.ts`; `target.ts` (`INTERACTIVE_SELECTOR`, `InspectMode`, `InspectTarget`, `resolveTarget`).
**Критерий:** tsc/build/knip чисто.

## Task 7: docStore → doc/docStore/

**Files:** Move → `src/doc/docStore/index.ts`; `types.ts` (`Tab`, `DocStoreOptions`, `DocStore`), `helpers.ts` (`errorCode`, `baseName`, `makeTab`, `DEBOUNCE_MS`).
**Критерий:** tsc/build/knip чисто. Потребители (`editorView`, `statusBar`, `fileActions`, `main`) импортируют из `./doc/docStore` — путь сохраняется.

## Task 8: palette → ui/palette/

**Files:** Move → `src/ui/palette/index.ts`; `types.ts` (`PaletteCommand`, `CommandProvider`, `PaletteOptions`, `PaletteController`), `origin.ts` (`originLabel`).
**Критерий:** tsc/build/knip чисто.

## Task 9: tauri → core/tauri/

**Files:** Move `src/core/tauri.ts` → `src/core/tauri/index.ts`; разложить по доменам: `documents.ts`, `plugins.ts`, `ai.ts`, `settings.ts` (по группам IPC-команд), общий `errors.ts` (`errorMessage`) и `types.ts`.
**Критерий:** публичные реэкспорты сохранены (`index.ts` реэкспортирует всё, что раньше экспортировал `tauri.ts`), потребители не меняются. tsc/build/knip чисто.

## Task 10: main.ts — декомпозиция композитора

**Files:** `src/main.ts` — вынести в `src/app/` чистые фазы инициализации (напр. `context.ts` — сборка ViewContext; `commands.ts` — список команд палитры), оставив в `main.ts` оркестрацию. Дизайн уточняется на месте по фактической структуре.
**Критерий:** поведение сохранено, tsc/build/knip/e2e чисто.

## Task 11 (B): перенос оставшихся верхнеуровневых файлов

**Files:** `fileActions.ts`, `formatActions.ts`, `images.ts`, `linkController.ts`, `previewView.ts`, `sampleDocument.ts`, `statusBar.ts`, `viewRegistry.ts`.
**Куда:** `src/views/` (`previewView`, `viewRegistry`), `src/editor/` (`fileActions`, `formatActions`, `statusBar`, `linkController`), `src/core/` (`images`), `src/doc/` (`sampleDocument`). Точный расклад согласовать на месте по связям.
**Критерий:** импорты обновлены, tsc/build/knip чисто.

## Task 12 (C): разбить style.css

**Files:** `src/style.css` (2541 строка) → `src/styles/` с `base.css`, `layout.css`, `editor.css`, `preview.css`, `panels.css`, `plugin.css`, `ui.css`, `themes.css`; `style.css` собирает через `@import`.
**Критерий:** визуально идентично (e2e темы/базовый UI зелёные), билд собирает CSS без ошибок.
