// Shapes the Rust commands return (camelCase serde).

export type Lang = "en" | "ja";

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

/** The Sanka account the app sends reports as (`get_account`). */
export type Account = {
  signedIn: boolean;
  userName: string | null;
  userEmail: string | null;
  workspaceName: string | null;
  /** A sign-in is waiting for the browser. */
  signingIn: boolean;
  /** Why the last sign-in from the browser failed. */
  error: string | null;
};

/** The counts the app shows beside a report it built, and sends as shown. */
export type ReportSummary = {
  high: number;
  medium: number;
  low: number;
  programs: number;
  programsPastSupport: number;
};

/** Exactly what Send will send (`prepare_send`, `prepare_edited_excel`). */
export type SendPreview = {
  token: string;
  fileName: string;
  format: string;
  sizeBytes: number;
  sha256: string;
  /** The scan the report was built from; null for an edited workbook. */
  scan: {
    hostname: string;
    deviceModel: string | null;
    osName: string;
    scannedAt: string;
    summary: ReportSummary;
  } | null;
};

export type SendResult = {
  receipt: {
    id: string;
    receivedAt: string;
    fileName: string;
    sizeBytes: number;
    sha256: string;
  };
  matches: boolean;
};
