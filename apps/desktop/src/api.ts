import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AudioDeviceInfo,
  BlackHoleInstallResult,
  BlackHoleStatus,
  DetectedCall,
  Meeting,
  ScreenPermissionStatus,
  Summary,
  Transcript,
} from "./types";

export const api = {
  listAudioDevices: () => invoke<AudioDeviceInfo[]>("list_audio_devices"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<void>("save_settings", { settings }),
  setApiKey: (apiKey: string) => invoke<void>("set_api_key", { apiKey }),
  hasApiKey: () => invoke<boolean>("has_api_key"),
  listMeetings: (query?: string) =>
    invoke<Meeting[]>("list_meetings", { query: query ?? null }),
  getMeeting: (meetingId: string) =>
    invoke<Meeting | null>("get_meeting", { meetingId }),
  getTranscript: (meetingId: string) =>
    invoke<Transcript | null>("get_transcript", { meetingId }),
  getSummary: (meetingId: string) =>
    invoke<Summary | null>("get_summary", { meetingId }),
  deleteMeeting: (meetingId: string) =>
    invoke<void>("delete_meeting", { meetingId }),
  startRecording: (title: string, deviceName?: string | null) =>
    invoke<Meeting>("start_recording", {
      title,
      deviceName: deviceName ?? null,
    }),
  stopRecording: () => invoke<Meeting>("stop_recording"),
  switchMicrophone: (deviceName: string | null) =>
    invoke<void>("switch_microphone", { deviceName }),
  retryProcessing: (meetingId: string) =>
    invoke<void>("retry_processing", { meetingId }),
  updateMeeting: (meetingId: string, title: string, description: string | null) =>
    invoke<Meeting>("update_meeting", { meetingId, title, description }),
  regenerateSummary: (meetingId: string) =>
    invoke<void>("regenerate_summary", { meetingId }),
  exportMeetingMarkdown: (meetingId: string) =>
    invoke<string>("export_meeting_markdown", { meetingId }),
  saveExportFile: (meetingId: string) =>
    invoke<string>("save_export_file", { meetingId }),
  getDataDir: () => invoke<string>("get_data_dir"),
  getRecordingState: () => invoke<string | null>("get_recording_state"),
  getAudioPath: (meetingId: string) =>
    invoke<string | null>("get_audio_path", { meetingId }),
  getScreenPermission: () =>
    invoke<ScreenPermissionStatus>("get_screen_permission"),
  requestScreenPermission: () =>
    invoke<ScreenPermissionStatus>("request_screen_permission"),
  resetScreenPermission: () => invoke<string>("reset_screen_permission"),
  openScreenSettings: () => invoke<void>("open_screen_settings"),
  getBlackHoleStatus: () => invoke<BlackHoleStatus>("get_blackhole_status"),
  installBlackHole: () => invoke<BlackHoleInstallResult>("install_blackhole"),
  openBlackHoleDownload: () => invoke<void>("open_blackhole_download"),
  activateBlackHole: () => invoke<BlackHoleInstallResult>("activate_blackhole"),
  scanActiveCalls: () => invoke<DetectedCall[]>("scan_active_calls"),
};
