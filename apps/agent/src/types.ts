// Shapes the Rust commands return (camelCase serde).

export type Lang = "en" | "ja";

export type Theme = "system" | "light" | "dark";

export type PostureCheck = {
  id: string;
  label: string;
  pass: boolean | null;
  detail?: string | null;
  collectedAt: string;
};

export type Nudge = {
  id: string;
  title: string;
  body: string;
  url?: string | null;
};

export type Status = {
  enrolled: boolean;
  agentVersion: string;
  apiBase: string;
  deviceId: string | null;
  workspaceId: string | null;
  sharingConsent: boolean;
  sharingConsentAt: string | null;
  lastCheckinAt: string | null;
  checkinIntervalSeconds: number;
  lastCheckin: {
    killSwitch: boolean;
    minAgentVersion: string;
    nudges: Nudge[];
  } | null;
  lastError: string | null;
  device: {
    platform: string;
    osVersion: string;
    osEdition: string | null;
    hostname: string;
    serialNumber: string | null;
    deviceModel: string | null;
  };
  posture: PostureCheck[];
  softwareCount: number | null;
  softwareSentAt: string | null;
  collecting: boolean;
  paused: boolean;
  updateRequired: boolean;
  minAgentVersion: string | null;
  launchAtLogin: boolean;
  pendingUploadCount: number;
  todayUsage: { appId: string; appName: string }[];
  revokePending: boolean;
  revokeError: string | null;
  removedByWorkspaceAt: string | null;
  updateChecks: boolean;
  lastUpdateCheckAt: string | null;
  updateCheckError: string | null;
};

export type SentPayload = {
  sentAt: string;
  endpoint: string;
  body: unknown;
  delivered: boolean;
};

/** One time someone in the workspace opened this person's app usage. */
export type PersonView = {
  viewedAt: string;
  /** Calendar days, YYYY-MM-DD. */
  from: string;
  to: string;
  viewerName: string | null;
  reason: string;
};

/** `person_views`: the latest views, newest first; null when not connected. */
export type PersonViews = { views: PersonView[]; total: number };

/** Why `person_views` failed; `offline` when the workspace could not be reached. */
export type PersonViewsError = { offline: boolean; message: string };

export type Severity = "high" | "medium" | "low";
export type LifecycleStatus = "ended" | "ending" | "supported";

export type ScanView = {
  lang: Lang;
  reportId: string;
  scannedAt: string;
  catalogVerifiedOn: string | null;
  device: {
    hostname: string;
    platform: string;
    osVersion: string;
    osEdition: string | null;
    deviceModel: string | null;
    serialNumber: string | null;
  };
  osName: string;
  osSupport: { status: LifecycleStatus; label: string; supportEndsOn: string } | null;
  counts: {
    high: number;
    medium: number;
    low: number;
    checksPassed: number;
    checksFailed: number;
    checksUnknown: number;
    programs: number;
    programsPastSupport: number;
  };
  findings: {
    severity: Severity;
    severityLabel: string;
    areaLabel: string;
    title: string;
    evidence: string;
    fix: string;
  }[];
  checks: {
    label: string;
    pass: boolean | null;
    resultLabel: string;
    detail: string | null;
  }[];
  software: {
    name: string;
    version: string | null;
    publisher: string | null;
    status: LifecycleStatus | null;
    statusLabel: string;
    flagged: boolean;
    supportEndsOn: string | null;
    successor: string | null;
  }[];
};

export type ReportFormat = "xlsx" | "csv" | "html";

export type SavedReport = {
  path: string;
  fileName: string;
  format: ReportFormat;
  sizeBytes: number;
  savedAt: string;
  reportId: string;
};
