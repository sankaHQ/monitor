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
  workspaceName: string | null;
  /** Device management connected this computer. */
  managed: boolean;
  /** Disconnect is offered (false when device management doesn't allow it). */
  canDisconnect: boolean;
  ownerEmail: string | null;
  enrollmentMethod: EnrollmentMethod | null;
  /** Connected, waiting for an admin to approve the device. */
  pendingApproval: boolean;
  /** A newer version is downloaded and installs on restart. */
  updateReady: UpdateInfo | null;
  /** This launch is the first after an update. */
  justUpdated: UpdateInfo | null;
  /** No version of the app ran here before: open on the welcome screen. */
  firstRun: boolean;
};

export type EnrollmentMethod = "code" | "invite" | "deployment_token" | "sign_in";

export type UpdateInfo = { version: string; notes: string | null };

/** Who an invite link is from (`preview_invite`). */
export type InvitePreview = {
  workspaceName: string;
  invitedBy: string | null;
  email: string | null;
  expiresAt: string;
};

export type CheckId =
  | "disk_encryption"
  | "firewall"
  | "gatekeeper"
  | "screen_lock"
  | "os_update"
  | "automatic_updates"
  | "antivirus"
  | "local_admin"
  | "device_management"
  | "secure_boot"
  | "system_integrity";

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
    /** The failed check behind a device finding. */
    check: CheckId | null;
    severity: Severity;
    severityLabel: string;
    areaLabel: string;
    title: string;
    evidence: string;
    fix: string;
  }[];
  checks: ScanCheck[];
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
  saas: SaasItem[];
  saasCheck: {
    browserCheck: boolean;
    browsers: BrowserResult[];
    browserSummary: string;
    allowlistSummary: string;
    catalogVerifiedOn: string | null;
  };
};

export type ScanCheck = {
  id: CheckId;
  /** The short name the sidebar shows. */
  name: string;
  label: string;
  why: string;
  pass: boolean | null;
  resultLabel: string;
  detail: string | null;
  /** For a failed check: its finding's severity and fix. */
  severity: Severity | null;
  severityLabel: string | null;
  fix: string | null;
};

/** A SaaS this computer uses. It carries no web addresses. */
export type SaasItem = {
  key: string;
  product: string;
  vendor: string;
  categoryLabel: string;
  ai: boolean;
  foundFrom: string;
  sources: string;
  allowed: boolean | null;
  allowedLabel: string;
  migrationLabel: string;
  migratable: boolean;
};

export type BrowserResult = {
  browser: "chrome" | "edge" | "firefox" | "safari";
  status: "read" | "not_found" | "needs_full_disk_access" | "unreadable";
  profiles: number;
};

export type CheckSchedule = "daily" | "weekly";

/** A risk a regular check found that the scan before it did not have. */
export type LocalNudge = {
  id: string;
  at: string;
  severity: Severity;
  saas: boolean;
  titleEn: string;
  titleJa: string;
  fixEn: string;
  fixJa: string;
};

export type SaasSettings = {
  browserCheck: boolean;
  /** This connected PC shares the SaaS it used with its workspace. */
  saasSharing: boolean;
  saasSharingHeld: boolean;
  allowlist: {
    origin: "command_line" | "machine" | "workspace" | "app" | null;
    name: string | null;
    allowed: number;
    unknown: string[];
    error: string | null;
    machinePath: string | null;
    chosen: boolean;
    summary: string;
  };
  schedule: CheckSchedule | null;
  lastScheduledCheckAt: string | null;
  nudges: LocalNudge[];
  catalogVerifiedOn: string;
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
    saas: number;
    saasNotAllowed: number;
    /** The SaaS sheet includes each SaaS's last day used and days used. */
    saasBrowserCheck: boolean;
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
