import { getVersion } from "@tauri-apps/api/app";
import type { Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { findUpdate, installUpdate } from "../updater";

/** Seção das Configurações: versão instalada e busca manual de atualização. */
export function UpdateSection() {
  const [version, setVersion] = useState<string | null>(null);
  const [update, setUpdate] = useState<Update | null>(null);
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null | undefined>(undefined);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => undefined);
  }, []);

  async function onCheck() {
    setChecking(true);
    setMessage(null);
    try {
      const found = await findUpdate();
      setUpdate(found);
      if (!found) setMessage("Você já está na versão mais recente.");
    } catch (e) {
      setMessage(`Não foi possível procurar atualização: ${String(e)}`);
    } finally {
      setChecking(false);
    }
  }

  async function onInstall() {
    if (!update) return;
    setMessage(null);
    setProgress(null);
    try {
      await installUpdate(update, setProgress);
    } catch (e) {
      setMessage(`Não foi possível atualizar: ${String(e)}`);
      setProgress(undefined);
    }
  }

  return (
    <section className="panel">
      <h2>Atualizações</h2>
      <p className="muted">
        Versão instalada: <strong>{version ?? "—"}</strong>. O Rebobina procura
        versões novas sozinho e avisa no topo da tela.
      </p>
      <div className="row gap">
        {update ? (
          <button className="btn primary" type="button" onClick={onInstall} disabled={progress !== undefined}>
            {progress !== undefined
              ? `Atualizando… ${progress != null ? `${progress}%` : ""}`
              : `Atualizar para ${update.version} e reiniciar`}
          </button>
        ) : (
          <button className="btn" type="button" onClick={onCheck} disabled={checking}>
            {checking ? "Procurando…" : "Procurar atualização"}
          </button>
        )}
      </div>
      {message && <p className="muted">{message}</p>}
    </section>
  );
}
