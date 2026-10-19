use crate::audio::Recorder;
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const BLACKHOLE_CASK: &str = "blackhole-2ch";
const BLACKHOLE_DOWNLOAD_URL: &str = "https://existential.audio/blackhole/";

const DRIVER_CANDIDATES: &[&str] = &[
    "/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver",
    "/Library/Audio/Plug-Ins/HAL/BlackHole.driver",
    "/Library/Audio/Plug-Ins/HAL/BlackHole16ch.driver",
    "/Library/Audio/Plug-Ins/HAL/BlackHole64ch.driver",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlackHoleStatus {
    /// Driver no disco OU device ativo no Core Audio.
    pub installed: bool,
    /// Arquivo .driver presente em /Library/Audio/Plug-Ins/HAL/
    pub driver_on_disk: bool,
    pub driver_path: Option<String>,
    /// Device visível no Core Audio (entrada).
    pub device_active: bool,
    pub brew_available: bool,
    pub brew_path: Option<String>,
    pub brew_cask_installed: bool,
    pub device_names: Vec<String>,
    pub download_url: String,
    pub may_need_reboot: bool,
    /// Mensagem amigável para a UI.
    pub hint: String,
}

pub fn status() -> AppResult<BlackHoleStatus> {
    let (driver_on_disk, driver_path) = find_driver_on_disk();
    let brew_path = find_brew();
    let brew_cask_installed = brew_path
        .as_ref()
        .map(|b| cask_installed(b))
        .unwrap_or(false);

    let devices = Recorder::list_input_devices().unwrap_or_default();
    let device_names: Vec<String> = devices
        .into_iter()
        .filter(|d| is_blackhole_name(&d.name))
        .map(|d| d.name)
        .collect();
    let device_active = !device_names.is_empty();

    let installed = driver_on_disk || brew_cask_installed || device_active;
    let may_need_reboot = installed && !device_active;

    let hint = if device_active {
        format!(
            "BlackHole ativo: {}.",
            device_names.join(", ")
        )
    } else if driver_on_disk {
        "BlackHole está instalado no disco, mas o macOS ainda não carregou o device. Clique em “Ativar agora” (reinicia o Core Audio) ou reinicie o Mac.".into()
    } else if brew_cask_installed {
        "O pacote Homebrew está instalado, mas o driver ainda não aparece. Clique em “Ativar agora” ou reinstale o cask.".into()
    } else {
        "BlackHole não encontrado. Instale para capturar o áudio da outra pessoa.".into()
    };

    Ok(BlackHoleStatus {
        installed,
        driver_on_disk,
        driver_path: driver_path.map(|p| p.to_string_lossy().to_string()),
        device_active,
        brew_available: brew_path.is_some(),
        brew_path: brew_path.map(|p| p.to_string_lossy().to_string()),
        brew_cask_installed,
        device_names,
        download_url: BLACKHOLE_DOWNLOAD_URL.into(),
        may_need_reboot,
        hint,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub method: String,
    pub message: String,
}

/// Instala BlackHole 2ch via Homebrew Cask (abre Terminal com autenticação)
/// ou, sem brew, abre a página oficial do instalador.
pub fn install() -> AppResult<InstallResult> {
    let current = status()?;
    if current.device_active {
        return Ok(InstallResult {
            method: "already".into(),
            message: current.hint,
        });
    }
    if current.driver_on_disk || current.brew_cask_installed {
        return Ok(InstallResult {
            method: "needs-activation".into(),
            message: format!(
                "{} Use “Ativar agora” nas Configurações (ou reinicie o Mac).",
                current.hint
            ),
        });
    }

    if let Some(brew) = find_brew() {
        let brew = brew.to_string_lossy();
        let brew_escaped = brew.replace('\\', "\\\\").replace('"', "\\\"");
        let cmd = format!(
            "{brew_escaped} install --cask {BLACKHOLE_CASK}; echo; echo \"Concluído. No Gravador, clique em Ativar agora (ou reinicie o Mac).\"; echo \"Pressione Enter para fechar...\"; read _"
        );
        let cmd_escaped = cmd.replace('\\', "\\\\").replace('"', "\\\"");

        let script = format!(
            "tell application \"Terminal\"\nactivate\ndo script \"{cmd_escaped}\"\nend tell"
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| AppError::Message(format!("Falha ao abrir o Terminal: {e}")))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Message(format!(
                "Não foi possível iniciar a instalação via Homebrew: {err}"
            )));
        }

        return Ok(InstallResult {
            method: "brew-terminal".into(),
            message: "Terminal aberto com a instalação do BlackHole. Depois de autorizar, volte ao Gravador e clique em “Ativar agora”.".into(),
        });
    }

    open_download_page()?;
    Ok(InstallResult {
        method: "download-page".into(),
        message: "Homebrew não encontrado. Abrimos a página oficial do BlackHole — baixe o 2ch, instale, depois clique em “Ativar agora” no Gravador.".into(),
    })
}

/// Reinicia o `coreaudiod` para carregar drivers HAL recém-instalados.
pub fn activate() -> AppResult<InstallResult> {
    let before = status()?;
    if before.device_active {
        return Ok(InstallResult {
            method: "already-active".into(),
            message: before.hint,
        });
    }
    if !before.driver_on_disk && !before.brew_cask_installed {
        return Err(AppError::Message(
            "BlackHole ainda não está instalado no disco. Use “Instalar BlackHole” primeiro.".into(),
        ));
    }

    let script = r#"do shell script "killall coreaudiod" with administrator privileges"#;
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| AppError::Message(format!("Falha ao pedir autorização: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Message(format!(
            "Não foi possível reiniciar o Core Audio: {err}. Reinicie o Mac manualmente."
        )));
    }

    // coreaudiod sobe sozinho; dar um tempo curto antes do status.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let after = status()?;
    if after.device_active {
        Ok(InstallResult {
            method: "coreaudio-restart".into(),
            message: format!(
                "Core Audio recarregado. Device ativo: {}.",
                after.device_names.join(", ")
            ),
        })
    } else {
        Ok(InstallResult {
            method: "coreaudio-restart-pending".into(),
            message: "Core Audio foi reiniciado, mas o BlackHole ainda não apareceu. Reinicie o Mac — o Homebrew costuma exigir isso após instalar o driver.".into(),
        })
    }
}

