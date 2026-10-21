import { useEffect, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { api } from "../api";
import { useAudioDevices } from "../hooks/useAudioDevices";
import type { ScreenPermissionStatus } from "../types";

export function Settings() {
  const [apiKey, setApiKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const { devices, deviceName, setDeviceName, refreshDevices } =
    useAudioDevices(2500);
  const [language, setLanguage] = useState("pt");
  const [autoDetectCalls, setAutoDetectCalls] = useState(true);
  const [vocabulary, setVocabulary] = useState("");
  const [dataDir, setDataDir] = useState("");
  const [screen, setScreen] = useState<ScreenPermissionStatus | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    (async () => {
      try {
        const [settings, key, dir, perm] = await Promise.all([
          api.getSettings(),
          api.hasApiKey(),
          api.getDataDir(),
          api.getScreenPermission(),
        ]);
        setHasKey(key);
        setLanguage(settings.language || "pt");
        setAutoDetectCalls(settings.autoDetectCalls !== false);
        setVocabulary(settings.vocabulary ?? "");
        if (settings.defaultDeviceName) {
          setDeviceName(settings.defaultDeviceName);
        }
        setDataDir(dir);
        setScreen(perm);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, [setDeviceName]);

  async function onSave(e: FormEvent) {
    e.preventDefault();
    setMessage(null);
    setError(null);
    try {
      if (apiKey.trim()) {
        await api.setApiKey(apiKey.trim());
        setHasKey(true);
        setApiKey("");
      }
      await api.saveSettings({
        defaultDeviceName: deviceName || null,
        language,
        autoDetectCalls,
        vocabulary,
      });
      setMessage("Configurações salvas.");
    } catch (err) {
      setError(String(err));
    }
  }

  async function onClearKey() {
    try {
      await api.setApiKey("");
      setHasKey(false);
      setMessage("API key removida do keychain.");
    } catch (err) {
      setError(String(err));
    }
  }

  async function onRequestScreen() {
    setBusy(true);
    setError(null);
    try {
      const s = await api.requestScreenPermission();
      setScreen(s);
      setMessage(s.hint);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <Link className="btn ghost" to="/">
            ← Biblioteca
          </Link>
          <h1>Configurações</h1>
        </div>
      </header>

      <section className="panel">
        <h2>Detecção de calls</h2>
        <p className="muted">
          Quando o Zoom, Meet, Teams, Discord, FaceTime etc. abrem, o Rebobina
          pergunta se você quer gravar. Ao sair da call, lembra de parar a
          gravação.
        </p>
        <label className="row gap" style={{ alignItems: "center" }}>
          <input
            type="checkbox"
            checked={autoDetectCalls}
            onChange={(e) => setAutoDetectCalls(e.target.checked)}
          />
          <span>Detectar calls e perguntar se quero gravar</span>
        </label>
        <p className="muted" style={{ marginTop: 8 }}>
          Clique em Salvar abaixo para aplicar. Para Meet no navegador, a
          permissão de Gravação de tela melhora a detecção da aba.
        </p>
      </section>

      <section className="panel">
        <h2>Áudio do sistema (ScreenCaptureKit)</h2>
        <p className="muted">
          Status:{" "}
          <strong>
            {screen == null
              ? "verificando…"
              : screen.needsRelaunch
                ? "ligada — reinicie o app (Cmd+Q)"
                : screen.granted
                  ? screen.adhocSigned
                    ? "toggle ligado (build ad-hoc — pode falhar no macOS 15+)"
                    : "permissão concedida"
                  : "permissão necessária"}
          </strong>
        </p>
        {screen && <p className="muted">{screen.hint}</p>}
        {screen?.adhocSigned && (
          <p className="error" style={{ marginTop: 8 }}>
            Este instalador está assinado de forma ad-hoc. No macOS 15+ o sistema
            pode mostrar Rebobina ligado em Gravação de tela e mesmo assim
            recusar a captura. A solução definitiva é um build com certificado
            Apple (Developer ID) no GitHub Actions.
          </p>
        )}
        <div className="row gap">
          {!screen?.granted && (
            <button
              className="btn primary"
              type="button"
              disabled={busy}
              onClick={onRequestScreen}
            >
              {busy ? "Aguarde…" : "Permitir captura"}
            </button>
          )}
          <button
            className="btn"
            type="button"
            onClick={() =>
              api.openScreenSettings().catch((e) => setError(String(e)))
            }
          >
            Abrir Ajustes do macOS
          </button>
          <button
            className="btn ghost"
            type="button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setError(null);
              try {
                setMessage(await api.resetScreenPermission());
                setScreen(await api.getScreenPermission());
              } catch (e) {
                setError(String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            Resetar permissão TCC
          </button>
          <button
            className="btn ghost"
            type="button"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                setScreen(await api.getScreenPermission());
              } finally {
                setBusy(false);
              }
            }}
          >
            Verificar de novo
          </button>
        </div>
      </section>

      <section className="panel">
        <h2>OpenAI</h2>
        <p className="muted privacy">
          Áudio e textos ficam no seu Mac. Apenas o necessário para transcrição
          (Whisper) e resumo (GPT-4o) é enviado à OpenAI.
        </p>
        <p className="muted">
          Status da key:{" "}
          <strong>{hasKey ? "configurada no keychain" : "não configurada"}</strong>
        </p>
        <form className="stack" onSubmit={onSave}>
          <label>
            API key
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={hasKey ? "•••••••• (cole para substituir)" : "sk-…"}
              autoComplete="off"
            />
          </label>
          <label>
            Idioma da transcrição
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
            >
              <option value="pt">Português (pt)</option>
              <option value="en">English (en)</option>
              <option value="es">Español (es)</option>
            </select>
          </label>
          <label>
            <span>
              Vocabulário <span className="optional">(opcional)</span>
            </span>
            <textarea
              id="vocabulary"
              rows={4}
              value={vocabulary}
              onChange={(e) => setVocabulary(e.target.value)}
              placeholder="Kavagy, Vivan, GetNet, Argo CD, SonarQube, Zabbix"
            />
            <span className="field-hint">
              Nomes de pessoas, empresas e termos técnicos que aparecem nas suas
              reuniões, separados por vírgula ou um por linha. Ajudam a
              transcrição a escrever certo e o resumo a corrigir o que saiu errado.
            </span>
          </label>
          <label>
            Microfone padrão
            <select
              value={deviceName}
              onChange={(e) => setDeviceName(e.target.value)}
              onFocus={() => {
                void refreshDevices();
              }}
              onMouseDown={() => {
                void refreshDevices();
              }}
            >
              <option value="">Seguir o sistema</option>
              {devices.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name}
                </option>
              ))}
            </select>
          </label>
          <div className="row gap">
            <button className="btn primary" type="submit">
              Salvar
            </button>
            {hasKey && (
              <button
                className="btn ghost danger"
                type="button"
                onClick={onClearKey}
              >
                Remover API key
              </button>
            )}
          </div>
        </form>
        {message && <p className="ok">{message}</p>}
        {error && <p className="error">{error}</p>}
      </section>

      <section className="panel">
        <h2>Como funciona</h2>
        <ol className="checklist">
          <li>
            Áudio remoto (Zoom/Meet/etc.) → ScreenCaptureKit (permissão de
            Gravação de tela).
          </li>
          <li>Sua voz → microfone selecionado acima.</li>
          <li>Os dois são misturados em um WAV local e enviados ao Whisper.</li>
          <li>BlackHole não é necessário.</li>
        </ol>
        <p className="muted">
          Dados locais: <code>{dataDir}</code>
        </p>
      </section>
    </div>
  );
}
