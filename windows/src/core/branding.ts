export const DEFAULT_APP_NAME = "Coucou";
let name = DEFAULT_APP_NAME;
export function cleanAppName(value: unknown): string {
  if (typeof value !== "string") return DEFAULT_APP_NAME;
  const cleaned = value.trim().replace(/\s+/gu, " ");
  return cleaned && [...cleaned].length <= 32 && /^[\p{L}\p{N} ._-]+$/u.test(cleaned)
    && /[\p{L}\p{N}]/u.test(cleaned) ? cleaned : DEFAULT_APP_NAME;
}
export function appName(): string { return name; }
export function setAppName(value: unknown): boolean {
  const next = cleanAppName(value); const changed = next !== name; name = next;
  if (typeof document !== "undefined") document.title = name;
  return changed;
}
export function brandText(text: string): string { return text.replace(/\bCoucou\b/g, () => name); }
