// linkController.ts — связка inspector + scrollsync над парой панелей
// (DESIGN_DOC §5.5). Оба читают один и тот же RenderIndex; контроллер владеет их
// жизненным циклом и даёт единую точку для main.ts.
//
// Хосты (editor/preview) статичны: при выключении предпросмотра панель лишь
// скрывается (§paneHost), а scrollsync безопасно пропускает синхронизацию при
// нулевой высоте панели (AC-9).

import { createInspector, type Inspector } from "./panels/inspector";
import { createScrollSync, type ScrollSync } from "./layout/scrollsync";
import type { RenderIndex } from "./doc/renderIndex";

interface LinkController {
  readonly inspector: Inspector;
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
