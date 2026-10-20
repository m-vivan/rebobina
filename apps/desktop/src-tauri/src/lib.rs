mod audio;
mod blackhole;
mod error;
mod meeting_detect;
mod openai;
mod pipeline;
mod recovery;
mod speakers;
mod storage;

use audio::{
    ensure_screen_permission, init_screen_permission_baseline, open_screen_privacy_settings,
    reset_screen_permission_tcc, screen_permission_status, AudioDeviceInfo, Recorder,
    RecordingStatus, ScreenPermissionStatus,
};
use blackhole::{BlackHoleStatus, InstallResult};
use chrono::Utc;
use error::{AppError, AppResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use storage::{AppSettings, Meeting, Storage, Summary, Transcript};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub storage: Arc<Storage>,
    pub recorder: Recorder,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LevelPayload {
    /// Maior entre microfone e chamada (compatível com versões anteriores da tela).
    level: f32,
    mic: f32,
    system: f32,
    meeting_id: String,
}

#[tauri::command]
fn list_audio_devices() -> AppResult<Vec<AudioDeviceInfo>> {
    Recorder::list_input_devices()
}

#[tauri::command]
fn get_screen_permission() -> AppResult<ScreenPermissionStatus> {
    Ok(screen_permission_status())
}

#[tauri::command]
fn request_screen_permission() -> AppResult<ScreenPermissionStatus> {
    let _ = ensure_screen_permission();
    Ok(screen_permission_status())
}

#[tauri::command]
fn open_screen_settings() -> AppResult<()> {
    open_screen_privacy_settings()
}

#[tauri::command]
fn reset_screen_permission() -> AppResult<String> {
    reset_screen_permission_tcc()
}

#[tauri::command]
fn get_blackhole_status() -> AppResult<BlackHoleStatus> {
    blackhole::status()
}

#[tauri::command]
fn install_blackhole() -> AppResult<InstallResult> {
    blackhole::install()
}

#[tauri::command]
fn open_blackhole_download() -> AppResult<()> {
    blackhole::open_download_page()
}

#[tauri::command]
fn activate_blackhole() -> AppResult<InstallResult> {
    blackhole::activate()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    state.storage.get_settings()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<()> {
    state.storage.save_settings(&settings)?;
    meeting_detect::set_enabled(&app, settings.auto_detect_calls);
    Ok(())
}

#[tauri::command]
fn scan_active_calls() -> AppResult<Vec<meeting_detect::DetectedCall>> {
    meeting_detect::scan_now()
}

#[tauri::command]
fn set_api_key(api_key: String) -> AppResult<()> {
    Storage::set_api_key(&api_key)
}

#[tauri::command]
fn has_api_key() -> AppResult<bool> {
    Storage::has_api_key()
}

#[tauri::command]
fn list_meetings(state: State<'_, AppState>, query: Option<String>) -> AppResult<Vec<Meeting>> {
    state.storage.list_meetings(query.as_deref())
}

#[tauri::command]
fn get_meeting(state: State<'_, AppState>, meeting_id: String) -> AppResult<Option<Meeting>> {
    state.storage.get_meeting(&meeting_id)
}

#[tauri::command]
fn get_transcript(
    state: State<'_, AppState>,
    meeting_id: String,
) -> AppResult<Option<Transcript>> {
    state.storage.get_transcript(&meeting_id)
}

#[tauri::command]
fn get_summary(state: State<'_, AppState>, meeting_id: String) -> AppResult<Option<Summary>> {
    state.storage.get_summary(&meeting_id)
}

#[tauri::command]
fn delete_meeting(state: State<'_, AppState>, meeting_id: String) -> AppResult<()> {
    if state.recorder.current_meeting_id().as_deref() == Some(meeting_id.as_str()) {
        return Err(AppError::Message(
            "Não é possível apagar uma reunião em gravação".into(),
        ));
    }
    state.storage.delete_meeting(&meeting_id)
}

#[tauri::command]
fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    device_name: Option<String>,
) -> AppResult<Meeting> {
    if state.recorder.is_recording() {
        return Err(AppError::Audio("Já existe uma gravação em andamento".into()));
    }

    let title = if title.trim().is_empty() {
        format!("Reunião {}", Utc::now().format("%Y-%m-%d %H:%M"))
    } else {
        title.trim().to_string()
    };

    let meeting = state
        .storage
        .create_meeting(&title, device_name.as_deref())?;
    let audio_path = state.storage.meeting_dir(&meeting.id).join("audio.wav");

    if let Err(e) = state
        .recorder
        .start(&meeting.id, device_name.as_deref(), &audio_path)
    {
        let _ = state
            .storage
            .set_status(&meeting.id, "error", Some(&e.to_string()));
        return Err(e);
    }

    let meeting_id = meeting.id.clone();
    std::thread::spawn(move || {
        loop {
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            if state.recorder.current_meeting_id().as_deref() != Some(meeting_id.as_str()) {
                break;
            }
            let (mic, system) = state.recorder.take_levels();
            let _ = app.emit(
                "recording-level",
                LevelPayload {
                    level: mic.max(system),
                    mic,
                    system,
                    meeting_id: meeting_id.clone(),
                },
            );
            drop(state);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    Ok(meeting)
}

// `async`: roda fora da thread principal; abrir um dispositivo de áudio pode demorar.
#[tauri::command(async)]
fn switch_microphone(state: State<'_, AppState>, device_name: Option<String>) -> AppResult<()> {
    state.recorder.switch_microphone(device_name.as_deref())
}

#[tauri::command]
/// `process` falso: só salva o áudio ("Parar"); a transcrição e o resumo ficam
/// para quando a pessoa pedir ("Rebobinar" no detalhe da reunião).
fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    process: Option<bool>,
) -> AppResult<Meeting> {
    let process = process.unwrap_or(true);
    let (meeting_id, path, duration_sec) = state.recorder.stop()?;
    let ended_at = Utc::now();
    state
        .storage
        .finish_recording(&meeting_id, &path, ended_at, duration_sec)?;
    if !process {
        state.storage.set_status(&meeting_id, "saved", None)?;
    }

    let meeting = state
        .storage
        .get_meeting(&meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada após gravar".into()))?;

    if process {
        spawn_pipeline(app, state.storage.clone(), meeting_id);
    }
    Ok(meeting)
}

#[tauri::command]
fn pause_recording(state: State<'_, AppState>) -> AppResult<RecordingStatus> {
    state.recorder.pause()
}

#[tauri::command]
fn resume_recording(state: State<'_, AppState>) -> AppResult<RecordingStatus> {
    state.recorder.resume()
}

#[tauri::command]
fn get_recording_status(state: State<'_, AppState>) -> AppResult<Option<RecordingStatus>> {
    Ok(state.recorder.status())
}

#[tauri::command]
fn retry_processing(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: String,
) -> AppResult<()> {
    let meeting = state
        .storage
        .get_meeting(&meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada".into()))?;

    if meeting.audio_path.is_none() {
        return Err(AppError::Message("Reunião sem áudio para processar".into()));
    }

    state.storage.set_status(&meeting_id, "processing", None)?;
    spawn_pipeline(app, state.storage.clone(), meeting_id);
    Ok(())
}

#[tauri::command]
fn update_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
    title: String,
    description: Option<String>,
) -> AppResult<Meeting> {
    state
        .storage
        .update_meeting_details(&meeting_id, &title, description.as_deref())?;
    state
        .storage
        .get_meeting(&meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada".into()))
}

#[tauri::command]
fn regenerate_summary(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: String,
) -> AppResult<()> {
    if state.storage.get_transcript(&meeting_id)?.is_none() {
        return Err(AppError::Message("Reunião ainda sem transcrição".into()));
    }
    state.storage.set_status(&meeting_id, "processing", None)?;
    let storage = state.storage.clone();
    std::thread::spawn(move || {
        match pipeline::regenerate_summary(&storage, &meeting_id) {
            Ok(()) => {
                let _ = app.emit("meeting-processed", meeting_id);
            }
            Err(e) => {
                let _ = app.emit(
                    "meeting-process-error",
                    serde_json::json!({ "meetingId": meeting_id, "error": e.to_string() }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn export_meeting_markdown(state: State<'_, AppState>, meeting_id: String) -> AppResult<String> {
    pipeline::export_markdown(&state.storage, &meeting_id)
}

#[tauri::command]
fn save_export_file(state: State<'_, AppState>, meeting_id: String) -> AppResult<String> {
    let md = pipeline::export_markdown(&state.storage, &meeting_id)?;
    let path: PathBuf = state.storage.meeting_dir(&meeting_id).join("export.md");
    std::fs::write(&path, md)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn get_data_dir(state: State<'_, AppState>) -> AppResult<String> {
    Ok(state.storage.data_dir().to_string_lossy().to_string())
}

#[tauri::command]
fn get_recording_state(state: State<'_, AppState>) -> AppResult<Option<String>> {
    Ok(state.recorder.current_meeting_id())
}

#[tauri::command]
fn get_audio_path(state: State<'_, AppState>, meeting_id: String) -> AppResult<Option<String>> {
    let meeting = state
        .storage
        .get_meeting(&meeting_id)?
        .ok_or_else(|| AppError::Message("Reunião não encontrada".into()))?;
    Ok(meeting.audio_path)
}

fn spawn_pipeline(app: AppHandle, storage: Arc<Storage>, meeting_id: String) {
    std::thread::spawn(move || {
        let result = pipeline::process_meeting(&storage, &meeting_id);
        match result {
            Ok(()) => {
                let _ = app.emit("meeting-processed", meeting_id);
            }
            Err(e) => {
                let _ = app.emit(
                    "meeting-process-error",
                    serde_json::json!({
                        "meetingId": meeting_id,
                        "error": e.to_string()
                    }),
                );
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let storage = Arc::new(Storage::init().expect("falha ao inicializar storage"));

    // Gravações que ficaram pela metade (app fechado ou caiu durante a gravação).
    // A lista é lida agora, antes de existir gravação ativa; a mixagem roda depois.
    match recovery::find_interrupted(&storage) {
        Ok(stuck) if !stuck.is_empty() => {
            let storage = Arc::clone(&storage);
            std::thread::spawn(move || {
                if let Err(e) = recovery::recover_interrupted(&storage, &stuck) {
                    eprintln!("rebobina: falha ao recuperar gravações: {e}");
                }
            });
        }
        Ok(_) => {}
        Err(e) => eprintln!("rebobina: falha ao procurar gravações interrompidas: {e}"),
    }
    let state = AppState {
        storage,
        recorder: Recorder::new(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .setup(|app| {
            init_screen_permission_baseline();
            let storage = app.state::<AppState>().storage.clone();
            meeting_detect::spawn_watcher(app.handle().clone(), storage);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_audio_devices,
            get_settings,
            save_settings,
            set_api_key,
            has_api_key,
            list_meetings,
            get_meeting,
            get_transcript,
            get_summary,
            delete_meeting,
            start_recording,
            stop_recording,
            retry_processing,
            update_meeting,
            switch_microphone,
            pause_recording,
            resume_recording,
            get_recording_status,
            regenerate_summary,
            export_meeting_markdown,
            save_export_file,
            get_data_dir,
            get_recording_state,
            get_audio_path,
            get_screen_permission,
            request_screen_permission,
            open_screen_settings,
            reset_screen_permission,
            get_blackhole_status,
            install_blackhole,
            open_blackhole_download,
            activate_blackhole,
            scan_active_calls
        ])
        .run(tauri::generate_context!())
        .expect("erro ao executar o Rebobina");
}
