import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../api";
import { CassetteAnimation } from "../components/CassetteAnimation";
import { SPEECH_THRESHOLD, SpeechMeter } from "../components/SpeechMeter";
import { FOLLOW_SYSTEM, useAudioDevices } from "../hooks/useAudioDevices";
import type { Meeting, MicSwitched } from "../types";
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
  // Qual botão de parar foi usado ("save" = só salvar; "process" = rebobinar).
  const [stopping, setStopping] = useState<"save" | "process" | null>(null);
  const [paused, setPaused] = useState(false);
  const [toggling, setToggling] = useState(false);
  const { devices, refreshDevices } = useAudioDevices(4000);
  // Microfone em uso na gravação (separado da escolha automática do hook).
  const [activeMic, setActiveMic] = useState<string>("");
  const [switchingMic, setSwitchingMic] = useState(false);
  // Microfone que está gravando de fato (vem do backend).
  const [currentMic, setCurrentMic] = useState<string | null>(null);
  const [micNotice, setMicNotice] = useState<MicSwitched | null>(null);

  useEffect(() => {
    if (!id) return;
    api
      .getMeeting(id)
      .then((m) => {
        setMeeting(m);
        setActiveMic((prev) => prev || m?.deviceName || FOLLOW_SYSTEM);
      })
      .catch((e) => setError(String(e)));
  }, [id]);

  const switchingRef = useRef(false);

  // O app trocou de microfone sozinho (padrão do macOS mudou ou voz em outro mic).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<MicSwitched>("mic-switched", (event) => {
      if (!id || event.payload.meetingId !== id) return;
      setMicNotice(event.payload);
      setCurrentMic(event.payload.to);
      setActiveMic(event.payload.reason === "system" ? FOLLOW_SYSTEM : event.payload.to);
      setLastHeardAt(Date.now());
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [id]);

  // Tempo e pausa vêm do backend, que desconta o tempo pausado.
  useEffect(() => {
    let alive = true;
    async function sync() {
      try {
        const status = await api.getRecordingStatus();
        if (!alive || !status || status.meetingId !== id) return;
        setElapsed(status.elapsedSec);
        setPaused(status.paused);
        setCurrentMic(status.micName);
        if (!switchingRef.current) {
          setActiveMic(status.followSystem ? FOLLOW_SYSTEM : (status.micName ?? FOLLOW_SYSTEM));
        }
      } catch {
        // Fora do Tauri não há gravação para acompanhar.
      }
    }
    void sync();
    const timer = setInterval(sync, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [id]);

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
    switchingRef.current = true;
    setError(null);
    setMicNotice(null);
    try {
      await api.switchMicrophone(name || null);
    } catch (e) {
      setActiveMic(previous);
      setError(String(e));
    } finally {
      setSwitchingMic(false);
      switchingRef.current = false;
    }
  }

  async function onTogglePause() {
    setToggling(true);
    setError(null);
    try {
      const status = paused ? await api.resumeRecording() : await api.pauseRecording();
      setPaused(status.paused);
      setElapsed(status.elapsedSec);
      if (!status.paused) setLastHeardAt(Date.now());
    } catch (e) {
      setError(String(e));
    } finally {
      setToggling(false);
    }
  }

  async function onStop(process: boolean) {
    setStopping(process ? "process" : "save");
    setError(null);
    try {
      const m = await api.stopRecording(process);
      navigate(`/meeting/${m.id}`);
    } catch (e) {
      setError(String(e));
      setStopping(null);
    }
  }

  const activity = paused ? 0 : Math.max(levels.mic, levels.system);
  const silentForAWhile = !paused && Date.now() - lastHeardAt > 6000;
  const busy = stopping !== null;

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
            {paused ? (
              <p className="eyebrow paused">❚❚ Pausada</p>
            ) : (
              <p className="eyebrow live">● Gravando</p>
            )}
            <h1>{title}</h1>
          </div>
          <p className={`timer ${paused ? "is-paused" : ""}`} aria-label="Tempo gravado">
            {formatDuration(elapsed)}
          </p>
        </div>

        <div className="deck-body">
          <CassetteAnimation
            recording={!paused}
            paused={paused}
            activity={activity}
            label={title}
          />
          <div className="speech-meters" aria-label="Quem está falando">
            <SpeechMeter label="Você" level={paused ? 0 : levels.mic} tick={tick} />
            <SpeechMeter label="Chamada" level={paused ? 0 : levels.system} tick={tick} />
          </div>
          <p className="muted vu-hint">
            {paused
              ? "Gravação pausada. Nada do que for dito agora entra na gravação."
              : silentForAWhile
                ? "Ainda não estamos ouvindo nada. Fale algo ou confira se o som da chamada está ligado."
                : "As barras se mexem quando alguém fala. Tudo está sendo gravado."}
          </p>
        </div>

        {micNotice && (
          <p className="mic-notice" role="status">
            <span>
              {micNotice.reason === "voice" ? (
                <>
                  Sua voz estava chegando em <strong>{micNotice.to}</strong>, não no
                  microfone gravado. Trocamos para ele.
                </>
              ) : (
                <>
                  O microfone do sistema mudou. Agora gravando de{" "}
                  <strong>{micNotice.to}</strong>.
                </>
              )}
            </span>
            <span className="row gap">
              {micNotice.reason === "voice" && (
                <button
                  className="btn small"
                  type="button"
                  onClick={() => void onSwitchMic(micNotice.from ?? FOLLOW_SYSTEM)}
                >
                  Desfazer
                </button>
              )}
              <button className="btn ghost small" type="button" onClick={() => setMicNotice(null)}>
                Ok
              </button>
            </span>
          </p>
        )}

        <div className="deck-controls">
          <label className="mic-picker">
            <span>Microfone</span>
            <select
              id="recording-mic"
              value={activeMic}
              disabled={switchingMic || busy}
              onChange={(e) => void onSwitchMic(e.target.value)}
              onFocus={() => void refreshDevices()}
            >
              <option value={FOLLOW_SYSTEM}>
                Seguir o sistema
                {activeMic === FOLLOW_SYSTEM && currentMic ? ` (agora: ${currentMic})` : ""}
              </option>
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

          <div className="deck-buttons">
            <button
              className={`btn ${paused ? "resume" : "pause"}`}
              type="button"
              onClick={onTogglePause}
              disabled={toggling || busy}
            >
              {paused ? "Retomar" : "Pausar"}
            </button>
            <button
              className="btn"
              type="button"
              onClick={() => onStop(false)}
              disabled={busy}
              title="Salva a gravação sem transcrever. Você pode rebobinar depois."
            >
              {stopping === "save" ? "Parando…" : "Parar"}
            </button>
            <button
              className="btn primary stop large"
              type="button"
              onClick={() => onStop(true)}
              disabled={busy}
              title="Salva, transcreve e resume a reunião"
            >
              {stopping === "process" ? "Parando…" : "Parar e rebobinar"}
            </button>
          </div>
        </div>

        {error && <p className="error">{error}</p>}
      </section>
    </div>
  );
}
