





interface Gutter {
  update(): void;
  syncScroll(): void;
  dispose(): void;
}

export function createGutter(editor: HTMLTextAreaElement, host: HTMLElement): Gutter {
  function update(): void {
    const lines = editor.value.split("\n").length;
    let out = "1";
    for (let i = 2; i <= lines; i++) out += "\n" + i;
    host.textContent = out;
    syncScroll();
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
