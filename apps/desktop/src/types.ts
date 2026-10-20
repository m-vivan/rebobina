/** "saved": gravação parada sem transcrever ("Parar"); falta rebobinar. */
export type MeetingStatus = "recording" | "saved" | "processing" | "ready" | "error";

export interface RecordingStatus {
  meetingId: string;
  paused: boolean;
  /** Segundos gravados, sem contar as pausas. */
  elapsedSec: number;
}

export interface Meeting {
  id: string;
  title: string;
  startedAt: string;
  endedAt: string | null;
  durationSec: number | null;
  audioPath: string | null;
  status: MeetingStatus | string;
  errorMessage: string | null;
  deviceName: string | null;
  description: string | null;
}

export interface AudioDeviceInfo {
  name: string;
  isDefault: boolean;
  channels: number;
  likelyLoopback: boolean;
}

export interface ScreenPermissionStatus {
  granted: boolean;
  /** Permissão ligada depois do app abrir — precisa Cmd+Q e reabrir. */
  needsRelaunch: boolean;
  /** Build ad-hoc: no macOS 15+ o SCK pode recusar mesmo com o toggle ON. */
  adhocSigned: boolean;
  hint: string;
}

export interface BlackHoleStatus {
  installed: boolean;
  driverOnDisk: boolean;
  driverPath: string | null;
  deviceActive: boolean;
  brewAvailable: boolean;
  brewPath: string | null;
  brewCaskInstalled: boolean;
  deviceNames: string[];
  downloadUrl: string;
  mayNeedReboot: boolean;
  hint: string;
}

export interface BlackHoleInstallResult {
  method: string;
  message: string;
}

export interface AppSettings {
  defaultDeviceName: string | null;
  language: string;
  autoDetectCalls: boolean;
  /** Nomes, empresas e termos que orientam a transcrição e o resumo. */
  vocabulary?: string;
}

export interface DetectedCall {
  app: string;
  title: string;
  fingerprint: string;
}

export interface CallEndedNotice {
  meetingId: string;
  lastApp: string | null;
}

export interface Transcript {
  meetingId: string;
  fullText: string;
  segmentsJson: string;
}

export interface TranscriptSegment {
  start: number;
  end: number;
  text: string;
  /** "me" = microfone (você); "others" = áudio da chamada. */
  speaker?: "me" | "others" | null;
}

export interface ActionItem {
  assignee?: string | null;
  task: string;
  due?: string | null;
}

export interface Summary {
  meetingId: string;
  overview: string;
  decisionsJson: string;
  actionItemsJson: string;
  topicsJson: string;
  notableQuotesJson: string;
  rawMarkdown: string;
  model: string;
  createdAt: string;
  detailsJson: string | null;
}

export interface Participant {
  name: string;
  role?: string | null;
}

export interface KeyPoint {
  topic: string;
  details: string[];
}

/** Resumo estruturado completo (campos novos podem faltar em resumos antigos). */
export interface SummaryDetails {
  suggested_title?: string;
  overview?: string;
  participants?: Participant[];
  key_points?: KeyPoint[];
  decisions?: string[];
  action_items?: ActionItem[];
  open_questions?: string[];
  next_steps?: string[];
  topics?: string[];
  notable_quotes?: string[];
}