pub fn open_download_page() -> AppResult<()> {
    let status = Command::new("open")
        .arg(BLACKHOLE_DOWNLOAD_URL)
        .status()
        .map_err(|e| AppError::Message(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Message(
            "Não foi possível abrir a página do BlackHole".into(),
        ))
    }
}

fn is_blackhole_name(name: &str) -> bool {
    let lower = name.to_lowercase().replace(' ', "");
    lower.contains("blackhole")
}

fn find_driver_on_disk() -> (bool, Option<PathBuf>) {
    for path in DRIVER_CANDIDATES {
        let p = PathBuf::from(path);
        if p.exists() {
            return (true, Some(p));
        }
    }

    // Qualquer BlackHole*.driver no HAL
    let hal = Path::new("/Library/Audio/Plug-Ins/HAL");
    if let Ok(entries) = std::fs::read_dir(hal) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("blackhole") && name.ends_with(".driver") {
                return (true, Some(entry.path()));
            }
        }
    }
    (false, None)
}

fn cask_installed(brew: &Path) -> bool {
    let output = Command::new(brew)
        .args(["list", "--cask", BLACKHOLE_CASK])
        .output();
    matches!(output, Ok(o) if o.status.success())
}

fn find_brew() -> Option<PathBuf> {
    let candidates = [
        "/opt/homebrew/bin/brew",
        "/usr/local/bin/brew",
    ];
    for path in candidates {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }

    let output = Command::new("/bin/zsh")
        .args(["-lc", "command -v brew"])
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            let p = PathBuf::from(&path);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_does_not_panic() {
        let s = status().expect("status");
        assert!(!s.download_url.is_empty());
        // Neste ambiente o driver costuma estar no disco após brew install.
        if Path::new("/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver").exists() {
            assert!(s.driver_on_disk);
            assert!(s.installed);
        }
    }

    #[test]
    fn blackhole_name_match() {
        assert!(is_blackhole_name("BlackHole 2ch"));
        assert!(is_blackhole_name("BlackHole2ch"));
        assert!(!is_blackhole_name("MacBook Pro Microphone"));
    }
}
