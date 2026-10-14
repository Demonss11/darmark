// toast.ts вЂ” С‚РѕСЃС‚-СѓРІРµРґРѕРјР»РµРЅРёСЏ РІ `#toast-host`.
//
// РќРµРёРЅС‚РµСЂР°РєС‚РёРІРЅС‹Рµ (С…РѕСЃС‚ `pointer-events: none`); Р°РІС‚Рѕ-СЃРєСЂС‹С‚РёРµ; РЅРµ Р±РѕР»РµРµ С‚СЂС‘С…
// РѕРґРЅРѕРІСЂРµРјРµРЅРЅРѕ (СЃР°РјС‹Р№ СЃС‚Р°СЂС‹Р№ СѓР±РёСЂР°РµС‚СЃСЏ). Р’С…РѕРґ/РІС‹С…РѕРґ вЂ” С‚РѕР»СЊРєРѕ `opacity`/
// `transform`, РёС… РіР°СЃРёС‚ РіР»РѕР±Р°Р»СЊРЅС‹Р№ `prefers-reduced-motion`. Live-region вЂ”
// СЃР°Рј С…РѕСЃС‚ (`aria-live="polite"`); РѕС€РёР±РєРё РґРѕРїРѕР»РЅРёС‚РµР»СЊРЅРѕ РЅРµСЃСѓС‚ `role="alert"`,
// С‡С‚РѕР±С‹ РѕР·РІСѓС‡РёРІР°С‚СЊСЃСЏ СЃСЂР°Р·Сѓ (assertive).

export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastOptions {
  kind?: ToastKind;
  /** Р”Р»РёС‚РµР»СЊРЅРѕСЃС‚СЊ РїРѕРєР°Р·Р° РІ РјСЃ; `<= 0` вЂ” В«Р·Р°Р»РёРїР°СЋС‰РёР№В» С‚РѕСЃС‚ Р±РµР· Р°РІС‚Рѕ-СЃРєСЂС‹С‚РёСЏ. */
  duration?: number;
}

const MAX_TOASTS = 3;
const EXIT_MS = 200;

/** РЈР±СЂР°С‚СЊ С‚РѕСЃС‚ СЃ РѕР±СЂР°С‚РЅРѕР№ Р°РЅРёРјР°С†РёРµР№ (РёРґРµРјРїРѕС‚РµРЅС‚РЅРѕ). */
function dismiss(node: HTMLElement): void {
  if (node.dataset.leaving) return;
  node.dataset.leaving = "1";
  node.classList.remove("toast-show");
  window.setTimeout(() => node.remove(), EXIT_MS);
}

/** РџРѕРєР°Р·Р°С‚СЊ С‚РѕСЃС‚; `duration <= 0` вЂ” С‚РѕСЃС‚ РЅРµ РёСЃС‡РµР·Р°РµС‚ СЃР°Рј. */
export function toast(message: string, opts: ToastOptions = {}): void {
  const host = document.getElementById("toast-host");
  if (!host) return;

  const kind: ToastKind = opts.kind ?? "info";
  const node = document.createElement("div");
  node.className = `toast toast-${kind}`;


  if (kind === "error") node.setAttribute("role", "alert");

  host.append(node);


  node.textContent = message;
  void node.offsetWidth;
  node.classList.add("toast-show");

  const all = Array.from(host.querySelectorAll<HTMLElement>(".toast"));
  for (let i = 0; i < all.length - MAX_TOASTS; i++) all[i]?.remove();

  const fallback = kind === "info" || kind === "success" ? 3000 : 5000;
  const duration = opts.duration ?? fallback;
  if (duration > 0) {
    window.setTimeout(() => dismiss(node), duration);
  } else {

    node.classList.add("toast-sticky");
    node.addEventListener("click", () => dismiss(node));
  }
}
