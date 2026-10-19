import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  onAction,
  registerActionTypes,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../api";
import type { CallEndedNotice, DetectedCall } from "../types";

const ACTION_START = "call-start";
const ACTION_STOP = "call-stop";
const ACT_RECORD = "record";
const ACT_DISMISS = "dismiss";
const ACT_STOP = "stop";
const ACT_KEEP = "keep";

async function ensureNotifyPermission(): Promise<boolean> {
  let granted = await isPermissionGranted();
  if (!granted) {
    const state = await requestPermission();
    granted = state === "granted";
  }
  return granted;
}

async function focusApp() {
  try {
    await getCurrentWindow().setFocus();
  } catch {
    // ignora
  }
}

/**
 * Notificações nativas do macOS ao detectar call / fim da call.
 */
export function CallDetectPrompt() {
  const navigate = useNavigate();
  const pendingStart = useRef<DetectedCall | null>(null);
  const pendingStop = useRef<CallEndedNotice | null>(null);
  const busy = useRef(false);
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;

  useEffect(() => {
    let unStart: (() => void) | undefined;
    let unEnd: (() => void) | undefined;
    let unAction: { unregister: () => Promise<void> } | undefined;
    let cancelled = false;

    async function startFromNotification(call: DetectedCall | null) {
      if (busy.current) return;
      busy.current = true;
      try {
        await focusApp();
        const recording = await api.getRecordingState();
        if (recording) {
          navigateRef.current(`/recording/${recording}`);
          return;
        }
        const title = call
          ? `${call.app} · ${new Date().toLocaleString("pt-BR", {
              dateStyle: "short",
              timeStyle: "short",
            })}`
          : `Reunião ${new Date().toLocaleString("pt-BR", {
              dateStyle: "short",
              timeStyle: "short",
            })}`;
        const meeting = await api.startRecording(title, null);
        pendingStart.current = null;
        navigateRef.current(`/recording/${meeting.id}`);
      } catch (e) {
        console.error(e);
        await focusApp();
      } finally {
        busy.current = false;
      }
    }

    async function stopFromNotification() {
      if (busy.current) return;
      busy.current = true;
      try {
        await focusApp();
        const recording = await api.getRecordingState();
        if (!recording) {
          pendingStop.current = null;
          return;
        }
        const meeting = await api.stopRecording();
        pendingStop.current = null;
        navigateRef.current(`/meeting/${meeting.id}`);
      } catch (e) {
        console.error(e);
        await focusApp();
      } finally {
        busy.current = false;
      }
    }

    (async () => {
      const ok = await ensureNotifyPermission();
      if (cancelled || !ok) return;

      await registerActionTypes([
        {
          id: ACTION_START,
          actions: [
            { id: ACT_RECORD, title: "Gravar", foreground: true },
            { id: ACT_DISMISS, title: "Agora não", foreground: false },
          ],
        },
        {
          id: ACTION_STOP,
          actions: [
            { id: ACT_STOP, title: "Parar e processar", foreground: true },
            { id: ACT_KEEP, title: "Continuar gravando", foreground: false },
          ],
        },
      ]);

      unAction = await onAction(async (event) => {
        const actionId = event.actionId;
        if (actionId === ACT_DISMISS || actionId === ACT_KEEP) {
          pendingStart.current = null;
          pendingStop.current = null;
          return;
        }

        const extra = event.notification?.extra ?? {};
        const kind =
          (extra.kind as string | undefined) ??
          event.notification?.actionTypeId ??
          null;

        const wantsRecord =
          actionId === ACT_RECORD ||
          (actionId === "tap" &&
            (kind === ACTION_START || kind === "start" || !!pendingStart.current));
        const wantsStop =
          actionId === ACT_STOP ||
          (actionId === "tap" &&
            (kind === ACTION_STOP || kind === "stop" || !!pendingStop.current));

        if (wantsRecord) {
          await startFromNotification(
            pendingStart.current ??
              (typeof extra.app === "string"
                ? {
                    app: extra.app,
                    title: String(extra.title ?? ""),
                    fingerprint: String(extra.fingerprint ?? ""),
                  }
                : null),
          );
        } else if (wantsStop) {
          await stopFromNotification();
        }
      });

      unStart = await listen<DetectedCall>("call-detected", (event) => {
        pendingStart.current = event.payload;
        pendingStop.current = null;
        const call = event.payload;
        void sendNotification({
          title: "Quer gravar esta reunião?",
          body: call.title
            ? `${call.app}: ${call.title}`
            : `Detectamos ${call.app}`,
          actionTypeId: ACTION_START,
          extra: {
            kind: "start",
            app: call.app,
            title: call.title,
            fingerprint: call.fingerprint,
          },
        });
      });

      unEnd = await listen<CallEndedNotice>("call-ended", (event) => {
        pendingStop.current = event.payload;
        pendingStart.current = null;
        const notice = event.payload;
        void sendNotification({
          title: "Call encerrada",
          body: notice.lastApp
            ? `A reunião do ${notice.lastApp} parece ter terminado. Parar a gravação?`
            : "A reunião parece ter terminado. Parar a gravação?",
          actionTypeId: ACTION_STOP,
          extra: {
            kind: "stop",
            meetingId: notice.meetingId,
            lastApp: notice.lastApp,
          },
        });
      });
    })().catch((e) => console.error("notificações:", e));

    return () => {
      cancelled = true;
      unStart?.();
      unEnd?.();
      void unAction?.unregister();
    };
  }, []);

  return null;
}
