// sidebar.ts — rail + сворачиваемый sidebar (UI-каркас idea5).
//
// Модуль владеет только видимостью: какой контент-панели показан и свёрнут ли
// весь sidebar. Rail-кнопки (кроме журнала-заглушки) кликают сюда; повторный
// клик по активной панели сворачивает sidebar. Домен/файлы здесь не живут.

export type SidebarPanel = "explorer" | "plugins";

export interface Sidebar {
  /** Переключить содержимое панели (sidebar раскрывается). */
  setPanel(panel: SidebarPanel): void;
  /** Повторный клик по активной панели сворачивает sidebar. */
  toggle(panel?: SidebarPanel): void;
  /** Текущая панель и состояние раскрытия — для синхронизации UI (rail). */
  activePanel(): SidebarPanel;
  isOpen(): boolean;
  dispose(): void;
}

export function createSidebar(opts: {
  sidebar: HTMLElement;
  panels: Record<string, HTMLElement>;
  /** Rail-кнопки панелей: сюда пишется активное состояние. */
  rail?: Record<string, HTMLElement>;
}): Sidebar {
  const { sidebar, panels, rail } = opts;

  let current: SidebarPanel = "explorer";
  let open = true;

  function render(): void {
    sidebar.classList.toggle("collapsed", !open);
    for (const [name, el] of Object.entries(panels)) {
      el.hidden = !(open && name === current);
    }
    if (rail) {
      for (const [name, btn] of Object.entries(rail)) {
        const active = open && name === current;
        btn.classList.toggle("active", active);
        btn.setAttribute("aria-pressed", String(active));
      }
    }
  }

  function setPanel(panel: SidebarPanel): void {
    current = panel;
    open = true;
    render();
  }

  function toggle(panel?: SidebarPanel): void {
    if (panel && panel !== current) {
      setPanel(panel);
    } else if (panel && panel === current && open) {
      open = false;
      render();
    } else {
      open = true;
      render();
    }
  }

  render();

  return {
    setPanel,
    toggle,
    activePanel: () => current,
    isOpen: () => open,
    dispose(): void {
      // Слушатели rail навешивает shell — здесь освобождать нечего.
    },
  };
}
