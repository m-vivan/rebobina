use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::sync::OnceLock;

const BUNDLE_ID: &str = "dev.vivan.rebobina";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenPermissionStatus {
    /// `CGPreflightScreenCaptureAccess` no momento da consulta.
    pub granted: bool,
    /// Preflight ficou true só depois do app abrir — ScreenCaptureKit só vale após reiniciar.
    pub needs_relaunch: bool,
    /// Build com assinatura ad-hoc (`codesign -`). No macOS 15+ o TCC/SCK costuma ignorar o toggle.
    pub adhoc_signed: bool,
    pub hint: String,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Preflight no início do processo (antes de o usuário mexer em Ajustes).
static PREFLIGHT_AT_LAUNCH: OnceLock<bool> = OnceLock::new();

pub fn init_screen_permission_baseline() {
    let _ = PREFLIGHT_AT_LAUNCH.get_or_init(|| unsafe { CGPreflightScreenCaptureAccess() });
}

fn preflight_at_launch() -> bool {
    *PREFLIGHT_AT_LAUNCH.get_or_init(|| unsafe { CGPreflightScreenCaptureAccess() })
}

/// Detecta assinatura ad-hoc (sem Team ID Apple) — problemática para SCK no Sequoia+.
pub fn is_adhoc_signed() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let output = std::process::Command::new("codesign")
        .args(["-dv", "--verbose=4"])
        .arg(&exe)
        .output();
    let Ok(out) = output else {
        return false;
    };
    // codesign -dv escreve em stderr
    let text = String::from_utf8_lossy(&out.stderr);
    if text.contains("Signature=adhoc") || text.contains("(adhoc)") {
        return true;
    }
    // Sem Team ID Apple ⇒ tratado como ad-hoc para efeitos de TCC/SCK.
    if text.contains("CodeDirectory") {
        let has_team = text.lines().any(|l| {
            let l = l.trim();
            l.starts_with("TeamIdentifier=")
                && !l.contains("not set")
                && l.len() > "TeamIdentifier=".len()
        });
        return !has_team;
    }
    false
}

/// Se o app já tem Gravação de tela, sem abrir o pedido do macOS.
pub fn has_screen_permission() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

pub fn screen_permission_status() -> ScreenPermissionStatus {
    let granted = unsafe { CGPreflightScreenCaptureAccess() };
    let needs_relaunch = granted && !preflight_at_launch();
    let adhoc_signed = is_adhoc_signed();

    let hint = if needs_relaunch {
        "A permissão foi ligada com o app aberto. Feche o Rebobina por completo (Cmd+Q) e abra de novo — senão a captura continua bloqueada.".into()
    } else if granted && adhoc_signed {
        "O macOS mostra Gravação de tela ligada, mas este build está com assinatura ad-hoc. No macOS 15+ o ScreenCaptureKit costuma recusar mesmo assim. Use um build assinado com certificado Apple (Developer ID), ou tente: desligar e religar o Rebobina em Ajustes → Cmd+Q → abrir de /Aplicativos.".into()
    } else if granted {
        "Permissão de Gravação de tela concedida — áudio do sistema disponível.".into()
    } else {
        "O Rebobina precisa de Gravação de tela para capturar o áudio da reunião. Em Ajustes → Privacidade e segurança → Gravação de tela, habilite o Rebobina, depois Cmd+Q e abra o app de novo.".into()
    };

    ScreenPermissionStatus {
        granted,
        needs_relaunch,
        adhoc_signed,
        hint,
    }
}

pub fn ensure_screen_permission() -> AppResult<()> {
    init_screen_permission_baseline();
    let granted = unsafe { CGPreflightScreenCaptureAccess() };
    if granted {
        if !preflight_at_launch() {
            return Err(AppError::Audio(
                "Permissão ligada, mas o app precisa reiniciar. Feche com Cmd+Q e abra o Rebobina de novo.".into(),
            ));
        }
        return Ok(());
    }
    let requested = unsafe { CGRequestScreenCaptureAccess() };
    if requested || unsafe { CGPreflightScreenCaptureAccess() } {
        return Err(AppError::Audio(
            "Permissão concedida neste momento — feche o Rebobina (Cmd+Q) e abra de novo para a captura funcionar.".into(),
        ));
    }
    Err(AppError::Audio(
        "Permissão de Gravação de tela negada. Abra Ajustes do Sistema → Privacidade e segurança → Gravação de tela, ative o Rebobina e reinicie o app (Cmd+Q).".into(),
    ))
}

pub fn open_screen_privacy_settings() -> AppResult<()> {
    let status = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .status()
        .map_err(|e| AppError::Message(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        let status = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture")
            .status()
            .map_err(|e| AppError::Message(e.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(AppError::Message(
                "Não foi possível abrir os Ajustes de privacidade".into(),
            ))
        }
    }
}

/// Limpa a entrada TCC de Gravação de tela deste bundle (usuário precisará conceder de novo).
pub fn reset_screen_permission_tcc() -> AppResult<String> {
    let status = std::process::Command::new("tccutil")
        .args(["reset", "ScreenCapture", BUNDLE_ID])
        .status()
        .map_err(|e| AppError::Message(format!("tccutil: {e}")))?;
    if status.success() {
        Ok(format!(
            "Permissão TCC reiniciada para {BUNDLE_ID}. Abra Ajustes, ative o Rebobina, depois Cmd+Q e abra o app de novo."
        ))
    } else {
        Err(AppError::Message(format!(
            "Não foi possível resetar TCC (exit {status}). No Terminal: tccutil reset ScreenCapture {BUNDLE_ID}"
        )))
    }
}

/// Mensagem amigável quando o SCK falha com recusa de TCC apesar do toggle ligado.
pub fn tcc_capture_denied_message(raw: &str) -> String {
    let adhoc = is_adhoc_signed();
    let base = format!(
        "Não foi possível iniciar a captura ({raw})."
    );
    if adhoc {
        format!(
            "{base} O Rebobina está com assinatura ad-hoc: no macOS 15+ o sistema costuma mostrar Gravação de tela ligada e mesmo assim recusar o ScreenCaptureKit. \
Solução definitiva: publicar um build assinado (secrets APPLE_CERTIFICATE no GitHub Actions). \
Enquanto isso tente: 1) Cmd+Q 2) em Ajustes desligar e religar Rebobina 3) abrir de /Aplicativos. \
Ou no Terminal: tccutil reset ScreenCapture {BUNDLE_ID}"
        )
    } else {
        format!(
            "{base} Em Ajustes → Privacidade → Gravação de tela: desligue e religue o Rebobina, depois Cmd+Q e abra o app de novo. \
Se persistir: tccutil reset ScreenCapture {BUNDLE_ID}"
        )
    }
}
