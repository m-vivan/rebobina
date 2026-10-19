import type { ActionItem, TranscriptSegment } from "./types";

export function formatDuration(sec: number | null | undefined): string {
  if (sec == null || Number.isNaN(sec)) return "—";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function formatDate(iso: string): string {
  try {
    return new Intl.DateTimeFormat("pt-BR", {
      dateStyle: "short",
      timeStyle: "short",
    }).format(new Date(iso));
  } catch {
    return iso;
  }
}

export function parseJsonArray<T>(raw: string, fallback: T[] = []): T[] {
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as T[]) : fallback;
  } catch {
    return fallback;
  }
}

export function parseSegments(raw: string): TranscriptSegment[] {
  return parseJsonArray<TranscriptSegment>(raw);
}

export function parseActionItems(raw: string): ActionItem[] {
  return parseJsonArray<ActionItem>(raw);
}

export function statusLabel(status: string): string {
  switch (status) {
    case "recording":
      return "Gravando";
    case "processing":
      return "Rebobinando";
    case "ready":
      return "Pronto";
    case "error":
      return "Erro";
    default:
      return status;
  }
}
