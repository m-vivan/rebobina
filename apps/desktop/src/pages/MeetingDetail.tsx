import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../api";
import type { Meeting, Summary, Transcript } from "../types";
import {
  formatDate,
  formatDuration,
  parseActionItems,
  parseJsonArray,
  parseSegments,
  statusLabel,
} from "../utils";

type Tab = "resumo" | "actions" | "decisions" | "transcript";

export function MeetingDetail() {
  const { id } = useParams<{ id: string }>();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [transcript, setTranscript] = useState<Transcript | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("resumo");
  const [error, setError] = useState<string | null>(null);
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);

  const load = useCallback(async () => {
    if (!id) return;
    const [m, t, s] = await Promise.all([
      api.getMeeting(id),
      api.getTranscript(id),
      api.getSummary(id),
    ]);
    setMeeting(m);
    setTranscript(t);
    setSummary(s);
    if (m?.audioPath) {
      const path = await api.getAudioPath(id);
      setAudioUrl(path ? convertFileSrc(path) : null);
    }
  }, [id]);

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, [load]);

  useEffect(() => {
    let unlistenOk: (() => void) | undefined;
    let unlistenErr: (() => void) | undefined;
    listen<string>("meeting-processed", (event) => {
      if (event.payload === id) {
        load().catch((e) => setError(String(e)));
      }
    }).then((fn) => {
      unlistenOk = fn;
    });
    listen<{ meetingId: string; error: string }>(
      "meeting-process-error",
      (event) => {
        if (event.payload.meetingId === id) {
          setError(event.payload.error);
          load().catch(() => undefined);
        }
      },
    ).then((fn) => {
      unlistenErr = fn;
    });
    return () => {
      unlistenOk?.();
      unlistenErr?.();
    };
  }, [id, load]);

  const decisions = useMemo(
    () => (summary ? parseJsonArray<string>(summary.decisionsJson) : []),
    [summary],
  );
  const topics = useMemo(
    () => (summary ? parseJsonArray<string>(summary.topicsJson) : []),
    [summary],
  );
  const actions = useMemo(
    () => (summary ? parseActionItems(summary.actionItemsJson) : []),
    [summary],
  );
  const segments = useMemo(
    () => (transcript ? parseSegments(transcript.segmentsJson) : []),
    [transcript],
  );

  async function onRetry() {
    if (!id) return;
    setRetrying(true);
    setError(null);
    try {
      await api.retryProcessing(id);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setRetrying(false);
    }
  }

  async function onExport() {
    if (!id) return;
    try {
      const path = await api.saveExportFile(id);
      setExportPath(path);
    } catch (e) {
      setError(String(e));
    }
  }

  if (!meeting) {
    return (
      <div className="page">
        <p className="muted">Carregando reunião…</p>
        {error && <p className="error">{error}</p>}
      </div>
    );
  }

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <Link className="btn ghost" to="/">
            ← Biblioteca
          </Link>
          <h1>{meeting.title}</h1>
          <p className="muted">
            {formatDate(meeting.startedAt)} ·{" "}
            {formatDuration(meeting.durationSec)} ·{" "}
            <span className={`pill status-${meeting.status}`}>
              {statusLabel(meeting.status)}
            </span>
          </p>
        </div>
        <div className="row gap">
          {(meeting.status === "error" || meeting.status === "processing") && (
            <button
              className="btn"
              type="button"
              onClick={onRetry}
              disabled={retrying || meeting.status === "processing"}
            >
              {meeting.status === "processing"
                ? "Processando…"
                : retrying
                  ? "Reenviando…"
                  : "Tentar de novo"}
            </button>
          )}
          <button className="btn primary" type="button" onClick={onExport}>
            Exportar Markdown
          </button>
        </div>
      </header>

      {meeting.status === "processing" && (
        <div className="banner">
          Transcrevendo e resumindo com a OpenAI… Isso pode levar alguns minutos.
        </div>
      )}
      {meeting.errorMessage && (
        <div className="banner warn">{meeting.errorMessage}</div>
      )}
      {error && <p className="error">{error}</p>}
      {exportPath && (
        <div className="banner">Export salvo em: <code>{exportPath}</code></div>
      )}

      {audioUrl && (
        <section className="panel">
          <h2>Áudio</h2>
          <audio controls src={audioUrl} className="audio-player" />
        </section>
      )}

      <div className="tabs">
        {(
          [
            ["resumo", "Resumo"],
            ["actions", "Action items"],
            ["decisions", "Decisões"],
            ["transcript", "Transcrição"],
          ] as const
        ).map(([key, label]) => (
          <button
            key={key}
            type="button"
            className={tab === key ? "tab active" : "tab"}
            onClick={() => setTab(key)}
          >
            {label}
          </button>
        ))}
      </div>

      <section className="panel">
        {tab === "resumo" && (
          <>
            {summary ? (
              <>
                <h2>Visão geral</h2>
                <p>{summary.overview}</p>
                {topics.length > 0 && (
                  <>
                    <h3>Tópicos</h3>
                    <ul>
                      {topics.map((t) => (
                        <li key={t}>{t}</li>
                      ))}
                    </ul>
                  </>
                )}
              </>
            ) : (
              <p className="muted">Resumo ainda não disponível.</p>
            )}
          </>
        )}

        {tab === "actions" && (
          <>
            {actions.length === 0 ? (
              <p className="muted">Nenhum action item identificado.</p>
            ) : (
              <ul className="action-list">
                {actions.map((a, i) => (
                  <li key={`${a.task}-${i}`}>
                    <strong>{a.task}</strong>
                    <span className="muted">
                      {a.assignee ? ` · ${a.assignee}` : ""}
                      {a.due ? ` · prazo ${a.due}` : ""}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}

        {tab === "decisions" && (
          <>
            {decisions.length === 0 ? (
              <p className="muted">Nenhuma decisão registrada.</p>
            ) : (
              <ul>
                {decisions.map((d) => (
                  <li key={d}>{d}</li>
                ))}
              </ul>
            )}
          </>
        )}

        {tab === "transcript" && (
          <>
            {segments.length > 0 ? (
              <div className="transcript">
                {segments.map((s, i) => (
                  <p key={`${s.start}-${i}`}>
                    <span className="ts">
                      {formatDuration(Math.floor(s.start))}
                    </span>
                    {s.text}
                  </p>
                ))}
              </div>
            ) : transcript ? (
              <p className="transcript-plain">{transcript.fullText}</p>
            ) : (
              <p className="muted">Transcrição ainda não disponível.</p>
            )}
          </>
        )}
      </section>
    </div>
  );
}
