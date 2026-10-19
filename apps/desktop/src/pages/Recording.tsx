import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api";
import type { Meeting } from "../types";
import { formatDuration } from "../utils";

export function Recording() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [level, setLevel] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);

  useEffect(() => {
    if (!id) return;
    api
      .getMeeting(id)
      .then((m) => {
        setMeeting(m);
        if (m?.startedAt) {
          const start = new Date(m.startedAt).getTime();
          setElapsed(Math.max(0, Math.floor((Date.now() - start) / 1000)));
        }
      })
      .catch((e) => setError(String(e)));
  }, [id]);

  useEffect(() => {
    const timer = setInterval(() => setElapsed((v) => v + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ level: number; meetingId: string }>("recording-level", (event) => {
      if (!id || event.payload.meetingId !== id) return;
      setLevel(event.payload.level);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [id]);

  async function onStop() {
    setStopping(true);
    setError(null);
    try {
      const m = await api.stopRecording();
      navigate(`/meeting/${m.id}`);
    } catch (e) {
      setError(String(e));
      setStopping(false);
    }
  }

  const barWidth = Math.min(100, Math.round(level * 140));

  return (
    <div className="page recording-page">
      <header className="page-header">
        <Link className="btn ghost" to="/">
          ← Biblioteca
        </Link>
      </header>

      <section className="panel record-live">
        <p className="eyebrow live">Gravando</p>
        <h1>{meeting?.title ?? "Reunião"}</h1>
        <p className="timer">{formatDuration(elapsed)}</p>

        <div className="vu" aria-label="Nível de áudio">
          <div className="vu-fill" style={{ width: `${barWidth}%` }} />
        </div>
        <p className="muted vu-hint">
          {level < 0.01
            ? "Sinal baixo — fale no microfone e confira se a reunião está audível. Permissão de Gravação de tela está ativa?"
            : "Recebendo áudio (mic + sistema)"}
        </p>

        {error && <p className="error">{error}</p>}

        <button
          className="btn danger large"
          type="button"
          onClick={onStop}
          disabled={stopping}
        >
          {stopping ? "Parando…" : "Parar e processar"}
        </button>
      </section>
    </div>
  );
}
