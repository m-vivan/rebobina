import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api";
import { CassetteAnimation } from "../components/CassetteAnimation";
import { SPEECH_THRESHOLD, SpeechMeter } from "../components/SpeechMeter";
import { useAudioDevices } from "../hooks/useAudioDevices";
import type { Meeting } from "../types";
import { formatDuration } from "../utils";

export function Recording() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [elapsed, setElapsed] = useState(0);
  // Nível suavizado de cada faixa: sobe na hora e desce aos poucos.
  const [levels, setLevels] = useState({ mic: 0, system: 0 });
  const [tick, setTick] = useState(0);
  const [lastHeardAt, setLastHeardAt] = useState(() => Date.now());
  const [error, setError] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);
  const { devices, refreshDevices } = useAudioDevices(4000);
  // Microfone em uso na gravação (separado da escolha automática do hook).
  const [activeMic, setActiveMic] = useState<string>("");
  const [switchingMic, setSwitchingMic] = useState(false);

  useEffect(() => {
    if (!id) return;
    api
      .getMeeting(id)
      .then((m) => {
        setMeeting(m);
        setActiveMic((prev) => prev || m?.deviceName || "");
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
    listen<{ level: number; mic?: number; system?: number; meetingId: string }>(
      "recording-level",
      (event) => {
        if (!id || event.payload.meetingId !== id) return;
        const mic = event.payload.mic ?? event.payload.level;
        const system = event.payload.system ?? 0;
        setLevels((prev) => ({
          mic: Math.max(mic, prev.mic * 0.7),
          system: Math.max(system, prev.system * 0.7),
        }));
        setTick((t) => t + 1);
        if (Math.max(mic, system) >= SPEECH_THRESHOLD) setLastHeardAt(Date.now());
      },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [id]);

  async function onSwitchMic(name: string) {
    const previous = activeMic;
    setActiveMic(name);
    setSwitchingMic(true);
    setError(null);
    try {
      await api.switchMicrophone(name || null);
    } catch (e) {
      setActiveMic(previous);
      setError(String(e));
    } finally {
      setSwitchingMic(false);
    }
  }

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

  const activity = Math.max(levels.mic, levels.system);
  const silentForAWhile = Date.now() - lastHeardAt > 6000;

  const title = meeting?.title ?? "Reunião";
  const micMissing = !!activeMic && !devices.some((d) => d.name === activeMic);

  return (
    <div className="page recording-page">
      <header className="page-header">
        <Link className="btn ghost" to="/">
          ← Suas reuniões
        </Link>
      </header>

      <section className="panel deck">
        <div className="deck-head">
          <div className="deck-title">
            <p className="eyebrow live">● Gravando</p>
            <h1>{title}</h1>
          </div>
          <p className="timer" aria-label="Tempo de gravação">
            {formatDuration(elapsed)}
          </p>
        </div>

        <div className="deck-body">
          <CassetteAnimation recording activity={activity} label={title} />
          <div className="speech-meters" aria-label="Quem está falando">
            <SpeechMeter label="Você" level={levels.mic} tick={tick} />
            <SpeechMeter label="Chamada" level={levels.system} tick={tick} />
          </div>
          <p className="muted vu-hint">
            {silentForAWhile
              ? "Ainda não estamos ouvindo nada. Fale algo ou confira se o som da chamada está ligado."
              : "As barras se mexem quando alguém fala. Tudo está sendo gravado."}
          </p>
        </div>

        <div className="deck-controls">
          <label className="mic-picker">
            <span>Microfone</span>
            <select
              id="recording-mic"
              value={activeMic}
              disabled={switchingMic || stopping}
              onChange={(e) => void onSwitchMic(e.target.value)}
              onFocus={() => void refreshDevices()}
            >
              <option value="">Padrão do sistema</option>
              {micMissing && (
                <option value={activeMic}>{activeMic} (desconectado)</option>
              )}
              {devices
                .filter((d) => !d.likelyLoopback)
                .map((d) => (
                  <option key={d.name} value={d.name}>
                    {d.name}
                  </option>
                ))}
            </select>
            {switchingMic && <span className="muted">Trocando…</span>}
          </label>

          <button
            className="btn primary stop large"
            type="button"
            onClick={onStop}
            disabled={stopping}
          >
            {stopping ? "Parando…" : "Parar e rebobinar"}
          </button>
        </div>

        {error && <p className="error">{error}</p>}
      </section>
    </div>
  );
}
