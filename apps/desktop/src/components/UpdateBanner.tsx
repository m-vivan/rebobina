import type { Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { api } from "../api";
import { findUpdate, installUpdate } from "../updater";

/** De quanto em quanto tempo procura versão nova com o app aberto. */
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/** Aviso de versão nova, com botão para atualizar e reiniciar. */
export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [progress, setProgress] = useState<number | null | undefined>(undefined);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    async function look() {
      try {
        const found = await findUpdate();
        if (alive) setUpdate(found);
      } catch {
        // Sem internet, sem release ou fora do Tauri: tenta de novo mais tarde.
      }
    }
    void look();
    const timer = setInterval(look, CHECK_EVERY_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  // Não reinicia o app no meio de uma gravação.
  useEffect(() => {
    if (!update) return;
    let alive = true;
    const poll = () =>
      api
        .getRecordingStatus()
        .then((s) => alive && setRecording(!!s))
        .catch(() => undefined);
    void poll();
    const timer = setInterval(poll, 2000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [update]);

  if (!update || dismissed) return null;

  const installing = progress !== undefined;

  async function onInstall() {
    if (!update) return;
    setError(null);
    setProgress(null);
    try {
      await installUpdate(update, setProgress);
    } catch (e) {
      setError(String(e));
      setProgress(undefined);
    }
  }

  return (
    <div className="update-banner" role="status">
      <span className="update-banner-text">
        {installing ? (
          <>Atualizando para a versão {update.version}… {progress != null ? `${progress}%` : ""}</>
        ) : (
          <>
            Nova versão do Rebobina: <strong>{update.version}</strong>
          </>
        )}
      </span>
      {error && <span className="error">{error}</span>}
      {!installing && (
        <div className="row gap">
          <button
            className="btn small"
            type="button"
            onClick={onInstall}
            disabled={recording}
            title={recording ? "Termine a gravação para atualizar" : undefined}
          >
            {recording ? "Atualizar depois da gravação" : "Atualizar e reiniciar"}
          </button>
          <button className="btn ghost small" type="button" onClick={() => setDismissed(true)}>
            Agora não
          </button>
        </div>
      )}
    </div>
  );
}
