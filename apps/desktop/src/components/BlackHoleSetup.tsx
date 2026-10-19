import { useEffect, useState } from "react";
import { api } from "../api";
import type { BlackHoleStatus } from "../types";

const DISMISS_KEY = "gravador.blackhole.dismissed";

export function BlackHoleSetup() {
  const [status, setStatus] = useState<BlackHoleStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await api.getBlackHoleStatus();
        if (cancelled) return;
        setStatus(s);
        const dismissed = localStorage.getItem(DISMISS_KEY) === "1";
        // Mostra se não há device ativo (instalação incompleta ou precisa ativar).
        if (!s.deviceActive && !dismissed) {
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

  if (!open || !status || status.deviceActive) {
    return null;
  }

  const needsActivation = status.installed && !status.deviceActive;

  async function onInstall() {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const result = await api.installBlackHole();
      setMessage(result.message);
      setStatus(await api.getBlackHoleStatus());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onActivate() {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const result = await api.activateBlackHole();
      setMessage(result.message);
      const s = await api.getBlackHoleStatus();
      setStatus(s);
      if (s.deviceActive) {
        setTimeout(() => setOpen(false), 1200);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onOpenSite() {
    try {
      await api.openBlackHoleDownload();
    } catch (e) {
      setError(String(e));
    }
  }

  function onDismiss() {
    localStorage.setItem(DISMISS_KEY, "1");
    setOpen(false);
  }

  async function onRecheck() {
    setBusy(true);
    setError(null);
    try {
      const s = await api.getBlackHoleStatus();
      setStatus(s);
      setMessage(s.hint);
      if (s.deviceActive) {
        setTimeout(() => setOpen(false), 1200);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal panel">
        <p className="eyebrow">Setup de áudio</p>
        <h2>
          {needsActivation ? "Ativar BlackHole" : "Instalar BlackHole"}
        </h2>
        <p className="muted">{status.hint}</p>
        {needsActivation && (
          <ul className="checklist">
            <li>
              Driver no disco:{" "}
              <code>{status.driverPath ?? "detectado"}</code>
            </li>
            <li>
              “Ativar agora” reinicia o Core Audio (pede senha de administrador).
            </li>
            <li>Se ainda não aparecer, reinicie o Mac.</li>
          </ul>
        )}

        {message && <p className="ok">{message}</p>}
        {error && <p className="error">{error}</p>}

        <div className="row gap" style={{ marginTop: 16 }}>
          {needsActivation ? (
            <button
              className="btn primary"
              type="button"
              disabled={busy}
              onClick={onActivate}
            >
              {busy ? "Aguarde…" : "Ativar agora"}
            </button>
          ) : (
            <button
              className="btn primary"
              type="button"
              disabled={busy}
              onClick={onInstall}
            >
              {busy
                ? "Aguarde…"
                : status.brewAvailable
                  ? "Instalar BlackHole"
                  : "Abrir instalador oficial"}
            </button>
          )}
          {status.brewAvailable && (
            <button className="btn ghost" type="button" onClick={onOpenSite}>
              Site oficial
            </button>
          )}
          <button
            className="btn"
            type="button"
            disabled={busy}
            onClick={onRecheck}
          >
            Verificar de novo
          </button>
          <button className="btn ghost" type="button" onClick={onDismiss}>
            Depois
          </button>
        </div>
      </div>
    </div>
  );
}
