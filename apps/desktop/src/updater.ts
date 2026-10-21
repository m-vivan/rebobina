import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

/** Procura versão nova na última release do GitHub. `null` = já está atualizado. */
export async function findUpdate(): Promise<Update | null> {
  return check();
}

/**
 * Baixa, instala e reinicia o app. O download feito pelo próprio app não
 * recebe a marca de quarentena, então o macOS não pede "Abrir Mesmo Assim".
 */
export async function installUpdate(
  update: Update,
  onProgress: (percent: number | null) => void,
): Promise<void> {
  let total = 0;
  let received = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      onProgress(total ? 0 : null);
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
      onProgress(total ? Math.min(100, Math.round((received / total) * 100)) : null);
    } else if (event.event === "Finished") {
      onProgress(100);
    }
  });
  await relaunch();
}
