// linkController.ts — связка inspector + scrollsync над парой панелей
// (DESIGN_DOC §5.5). Оба читают один и тот же RenderIndex (Фаза 4); контроллер
// владеет их жизненным циклом и даёт единую точку для main.ts.
//
// Хосты (editor/preview) статичны: элементы живут всё время приложения, при
// выключении предпросмотра панель лишь скрывается (§paneHost), а scrollsync сам
// безопасно пропускает синхронизацию, когда у панели нулевая высота (AC-9).

import { createInspector, type Inspector } from "./inspector";
import { createScrollSync, type ScrollSync } from "./scrollsync";
import type { RenderIndex } from "./renderIndex";

export interface LinkController {
  /** Инспектор: включение/выключение, активность, реакция на выделение. */
  readonly inspector: Inspector;
  /** Индекс рендера изменился (перестроен previewView или сброшен при правке). */
  onIndexChanged(): void;
  setSyncEnabled(on: boolean): void;
}

export function createLinkController(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  index: RenderIndex;
  statusEl: HTMLElement;
}): LinkController {
  let scrollSync: ScrollSync;
  const inspector = createInspector({
    editor: opts.editor,
    preview: opts.preview,
    statusEl: opts.statusEl,
    index: opts.index,
    // Программный scrollIntoView инспектора не должен тянуть вторую панель.
    beforeScrollIntoView: () => scrollSync.suspend(),
  });
  scrollSync = createScrollSync({
    editor: opts.editor,
    preview: opts.preview,
    index: opts.index,
  });

  return {
    inspector,
    onIndexChanged() {
      inspector.onIndexChanged();
      scrollSync.onIndexChanged();
    },
    setSyncEnabled(on: boolean) {
      scrollSync.setEnabled(on);
    },
  };
}
