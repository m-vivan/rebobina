import { useEffect, useState } from "react";
import { api } from "../api";
import type { ScreenPermissionStatus } from "../types";

const DISMISS_KEY = "gravador.screen.dismissed";

export function ScreenPermissionSetup() {
  const [status, setStatus] = useState<ScreenPermissionStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await api.getScreenPermission();
        if (cancelled) return;
        setStatus(s);
        const dismissed = localStorage.getItem(DISMISS_KEY) === "1";
        if (!s.granted && !dismissed) {
          setOpen(true);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (!open || !status || status.granted) {
    return null;
  }

  async function onRequest() {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const s = await api.requestScreenPermission();
      setStatus(s);
      if (s.granted) {
        setMessage("Permissão concedida.");
        setTimeout(() => setOpen(false), 800);
      } else {
        setMessage(
          "Ainda sem permissão. Abra os Ajustes, ative o Rebobina e reinicie o app.",
        );
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onOpenSettings() {
    try {
      await api.openScreenSettings();
    } catch (e) {
      setError(String(e));
    }
  }

  function onDismiss() {
    localStorage.setItem(DISMISS_KEY, "1");
    setOpen(false);
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal panel">
        <p className="eyebrow">Primeiro uso</p>
        <h2>Deixe o Rebobina ouvir suas chamadas</h2>
        <p className="muted">
          Para gravar o que as outras pessoas falam, o macOS pede a permissão
          chamada <strong>Gravação de tela</strong>. O Rebobina usa só o som:
          nenhuma imagem da sua tela é salva.
        </p>
        <ul className="checklist">
          <li>Clique em “Permitir” e confirme na janela do macOS.</li>
          <li>
            Se a janela não aparecer, abra os Ajustes e ative o Rebobina em Privacidade → Gravação de tela.
          </li>
          <li>Feche e abra o Rebobina de novo para valer.</li>
        </ul>

        {message && <p className="ok">{message}</p>}
        {error && <p className="error">{error}</p>}

        <div className="row gap" style={{ marginTop: 16 }}>
          <button
            className="btn primary"
            type="button"
            disabled={busy}
            onClick={onRequest}
          >
            {busy ? "Aguarde…" : "Permitir"}
          </button>
          <button className="btn" type="button" onClick={onOpenSettings}>
            Abrir Ajustes
          </button>
          <button className="btn ghost" type="button" onClick={onDismiss}>
            Depois
          </button>
        </div>
      </div>
    </div>
  );
}
