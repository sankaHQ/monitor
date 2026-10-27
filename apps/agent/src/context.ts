import type { Strings } from "./i18n";
import type { Account, Lang, SaasSettings, SavedReport, ScanView, SentPayload, Status, Theme } from "./types";

/** The sidebar's destinations; a check's page is `check:<id>`. */
export type Screen =
  | "overview"
  | "risks"
  | "software"
  | "saas"
  | "reports"
  | "workspace"
  | "privacy"
  | "settings"
  | `check:${string}`;

/** What every page reads and can change, from App. */
export type AppContext = {
  s: Strings;
  lang: Lang;
  status: Status;
  payloads: SentPayload[];
  scan: ScanView | null;
  scanning: boolean;
  saas: SaasSettings | null;
  account: Account | null;
  saved: SavedReport[];
  busy: boolean;
  reportLang: Lang;
  theme: Theme;
  go: (screen: Screen) => void;
  /** Runs a command that answers with the status, and reloads the sent log. */
  run: (command: string, args?: Record<string, unknown>) => Promise<void>;
  refresh: () => Promise<void>;
  runScan: () => Promise<void>;
  refreshScan: () => void;
  setSaas: (settings: SaasSettings) => void;
  setSaved: (saved: SavedReport[]) => void;
  setAccount: (account: Account) => void;
  setStatus: (status: Status) => void;
  setBusy: (busy: boolean) => void;
  setError: (error: string | null) => void;
  setReportLang: (lang: Lang) => void;
  setTheme: (theme: Theme) => void;
  openSend: () => void;
  restartToUpdate: () => void;
};

/** The workspace's name, or "your workspace" before an older API said it. */
export function workspaceName(ctx: AppContext): string {
  return ctx.status.workspaceName ?? ctx.s.workspace.thisWorkspace;
}
