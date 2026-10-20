import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api } from "../api";
import type { Meeting, RecordingStatus } from "../types";
import { formatDuration } from "../utils";

/**
 * Faixa fixa que aparece em qualquer tela enquanto há uma gravação ativa,
 * para que ela nunca fique rodando sem um jeito de abrir ou parar.
 */
export function RecordingBanner() {
  const location = useLocation();
  const navigate = useNavigate();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    let currentId: string | null = null;
    async function check() {
      try {
        const next = await api.getRecordingStatus();
        if (!alive) return;
        setStatus(next);
        if (!next) {
          currentId = null;
          setMeeting(null);
          return;
        }
        if (next.meetingId !== currentId) {
          currentId = next.meetingId;
          const m = await api.getMeeting(next.meetingId);
          if (alive) setMeeting(m);
        }
      } catch {
        // Fora do Tauri (ex.: preview no navegador) não há gravação para mostrar.
      }
    }
    void check();
    const poll = setInterval(check, 1000);
    return () => {
      alive = false;
      clearInterval(poll);
    };
  }, [location.pathname]);

  if (!meeting || !status || location.pathname === `/recording/${meeting.id}`) {
    return null;
  }

  async function onTogglePause() {
    setError(null);
    try {
      setStatus(status?.paused ? await api.resumeRecording() : await api.pauseRecording());
    } catch (e) {
      setError(String(e));
    }
  }

  async function onStop() {
    setStopping(true);
    setError(null);
    try {
      const m = await api.stopRecording();
      setMeeting(null);
      navigate(`/meeting/${m.id}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setStopping(false);
    }
  }

  return (
    <div className={`recording-banner ${status.paused ? "is-paused" : ""}`} role="status">
      <span className="recording-banner-dot" aria-hidden />
      <span className="recording-banner-text">
        {status.paused ? "Pausada" : "Gravando"} <strong>{meeting.title}</strong>
        <span className="recording-banner-time">{formatDuration(status.elapsedSec)}</span>
      </span>
      {error && <span className="error">{error}</span>}
      <div className="row gap">
        <button
          className="btn ghost"
          type="button"
          onClick={() => navigate(`/recording/${meeting.id}`)}
        >
          Abrir
        </button>
        <button className="btn ghost" type="button" onClick={onTogglePause}>
          {status.paused ? "Retomar" : "Pausar"}
        </button>
        <button
          className="btn danger"
          type="button"
          onClick={onStop}
          disabled={stopping}
        >
          {stopping ? "Parando…" : "Parar e rebobinar"}
        </button>
      </div>
    </div>
  );
}
