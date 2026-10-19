use crate::error::{AppError, AppResult};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenPermissionStatus {
    pub granted: bool,
    pub hint: String,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub fn screen_permission_status() -> ScreenPermissionStatus {
    let granted = unsafe { CGPreflightScreenCaptureAccess() };
    let hint = if granted {
        "Permissão de Gravação de tela concedida — áudio do sistema disponível.".into()
    } else {
        "O Rebobina precisa de Gravação de tela para capturar o áudio da reunião. Em Ajustes → Privacidade e segurança → Gravação de tela, habilite o Rebobina e reinicie o app.".into()
    };
    ScreenPermissionStatus { granted, hint }
}

pub fn ensure_screen_permission() -> AppResult<()> {
    let granted = unsafe { CGPreflightScreenCaptureAccess() };
    if granted {
        return Ok(());
    }
    let requested = unsafe { CGRequestScreenCaptureAccess() };
    if requested || unsafe { CGPreflightScreenCaptureAccess() } {
        return Ok(());
    }
    Err(AppError::Audio(
        "Permissão de Gravação de tela negada. Abra Ajustes do Sistema → Privacidade e segurança → Gravação de tela, ative o Rebobina e reinicie o app.".into(),
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
        // Fallback macOS moderno
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
