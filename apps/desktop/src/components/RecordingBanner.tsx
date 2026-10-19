import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api } from "../api";
import type { Meeting } from "../types";
import { formatDuration } from "../utils";

/**
 * Faixa fixa que aparece em qualquer tela enquanto há uma gravação ativa,
 * para que ela nunca fique rodando sem um jeito de abrir ou parar.
 */
export function RecordingBanner() {
  const location = useLocation();
  const navigate = useNavigate();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [now, setNow] = useState(Date.now());
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    async function check() {
      try {
        const id = await api.getRecordingState();
        if (!alive) return;
        if (!id) {
          setMeeting(null);
          return;
        }
        setMeeting((prev) => (prev?.id === id ? prev : null));
        const m = await api.getMeeting(id);
        if (alive) setMeeting(m);
      } catch {
        // Fora do Tauri (ex.: preview no navegador) não há gravação para mostrar.
      }
    }
    void check();
    const poll = setInterval(check, 2000);
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      alive = false;
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [location.pathname]);

  if (!meeting || location.pathname === `/recording/${meeting.id}`) {
    return null;
  }

  const elapsed = Math.max(
    0,
    Math.floor((now - new Date(meeting.startedAt).getTime()) / 1000),
  );

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
    <div className="recording-banner" role="status">
      <span className="recording-banner-dot" aria-hidden />
      <span className="recording-banner-text">
        Gravando <strong>{meeting.title}</strong>
        <span className="recording-banner-time">{formatDuration(elapsed)}</span>
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
