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
        <p className="eyebrow">Setup de áudio</p>
        <h2>Permissão de Gravação de tela</h2>
        <p className="muted">
          O Rebobina captura o áudio do sistema pelo{" "}
          <strong>ScreenCaptureKit</strong> da Apple — sem drivers extras. O
          macOS pede permissão de Gravação de tela (nenhum vídeo é salvo).
        </p>
        <ul className="checklist">
          <li>Clique em “Permitir” e aceite o diálogo do macOS.</li>
          <li>
            Se não aparecer: Ajustes → Privacidade → Gravação de tela → Rebobina.
          </li>
          <li>Reinicie o app depois de ativar a permissão.</li>
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
            {busy ? "Aguarde…" : "Permitir captura"}
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
