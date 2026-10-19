import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api";
import type { Meeting, Summary, Transcript } from "../types";
import {
  copyText,
  formatDate,
  formatDuration,
  parseActionItems,
  parseJsonArray,
  parseSegments,
  parseSummaryDetails,
  speakerLabel,
  statusLabel,
  transcriptToText,
} from "../utils";

type Tab = "resumo" | "actions" | "decisions" | "transcript";
type Copied = "resumo" | "transcript" | null;

export function MeetingDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [transcript, setTranscript] = useState<Transcript | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("resumo");
  const [error, setError] = useState<string | null>(null);
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draftTitle, setDraftTitle] = useState("");
  const [draftDescription, setDraftDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [copied, setCopied] = useState<Copied>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);

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

  const details = useMemo(() => parseSummaryDetails(summary?.detailsJson), [summary]);
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

  const suggestedTitle = details?.suggested_title?.trim();
  const showSuggestion =
    !!suggestedTitle && !!meeting && suggestedTitle !== meeting.title && !editing;
  const overviewParagraphs = (summary?.overview ?? "")
    .split(/\n{2,}/)
    .map((p) => p.trim())
    .filter(Boolean);

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

  async function onRegenerate() {
    if (!id) return;
    setRetrying(true);
    setError(null);
    try {
      await api.regenerateSummary(id);
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

  function startEditing() {
    if (!meeting) return;
    setDraftTitle(meeting.title);
    setDraftDescription(meeting.description ?? "");
    setEditing(true);
  }

  async function saveDetails(title: string, description: string | null) {
    if (!id) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await api.updateMeeting(id, title, description);
      setMeeting(updated);
      setEditing(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function onSubmitEdit(e: FormEvent) {
    e.preventDefault();
    void saveDetails(draftTitle, draftDescription);
  }

  async function onDelete() {
    if (!id) return;
    setDeleting(true);
    setError(null);
    try {
      await api.deleteMeeting(id);
      navigate("/");
    } catch (e) {
      setError(String(e));
      setDeleting(false);
      setConfirmDelete(false);
    }
  }

  async function onCopy(kind: Exclude<Copied, null>) {
    const text =
      kind === "resumo"
        ? (summary?.rawMarkdown ?? "")
        : transcriptToText(segments, transcript?.fullText ?? "");
    try {
      await copyText(text);
      setCopied(kind);
      setTimeout(() => setCopied((c) => (c === kind ? null : c)), 2000);
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

  const busy = meeting.status === "processing";

  return (
    <div className="page">
      <header className="page-header">
        <div className="meeting-head">
          <Link className="btn ghost" to="/">
            ← Suas reuniões
          </Link>

          {editing ? (
            <form className="edit-form" onSubmit={onSubmitEdit}>
              <label>
                Nome da reunião
                <input
                  id="edit-title"
                  value={draftTitle}
                  onChange={(e) => setDraftTitle(e.target.value)}
                  autoFocus
                />
              </label>
              <label>
                <span>
                  Descrição <span className="optional">(opcional)</span>
                </span>
                <textarea
                  id="edit-description"
                  rows={3}
                  value={draftDescription}
                  onChange={(e) => setDraftDescription(e.target.value)}
                  placeholder="Ex.: Conversa com o Guilherme sobre a vaga de DevOps. Ela também ajuda o resumo a entender o contexto."
                />
              </label>
              <div className="row gap">
                <button className="btn primary" type="submit" disabled={saving}>
                  {saving ? "Salvando…" : "Salvar"}
                </button>
                <button
                  className="btn ghost"
                  type="button"
                  onClick={() => setEditing(false)}
                  disabled={saving}
                >
                  Cancelar
                </button>
              </div>
            </form>
          ) : (
            <>
              <div className="title-row">
                <h1>{meeting.title}</h1>
                <button className="btn ghost small" type="button" onClick={startEditing}>
                  Editar
                </button>
              </div>
              <p className="muted">
                {formatDate(meeting.startedAt)} ·{" "}
                {formatDuration(meeting.durationSec)} ·{" "}
                <span className={`pill status-${meeting.status}`}>
                  {statusLabel(meeting.status)}
                </span>
              </p>
              {meeting.description ? (
                <p className="meeting-description">{meeting.description}</p>
              ) : (
                <button className="link-button" type="button" onClick={startEditing}>
                  + Adicionar descrição
                </button>
              )}
            </>
          )}

          {showSuggestion && (
            <div className="suggestion">
              <span>
                Sugestão de nome: <strong>{suggestedTitle}</strong>
              </span>
              <button
                className="btn small"
                type="button"
                disabled={saving}
                onClick={() => saveDetails(suggestedTitle, meeting.description)}
              >
                Usar este nome
              </button>
            </div>
          )}
        </div>

        <div className="row gap header-actions">
          {(meeting.status === "error" || busy) && (
            <button
              className="btn"
              type="button"
              onClick={onRetry}
              disabled={retrying || busy}
            >
              {busy ? "Processando…" : retrying ? "Reenviando…" : "Tentar de novo"}
            </button>
          )}
          {transcript && meeting.status === "ready" && (
            <button
              className="btn"
              type="button"
              onClick={onRegenerate}
              disabled={retrying}
              title="Gera o resumo de novo a partir da transcrição, sem reenviar o áudio"
            >
              Refazer resumo
            </button>
          )}
          <button className="btn primary" type="button" onClick={onExport}>
            Exportar Markdown
          </button>
          {confirmDelete ? (
            <div className="delete-confirm-detail">
              <span>Excluir a reunião e o áudio?</span>
              <button
                className="btn danger"
                type="button"
                onClick={onDelete}
                disabled={deleting}
              >
                {deleting ? "Excluindo…" : "Excluir"}
              </button>
              <button
                className="btn ghost"
                type="button"
                onClick={() => setConfirmDelete(false)}
                disabled={deleting}
              >
                Cancelar
              </button>
            </div>
          ) : (
            <button
              className="btn ghost danger"
              type="button"
              onClick={() => setConfirmDelete(true)}
              disabled={meeting.status === "recording"}
              title={
                meeting.status === "recording"
                  ? "Pare a gravação antes de excluir"
                  : undefined
              }
            >
              Excluir
            </button>
          )}
        </div>
      </header>

      {busy && (
        <div className="banner">
          Rebobinando: transcrevendo e resumindo com a OpenAI. Isso pode levar
          alguns minutos.
        </div>
      )}
      {meeting.errorMessage && <div className="banner warn">{meeting.errorMessage}</div>}
      {error && <p className="error">{error}</p>}
      {exportPath && (
        <div className="banner">
          Export salvo em: <code>{exportPath}</code>
        </div>
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
            ["actions", "Tarefas"],
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
              <div className="summary">
                <div className="row between">
                  <h2>Visão geral</h2>
                  <button className="btn small" type="button" onClick={() => onCopy("resumo")}>
                    {copied === "resumo" ? "Copiado" : "Copiar resumo"}
                  </button>
                </div>
                {overviewParagraphs.map((p, i) => (
                  <p key={i}>{p}</p>
                ))}

                {details?.participants && details.participants.length > 0 && (
                  <>
                    <h3>Participantes</h3>
                    <ul className="chips">
                      {details.participants.map((p, i) => (
                        <li key={`${p.name}-${i}`}>
                          {p.name}
                          {p.role ? <span className="muted"> · {p.role}</span> : null}
                        </li>
                      ))}
                    </ul>
                  </>
                )}

                {details?.key_points && details.key_points.length > 0 && (
                  <>
                    <h3>Pontos principais</h3>
                    {details.key_points.map((kp, i) => (
                      <div key={`${kp.topic}-${i}`} className="key-point">
                        <h4>{kp.topic}</h4>
                        <ul>
                          {kp.details.map((d, j) => (
                            <li key={j}>{d}</li>
                          ))}
                        </ul>
                      </div>
                    ))}
                  </>
                )}

                {details?.open_questions && details.open_questions.length > 0 && (
                  <>
                    <h3>Perguntas em aberto</h3>
                    <ul>
                      {details.open_questions.map((q, i) => (
                        <li key={i}>{q}</li>
                      ))}
                    </ul>
                  </>
                )}

                {details?.next_steps && details.next_steps.length > 0 && (
                  <>
                    <h3>Próximos passos</h3>
                    <ul>
                      {details.next_steps.map((n, i) => (
                        <li key={i}>{n}</li>
                      ))}
                    </ul>
                  </>
                )}

                {!details?.key_points?.length && topics.length > 0 && (
                  <>
                    <h3>Tópicos</h3>
                    <ul>
                      {topics.map((t) => (
                        <li key={t}>{t}</li>
                      ))}
                    </ul>
                  </>
                )}

                {!details && meeting.status === "ready" && transcript && (
                  <p className="muted hint">
                    Este resumo foi feito com a versão antiga. Use{" "}
                    <strong>Refazer resumo</strong> para ter participantes, pontos
                    principais e próximos passos.
                  </p>
                )}
              </div>
            ) : (
              <p className="muted">Resumo ainda não disponível.</p>
            )}
          </>
        )}

        {tab === "actions" && (
          <>
            {actions.length === 0 ? (
              <p className="muted">Nenhuma tarefa identificada.</p>
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
            {transcript && (
              <div className="row between">
                <h2>Transcrição</h2>
                <button className="btn small" type="button" onClick={() => onCopy("transcript")}>
                  {copied === "transcript" ? "Copiado" : "Copiar transcrição"}
                </button>
              </div>
            )}
            {segments.length > 0 ? (
              <div className="transcript">
                {segments.map((s, i) => {
                  const who = speakerLabel(s.speaker);
                  return (
                    <p key={`${s.start}-${i}`}>
                      <span className="ts">{formatDuration(Math.floor(s.start))}</span>
                      <span>
                        {who && (
                          <span className={`speaker speaker-${s.speaker}`}>{who}</span>
                        )}
                        {s.text}
                      </span>
                    </p>
                  );
                })}
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
