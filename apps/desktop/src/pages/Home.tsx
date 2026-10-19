import { useCallback, useEffect, useState, type FormEvent, type MouseEvent } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../api";
import { BrandMark } from "../components/BrandMark";
import { useAudioDevices } from "../hooks/useAudioDevices";
import type { Meeting } from "../types";
import { formatDate, formatDuration, statusLabel } from "../utils";

export function Home() {
  const navigate = useNavigate();
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [query, setQuery] = useState("");
  const [title, setTitle] = useState("");
  const { devices, deviceName, setDeviceName, refreshDevices } =
    useAudioDevices(2000);
  const [hasKey, setHasKey] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [starting, setStarting] = useState(false);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const refresh = useCallback(async (q?: string) => {
    const list = await api.listMeetings(q);
    setMeetings(list);
  }, []);

  useEffect(() => {
    (async () => {
      try {
        setHasKey(await api.hasApiKey());
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, [refresh]);

  useEffect(() => {
    const t = setTimeout(() => {
      refresh(query).catch((e) => setError(String(e)));
    }, 250);
    return () => clearTimeout(t);
  }, [query, refresh]);

  async function onStart(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setStarting(true);
    try {
      const meeting = await api.startRecording(title, deviceName || null);
      navigate(`/recording/${meeting.id}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setStarting(false);
    }
  }

  function askDelete(e: MouseEvent, id: string) {
    e.preventDefault();
    e.stopPropagation();
    setConfirmId(id);
    setError(null);
  }

  async function confirmDelete(e: MouseEvent, id: string) {
    e.preventDefault();
    e.stopPropagation();
    setDeleting(true);
    setError(null);
    try {
      await api.deleteMeeting(id);
      setMeetings((prev) => prev.filter((m) => m.id !== id));
      setConfirmId(null);
      await refresh(query);
    } catch (err) {
      setError(String(err));
    } finally {
      setDeleting(false);
    }
  }

  return (
    <div className="page">
      <header className="page-header">
        <div className="brand-lockup">
          <BrandMark size={60} />
          <div className="brand-copy">
            <h1 className="brand-name">rebobina</h1>
            <p className="brand-tag">Aperte REC na reunião. Depois é só rebobinar.</p>
          </div>
        </div>
        <Link className="btn ghost" to="/settings">
          Configurações
        </Link>
      </header>

      {!hasKey && (
        <div className="banner warn">
          Configure sua API key da OpenAI em{" "}
          <Link to="/settings">Configurações</Link> para transcrever e resumir.
        </div>
      )}

      <section className="panel record-panel">
        <h2>Nova fita</h2>
        <p className="muted">
          Microfone + áudio do sistema via ScreenCaptureKit. Na primeira vez,
          autorize <strong>Gravação de tela</strong> — nenhum vídeo é salvo.
        </p>
        <form className="record-form" onSubmit={onStart}>
          <label>
            Título
            <input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Standup, 1:1, planning…"
            />
          </label>
          <label>
            Microfone
            <select
              value={deviceName}
              onChange={(e) => setDeviceName(e.target.value)}
              onFocus={() => {
                void refreshDevices();
              }}
              onMouseDown={() => {
                void refreshDevices();
              }}
            >
              {devices.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name}
                  {d.isDefault ? " (padrão)" : ""}
                </option>
              ))}
            </select>
          </label>
          <button className="btn primary rec" type="submit" disabled={starting}>
            {starting ? "Iniciando…" : "REC"}
          </button>
        </form>
      </section>

      <section className="panel">
        <div className="row between">
          <h2>Suas fitas</h2>
          <input
            className="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Buscar título ou transcript…"
          />
        </div>

        {loading && <p className="muted">Carregando…</p>}
        {error && <p className="error">{error}</p>}

        {!loading && meetings.length === 0 && (
          <p className="muted">Nenhuma fita ainda. Aperte REC na próxima call.</p>
        )}

        <ul className="meeting-list">
          {meetings.map((m) => (
            <li key={m.id} className="meeting-row">
              <Link to={`/meeting/${m.id}`} className="meeting-main">
                <strong>{m.title}</strong>
                <span className="muted meta">
                  {formatDate(m.startedAt)} · {formatDuration(m.durationSec)}
                </span>
              </Link>
              <span className={`pill status-${m.status}`}>
                {statusLabel(m.status)}
              </span>
              {confirmId === m.id ? (
                <div className="row gap delete-confirm">
                  <button
                    className="btn danger"
                    type="button"
                    disabled={deleting}
                    onClick={(e) => confirmDelete(e, m.id)}
                  >
                    {deleting ? "Apagando…" : "Confirmar"}
                  </button>
                  <button
                    className="btn ghost"
                    type="button"
                    disabled={deleting}
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      setConfirmId(null);
                    }}
                  >
                    Cancelar
                  </button>
                </div>
              ) : (
                <button
                  className="btn ghost danger"
                  type="button"
                  onClick={(e) => askDelete(e, m.id)}
                >
                  Apagar
                </button>
              )}
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
