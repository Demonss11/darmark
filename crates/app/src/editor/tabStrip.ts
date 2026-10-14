// Лента вкладок документов.

import { confirm } from "@tauri-apps/plugin-dialog";
import { asDocumentId, type DocumentId } from "../core/ids";
import type { DocStore, Tab } from "../doc/docStore";

export interface TabStrip {
  render(tabs: Tab[], activeId: DocumentId | null): void;
  dispose(): void;
}

export function createTabStrip(opts: {
  strip: HTMLElement;
  panes: HTMLElement;
  store: DocStore;
}): TabStrip {
  const { strip, panes, store } = opts;

  function fileIcon(name: string): string {
    const dot = name.lastIndexOf(".");
    const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
    switch (ext) {
      case "md":
      case "markdown":
      case "mdown":
      case "mkd":
        return "M↓";
      case "txt":
        return "T";
      default:
        return "•";
    }
  }

  async function closeTabWithConfirm(id: DocumentId): Promise<void> {
    const tab = store.tabs().find((t) => t.id === id);
    if (!tab) return;
    if (tab.dirty) {
      const ok = await confirm(`«${tab.name}» — несохранённые изменения будут потеряны. Закрыть?`, {
        title: "ПЕНАЛ",
        kind: "warning",
      });
      if (!ok) return;
    }
    await store.closeTab(id);
  }

  function createTabElement(tab: Tab): HTMLElement {
    const el = document.createElement("div");
    el.className = "tab";
    el.dataset.id = tab.id;
    el.id = `tab-${tab.id}`;
    el.setAttribute("role", "tab");
    el.setAttribute("aria-controls", "panes");
    el.setAttribute("aria-label", tab.name);
    el.tabIndex = -1;

    const ficon = document.createElement("span");
    ficon.className = "ficon";
    ficon.setAttribute("aria-hidden", "true");
    el.appendChild(ficon);

    const name = document.createElement("span");
    name.className = "name";
    el.appendChild(name);

    const close = document.createElement("button");
    close.className = "close";
    close.type = "button";
    close.title = "Закрыть";
    close.tabIndex = -1;
    close.textContent = "×";
    el.appendChild(close);

    el.addEventListener("click", (e) => {
      if (e.target === close) return;
      store.switchTab(tab.id);
      el.focus();
    });
    close.addEventListener("click", (e) => {
      e.stopPropagation();
      void closeTabWithConfirm(tab.id);
    });

    return el;
  }

  function updateTabElement(el: HTMLElement, tab: Tab, activeId: DocumentId | null): void {
    const isActive = tab.id === activeId;
    el.setAttribute("aria-selected", String(isActive));
    el.tabIndex = isActive ? 0 : -1;

    const ficon = el.querySelector(".ficon");
    if (ficon) ficon.textContent = fileIcon(tab.name);

    const name = el.querySelector(".name");
    if (name) name.textContent = tab.name;

    let dirty = el.querySelector(".dirty");
    if (tab.dirty && !dirty) {
      dirty = document.createElement("span");
      dirty.className = "dirty";
      dirty.setAttribute("aria-hidden", "true");
      const close = el.querySelector(".close");
      if (close) el.insertBefore(dirty, close);
      else el.appendChild(dirty);
    } else if (!tab.dirty && dirty) {
      dirty.remove();
    }

    const close = el.querySelector(".close") as HTMLElement | null;
    if (close) close.setAttribute("aria-label", `Закрыть ${tab.name}`);
  }

  function syncTabPanel(activeId: DocumentId | null): void {
    if (activeId) {
      panes.setAttribute("aria-labelledby", `tab-${activeId}`);
    } else {
      panes.removeAttribute("aria-labelledby");
    }
  }

  function render(tabs: Tab[], activeId: DocumentId | null): void {
    const existing = new Map<string, HTMLElement>();
    for (const el of Array.from(strip.children) as HTMLElement[]) {
      const id = el.dataset.id;
      if (id) existing.set(id, el);
    }

    const seen = new Set<string>();
    for (const tab of tabs) {
      seen.add(tab.id);
      let el = existing.get(tab.id);
      if (!el) {
        el = createTabElement(tab);
        strip.appendChild(el);
      }
      updateTabElement(el, tab, activeId);
    }

    for (const [id, el] of existing) {
      if (!seen.has(id)) el.remove();
    }

    syncTabPanel(activeId);
  }

  function onKeyDown(e: KeyboardEvent): void {
    const tabs = Array.from(strip.querySelectorAll<HTMLElement>(".tab"));
    if (tabs.length === 0) return;
    const current = document.activeElement as HTMLElement | null;
    const currentTab = (current?.closest(".tab") ?? null) as HTMLElement | null;
    if (!currentTab) return;
    const currentIndex = tabs.indexOf(currentTab);
    if (currentIndex < 0) return;

    const wantsClose =
      e.key === "Delete" ||
      ((e.ctrlKey || e.metaKey) && e.code === "KeyW");
    if (wantsClose) {
      e.preventDefault();
      const id = currentTab.dataset.id;
      if (!id) return;
      void closeTabWithConfirm(asDocumentId(id)).then(() => {
        const activeId = store.activeId();
        if (activeId) {
          const next = strip.querySelector<HTMLElement>(`.tab[data-id="${activeId}"]`);
          next?.focus();
        }
      });
      return;
    }

    let targetIndex = currentIndex;
    if (e.key === "ArrowLeft") targetIndex = (currentIndex - 1 + tabs.length) % tabs.length;
    else if (e.key === "ArrowRight") targetIndex = (currentIndex + 1) % tabs.length;
    else if (e.key === "Home") targetIndex = 0;
    else if (e.key === "End") targetIndex = tabs.length - 1;
    else return;

    e.preventDefault();
    const target = tabs[targetIndex];
    const id = target.dataset.id;
    if (id) store.switchTab(asDocumentId(id));
    target.focus();
  }

  strip.addEventListener("keydown", onKeyDown);

  return {
    render,
    dispose(): void {
      strip.removeEventListener("keydown", onKeyDown);
    },
  };
}
