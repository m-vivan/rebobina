use crate::error::AppResult;
use crate::storage::Storage;
use serde::Serialize;
use std::collections::HashSet;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCall {
    pub app: String,
    pub title: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallEndedNotice {
    pub meeting_id: String,
    pub last_app: Option<String>,
}

struct DetectorState {
    /// Fingerprints já promptados nesta sessão de call (até sumirem).
    prompted: HashSet<String>,
    last_seen: HashSet<String>,
    enabled: AtomicBool,
    /// Viu call enquanto gravava (evita lembrete se gravou sem call detectada).
    saw_call_while_recording: bool,
    /// Polls seguidos sem call (debounce para não disparar ao trocar de janela).
    call_absent_ticks: u32,
    stop_prompted: bool,
    last_app_while_recording: Option<String>,
}

impl DetectorState {
    fn new() -> Self {
        Self {
            prompted: HashSet::new(),
            last_seen: HashSet::new(),
            enabled: AtomicBool::new(true),
            saw_call_while_recording: false,
            call_absent_ticks: 0,
            stop_prompted: false,
            last_app_while_recording: None,
        }
    }

    fn reset_recording_tracking(&mut self) {
        self.saw_call_while_recording = false;
        self.call_absent_ticks = 0;
        self.stop_prompted = false;
        self.last_app_while_recording = None;
    }
}

pub fn spawn_watcher(app: AppHandle, storage: Arc<Storage>) {
    let state = Arc::new(Mutex::new(DetectorState::new()));

    // Carrega preferência
    if let Ok(settings) = storage.get_settings() {
        state
            .lock()
            .unwrap()
            .enabled
            .store(settings.auto_detect_calls, Ordering::Relaxed);
    }

    app.manage(DetectorHandle {
        state: Arc::clone(&state),
    });

    let app_poll = app.clone();
    thread::spawn(move || {
        // ~2 polls (~6s) sem call antes de lembrar de parar
        const ABSENT_TICKS_BEFORE_STOP_PROMPT: u32 = 2;

        loop {
            thread::sleep(Duration::from_secs(3));

            let enabled = state
                .lock()
                .map(|s| s.enabled.load(Ordering::Relaxed))
                .unwrap_or(true);
            if !enabled {
                continue;
            }

            let recording_id = app_poll
                .try_state::<crate::AppState>()
                .and_then(|s| s.recorder.current_meeting_id());

            let detected = scan_calls();
            let fingerprints: HashSet<String> =
                detected.iter().map(|c| c.fingerprint.clone()).collect();

            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };

            if let Some(meeting_id) = recording_id {
                // Durante gravação: lembrar de parar quando a call sumir
                if let Some(first) = detected.first() {
                    guard.saw_call_while_recording = true;
                    guard.call_absent_ticks = 0;
                    guard.last_app_while_recording = Some(first.app.clone());
                } else if guard.saw_call_while_recording {
                    guard.call_absent_ticks = guard.call_absent_ticks.saturating_add(1);
                    if guard.call_absent_ticks >= ABSENT_TICKS_BEFORE_STOP_PROMPT
                        && !guard.stop_prompted
                    {
                        guard.stop_prompted = true;
                        let notice = CallEndedNotice {
                            meeting_id,
                            last_app: guard.last_app_while_recording.clone(),
                        };
                        let _ = app_poll.emit("call-ended", notice);
                    }
                }
            } else {
                guard.reset_recording_tracking();

                // Fora de gravação: perguntar se quer gravar ao entrar
                guard.prompted.retain(|fp| fingerprints.contains(fp));

                for call in &detected {
                    let is_new = !guard.last_seen.contains(&call.fingerprint);
                    let already = guard.prompted.contains(&call.fingerprint);
                    if is_new && !already {
                        guard.prompted.insert(call.fingerprint.clone());
                        let _ = app_poll.emit("call-detected", call.clone());
                    }
                }
            }

            guard.last_seen = fingerprints;
        }
    });
}

pub struct DetectorHandle {
    state: Arc<Mutex<DetectorState>>,
}

pub fn set_enabled(app: &AppHandle, enabled: bool) {
    if let Some(handle) = app.try_state::<DetectorHandle>() {
        if let Ok(guard) = handle.state.lock() {
            guard.enabled.store(enabled, Ordering::Relaxed);
        }
    }
}

pub fn scan_now() -> AppResult<Vec<DetectedCall>> {
    Ok(scan_calls())
}

