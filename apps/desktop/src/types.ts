export type MeetingStatus = "recording" | "processing" | "ready" | "error";

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
}

export interface AudioDeviceInfo {
  name: string;
  isDefault: boolean;
  channels: number;
  likelyLoopback: boolean;
}

export interface ScreenPermissionStatus {
  granted: boolean;
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
}
