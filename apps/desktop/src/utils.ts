import type { ActionItem, SummaryDetails, TranscriptSegment } from "./types";

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

export function parseSummaryDetails(raw: string | null | undefined): SummaryDetails | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as SummaryDetails) : null;
  } catch {
    return null;
  }
}

export function speakerLabel(speaker: TranscriptSegment["speaker"]): string | null {
  if (speaker === "me") return "Você";
  if (speaker === "others") return "Participante";
  return null;
}

/** Transcrição em texto puro, com tempo e quem fala, para copiar. */
export function transcriptToText(segments: TranscriptSegment[], fallback: string): string {
  if (segments.length === 0) return fallback;
  return segments
    .map((s) => {
      const who = speakerLabel(s.speaker);
      return `[${formatDuration(Math.floor(s.start))}] ${who ? `${who}: ` : ""}${s.text}`;
    })
    .join("\n");
}

/** Copia texto; usa um textarea temporário se a API de clipboard falhar. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    if (!ok) throw new Error("Não foi possível copiar");
  }
}
