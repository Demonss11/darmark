// Классификация ошибок AI и заголовки карточек.

export type ErrorKind = "auth" | "offline" | "rate" | "context" | "generic";

export interface ErrorAction {
  label: string;
  run(): void;
}

const ERROR_TITLES: Record<ErrorKind, string> = {
  auth: "Неверный API-ключ",
  offline: "Нет соединения",
  rate: "Превышен лимит запросов",
  context: "Документ слишком большой",
  generic: "Ошибка AI",
};

export function errorTitle(kind: ErrorKind, code: string): string {
  if (code === "ai_no_key") return "API-ключ не задан";
  if (code === "ai_no_consent") return "Нужно согласие на отправку";
  if (code === "ai_disabled") return "AI выключен";
  return ERROR_TITLES[kind];
}

export function errorKind(code: string, message: string): ErrorKind {
  const c = code.toLowerCase();
  const m = message.toLowerCase();
  if (c.includes("key") || c.includes("auth") || c.includes("401") || m.includes("401") || m.includes("ключ")) {
    return "auth";
  }
  if (
    c.includes("offline") ||
    c.includes("network") ||
    c.includes("timeout") ||
    m.includes("соедин") ||
    m.includes("network") ||
    m.includes("timeout") ||
    m.includes("офлайн")
  ) {
    return "offline";
  }
  if (c.includes("rate") || m.includes("429") || m.includes("лимит")) return "rate";
  if (c.includes("context") || m.includes("context length") || m.includes("слишком большой")) {
    return "context";
  }
  return "generic";
}

export function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code?: unknown }).code;
    if (typeof code === "string") return code;
  }
  return undefined;
}
