import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { AudioDeviceInfo } from "../types";

/** Valor do seletor que significa "seguir o microfone padrão do macOS". */
export const FOLLOW_SYSTEM = "";

/**
 * Escolha inicial: o microfone padrão salvo nas Configurações, se estiver
 * conectado; senão, seguir o sistema.
 */
function initialChoice(devices: AudioDeviceInfo[], savedDefault?: string | null): string {
  if (savedDefault && devices.some((d) => d.name === savedDefault)) {
    return savedDefault;
  }
  return FOLLOW_SYSTEM;
}

/**
 * Nas atualizações seguintes, mantém a escolha da pessoa (inclusive "seguir o
 * sistema"); só muda se o microfone escolhido for desconectado.
 */
function keepChoice(devices: AudioDeviceInfo[], current: string): string {
  if (current === FOLLOW_SYSTEM || devices.some((d) => d.name === current)) {
    return current;
  }
  return FOLLOW_SYSTEM;
}

/** Lista de microfones que atualiza ao focar o select e a cada poucos segundos. */
export function useAudioDevices(pollMs = 2000) {
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [deviceName, setDeviceName] = useState("");
  const savedDefault = useRef<string | null>(null);
  const refreshing = useRef(false);
  // A escolha inicial só é aplicada uma vez; depois vale o que a pessoa escolheu.
  const initialized = useRef(false);

  const refreshDevices = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      const list = await api.listAudioDevices();
      setDevices(list);
      if (!initialized.current) {
        initialized.current = true;
        setDeviceName(initialChoice(list, savedDefault.current));
      } else {
        setDeviceName((prev) => keepChoice(list, prev));
      }
    } catch {
      // silencioso no poll; erros de start ainda aparecem na UI
    } finally {
      refreshing.current = false;
    }
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const settings = await api.getSettings();
        savedDefault.current = settings.defaultDeviceName;
      } catch {
        /* ignore */
      }
      await refreshDevices();
    })();
  }, [refreshDevices]);

  useEffect(() => {
    const id = window.setInterval(() => {
      void refreshDevices();
    }, pollMs);
    return () => window.clearInterval(id);
  }, [pollMs, refreshDevices]);

  useEffect(() => {
    const onFocus = () => {
      void refreshDevices();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refreshDevices]);

  return {
    devices,
    deviceName,
    setDeviceName,
    refreshDevices,
  };
}
