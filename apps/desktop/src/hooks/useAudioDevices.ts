import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { AudioDeviceInfo } from "../types";

function pickPreferred(
  devices: AudioDeviceInfo[],
  current: string,
  savedDefault?: string | null,
): string {
  if (current && devices.some((d) => d.name === current)) {
    return current;
  }
  if (savedDefault && devices.some((d) => d.name === savedDefault)) {
    return savedDefault;
  }
  return (
    devices.find((d) => d.isDefault && !d.likelyLoopback)?.name ||
    devices.find((d) => !d.likelyLoopback)?.name ||
    devices.find((d) => d.isDefault)?.name ||
    devices[0]?.name ||
    ""
  );
}

/** Lista de microfones que atualiza ao focar o select e a cada poucos segundos. */
export function useAudioDevices(pollMs = 2000) {
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [deviceName, setDeviceName] = useState("");
  const savedDefault = useRef<string | null>(null);
  const refreshing = useRef(false);

  const refreshDevices = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      const list = await api.listAudioDevices();
      setDevices(list);
      setDeviceName((prev) =>
        pickPreferred(list, prev, savedDefault.current),
      );
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