fn scan_calls() -> Vec<DetectedCall> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();

    // 1) Processos conhecidos de call
    for (app, title) in scan_processes() {
        let fingerprint = format!("proc:{}", app.to_lowercase());
        if seen.insert(fingerprint.clone()) {
            found.push(DetectedCall {
                app,
                title,
                fingerprint,
            });
        }
    }

    // 2) Janelas via ScreenCaptureKit (Meet no Chrome, títulos Zoom, etc.)
    for call in scan_windows_sck() {
        if seen.insert(call.fingerprint.clone()) {
            found.push(call);
        }
    }

    found
}

fn scan_processes() -> Vec<(String, String)> {
    let output = Command::new("/bin/ps")
        .args(["-axc", "-o", "comm="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();

    let rules: &[(&str, &str, &str)] = &[
        ("zoom.us", "Zoom", "Reunião Zoom"),
        ("Zoom", "Zoom", "Reunião Zoom"),
        ("Microsoft Teams", "Microsoft Teams", "Reunião Teams"),
        ("Teams", "Microsoft Teams", "Reunião Teams"),
        ("Discord", "Discord", "Call Discord"),
        ("FaceTime", "FaceTime", "FaceTime"),
        ("Webex", "Webex", "Reunião Webex"),
        ("Skype", "Skype", "Call Skype"),
        ("Slack", "Slack", "Possível Huddle Slack"),
    ];

    let procs: HashSet<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    for (needle, app, title) in rules {
        if procs.iter().any(|p| p == *needle || p.contains(needle)) {
            // Slack sozinho é barulhento — só conta se também houver indício de call
            if *app == "Slack" {
                continue;
            }
            hits.push(((*app).into(), (*title).into()));
        }
    }

    // Slack huddle: processo Slack + janela (tratado no SCK). Aqui só se "Slack Helper" etc.
    hits
}

fn scan_windows_sck() -> Vec<DetectedCall> {
    use screencapturekit::prelude::*;

    let Ok(content) = SCShareableContent::get() else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for window in content.windows() {
        let title = window.title().unwrap_or_default();
        let app_name = window
            .owning_application()
            .map(|a| a.application_name())
            .unwrap_or_default();

        if title.trim().is_empty() && app_name.trim().is_empty() {
            continue;
        }

        if let Some(call) = classify_window(&app_name, &title) {
            found.push(call);
        }
    }
    found
}

fn classify_window(app: &str, title: &str) -> Option<DetectedCall> {
    let app_l = app.to_lowercase();
    let title_l = title.to_lowercase();

    let (app_label, kind) = if app_l.contains("zoom")
        || title_l.contains("zoom meeting")
        || title_l.contains("zoom webinar")
    {
        ("Zoom", "Reunião Zoom")
    } else if app_l.contains("teams")
        || title_l.contains("microsoft teams")
        || (title_l.contains("|") && title_l.contains("teams"))
    {
        ("Microsoft Teams", "Reunião Teams")
    } else if title_l.contains("meet.google.com")
        || title_l.contains("google meet")
        || ((title_l.contains("meet -")
            || title_l.contains(" – meet")
            || title_l.contains(" | meet"))
            && (app_l.contains("chrome")
                || app_l.contains("safari")
                || app_l.contains("firefox")
                || app_l.contains("edge")
                || app_l.contains("brave")
                || app_l.contains("arc")))
    {
        ("Google Meet", "Reunião Meet")
    } else if app_l.contains("facetime") || title_l.contains("facetime") {
        ("FaceTime", "FaceTime")
    } else if app_l.contains("discord")
        && (title_l.contains("voice") || title_l.contains("discord"))
    {
        ("Discord", "Call Discord")
    } else if app_l.contains("webex") || title_l.contains("webex") {
        ("Webex", "Reunião Webex")
    } else if app_l.contains("slack") && title_l.contains("huddle") {
        ("Slack", "Huddle Slack")
    } else {
        return None;
    };

    // Evita abas genéricas tipo "Zoom" na App Store
    if title_l.contains("app store") || title_l.contains("settings") || title_l.contains("ajustes")
    {
        return None;
    }

    let fingerprint = format!(
        "win:{}:{}",
        app_label.to_lowercase(),
        // normaliza título curto
        title.chars().take(80).collect::<String>().to_lowercase()
    );

    Some(DetectedCall {
        app: app_label.into(),
        title: if title.trim().is_empty() {
            kind.into()
        } else {
            title.chars().take(120).collect()
        },
        fingerprint,
    })
}
