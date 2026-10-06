// gutter.ts — номера строк редактора (UI-каркас idea5).
//
// Сам `#editor` остаётся скролл-элементом; gutter лишь перерисовывает номера и
// подтягивает свой scrollTop за редактором. Геометрия (font-size/line-height/
// padding) задана общими CSS-переменными, иначе номера «поедут».

export interface Gutter {
  /** Перерисовать номера (число строк = `host.value`). */
  update(): void;
  /** Синхронизировать прокрутку gutter с редактором. */
  syncScroll(): void;
  dispose(): void;
}

export function createGutter(editor: HTMLTextAreaElement, host: HTMLElement): Gutter {
  function update(): void {
    const lines = editor.value.split("\n").length;
    let out = "1";
    for (let i = 2; i <= lines; i++) out += "\n" + i;
    host.textContent = out;
    syncScroll(); // textContent сбрасывает scrollTop — возвращаем позицию
  }

  function syncScroll(): void {
    host.scrollTop = editor.scrollTop;
  }

  editor.addEventListener("scroll", syncScroll);
  update();

  return {
    update,
    syncScroll,
    dispose(): void {
      editor.removeEventListener("scroll", syncScroll);
    },
  };
}
