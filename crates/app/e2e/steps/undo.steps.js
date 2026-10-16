// Шаги undo/redo документа (D5).
//
// Владелец истории — Rust-стор; фронт лишь шлёт `undo_document`/`redo_document` и
// применяет результат. Нативная история textarea отключена, а `editorView` слушает
// `keydown` именно на `#editor` и различает сочетание по `e.code` (независимо от
// раскладки). Поэтому хоткей проверяем синтетическим keydown прямо на textarea —
// в отличие от `я нажимаю хоткей` (window без `code`), которым управляют shell.ts.
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

/// Дописывает текст в редактор и генерирует `input` — тот же путь, что у реального
/// набора. Значение читаем из DOM: редактор и есть буфер ввода (D5).
When("я дописываю в редактор {string}", async (addition) => {
  await browser.execute((text) => {
    const el = document.getElementById("editor");
    el.value = el.value + text;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, addition);
});

/// Синтетический Ctrl+Z/Ctrl+Y на `#editor`. Сочетание задаётся как "Ctrl+Z".
/// `code` важен: editorView различает клавишу по `e.code` ("KeyZ"/"KeyY").
When("в редакторе я нажимаю хоткей {string}", async (spec) => {
  const parts = spec.split("+");
  const key = parts[parts.length - 1];
  await browser.execute(
    (k, ctrl) => {
      document.getElementById("editor").dispatchEvent(
        new KeyboardEvent("keydown", {
          code: `Key${k}`,
          key: k.toLowerCase(),
          ctrlKey: ctrl,
          bubbles: true,
          cancelable: true,
        })
      );
    },
    key,
    parts.includes("Ctrl")
  );
  await browser.pause(50);
});

/// Ожидание пропадания фрагмента в редакторе: undo/redo меняют буфер асинхронно
/// (IPC → стор → подписка editorView), поэтому однократное чтение дало бы ложный
/// результат.
Then("в редакторе нет {string}", async (fragment) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (f) => !(document.getElementById("editor")?.value ?? "").includes(f),
        fragment
      ),
    { timeout: 10000, timeoutMsg: `В редакторе осталось: ${fragment}` }
  );
});

/// Ожидание пропадания фрагмента в предпросмотре: после undo/redo стор перерисовывает
/// HTML из нового текста (`reload` → `render_document`), это тоже асинхронный путь.
Then("в предпросмотре не осталось {string}", async (fragment) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (f) => !(document.getElementById("preview")?.innerHTML ?? "").includes(f),
        fragment
      ),
    { timeout: 10000, timeoutMsg: `В предпросмотре осталось: ${fragment}` }
  );
});
