import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { STRINGS, systemLang } from "./i18n";
import type { AppContext, Screen } from "./context";
import { FirstRun } from "./pages/FirstRun";
import { Overview, Risks, CheckPage } from "./pages/Overview";
import { PrivacyPage } from "./pages/Privacy";
import { Reports, SendSheet } from "./pages/Reports";
import { SaasPage } from "./pages/Saas";
import { SettingsPage } from "./pages/Settings";
import { SoftwarePage } from "./pages/Software";
import { JoinSheet, WorkspacePage } from "./pages/Workspace";
import { Sidebar } from "./Sidebar";
import type { Account, Lang, SaasSettings, SavedReport, ScanView, SentPayload, Status, Theme } from "./types";

// The app follows the OS language. Reports can be saved in another language,
// chosen in Reports and Settings; the key predates that and keeps an earlier choice.
const REPORT_LANG_KEY = "sanka-monitor.report-lang";
// System, Light or Dark, chosen in Settings.
const THEME_KEY = "sanka-monitor.theme";
const THEMES: readonly Theme[] = ["system", "light", "dark"];

function stored<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    if (value && (allowed as readonly string[]).includes(value)) return value as T;
  } catch {
    // Storage can be unavailable; use the fallback.
  }
  return fallback;
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered; the choice still applies now.
  }
}

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [payloads, setPayloads] = useState<SentPayload[]>([]);
  const [screen, setScreen] = useState<Screen>("overview");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lang] = useState<Lang>(systemLang);
  const [reportLang, setReportLangState] = useState<Lang>(() =>
    stored(REPORT_LANG_KEY, Object.keys(STRINGS) as Lang[], systemLang()),
  );
  const [theme, setThemeState] = useState<Theme>(() => stored(THEME_KEY, THEMES, "system"));
  const [scan, setScan] = useState<ScanView | null>(null);
  const [scanning, setScanning] = useState(false);
  const [saved, setSaved] = useState<SavedReport[]>([]);
  const [account, setAccount] = useState<Account | null>(null);
  const [saas, setSaas] = useState<SaasSettings | null>(null);
  const [sendOpen, setSendOpen] = useState(false);
  const [invite, setInvite] = useState<string | null>(null);
  const [firstRun, setFirstRun] = useState<boolean | null>(null);
  // undefined until the settings first load; then the last regular check's
  // time, or null before the first one.
  const lastRegularCheck = useRef<string | null | undefined>(undefined);
  const scanned = useRef(false);
  const s = STRINGS[lang];

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  // The page's colours and the window's title bar follow the choice; "system"
  // hands both back to the OS appearance.
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    try {
      getCurrentWindow()
        .setTheme(theme === "system" ? null : theme)
        .catch(() => {});
    } catch {
      // Outside the app window (a browser preview) there is no title bar to set.
    }
  }, [theme]);

  const refresh = useCallback(async () => {
    try {
      const next = await invoke<Status>("get_status");
      setStatus(next);
      setFirstRun((current) => (current === null ? next.firstRun : current));
      setPayloads(await invoke<SentPayload[]>("get_transparency"));
    } catch {
      // Status polling failures surface via lastError on the next success.
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 30_000);
    const unlisten = listen("status-changed", () => refresh());
    return () => {
      clearInterval(timer);
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [refresh]);

  // The account the app sends reports as; a sign-in finishes in the browser.
  useEffect(() => {
    invoke<Account>("get_account").then(setAccount).catch(() => {});
    const unlisten = listen<Account>("account-changed", (event) => {
      setAccount(event.payload);
      // The sign-in request is listed in the Privacy page.
      refresh();
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [refresh]);

  // An invite link (sanka-monitor://enroll?invite=…) opens the connect sheet,
  // including one that launched the app before this window loaded.
  useEffect(() => {
    invoke<string | null>("take_pending_invite")
      .then((pending) => pending && setInvite(pending))
      .catch(() => {});
    const unlisten = listen<string>("enroll-invite", (event) => {
      invoke("take_pending_invite").catch(() => {});
      setInvite(event.payload);
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);

  // The scan the app holds, again: after the browser check or the allowlist
  // changed it, after a regular check replaced it, or a scan from the tray.
  const refreshScan = useCallback(() => {
    invoke<ScanView | null>("get_scan", { lang })
      .then((view) => view && setScan(view))
      .catch(() => {});
  }, [lang]);

  // A check-in that brought a new workspace allowlist re-marked the scan the
  // app holds: show it, and the list now in force, at once.
  useEffect(() => {
    const unlisten = listen("scan-changed", () => {
      refreshScan();
      invoke<SaasSettings>("get_saas_settings", { lang })
        .then(setSaas)
        .catch(() => {});
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [lang, refreshScan]);

  // SaaS settings and regular-check nudges; a regular check that ran since
  // the last look brings its scan with it.
  useEffect(() => {
    const load = () =>
      invoke<SaasSettings>("get_saas_settings", { lang })
        .then((next) => {
          setSaas(next);
          if (next.lastScheduledCheckAt !== lastRegularCheck.current) {
            if (lastRegularCheck.current !== undefined) refreshScan();
            lastRegularCheck.current = next.lastScheduledCheckAt;
          }
        })
        .catch(() => {});
    load();
    const timer = setInterval(load, 30_000);
    return () => clearInterval(timer);
  }, [lang, refreshScan]);

  const runScan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      setScan(await invoke<ScanView>("scan_device", { lang }));
    } catch (err) {
      setError(String(err));
    } finally {
      setScanning(false);
    }
  }, [lang]);

  // The first open scans at once: reading this computer needs no account.
  useEffect(() => {
    if (scanned.current) return;
    scanned.current = true;
    invoke<ScanView | null>("get_scan", { lang }).then((existing) => {
      if (existing) setScan(existing);
      else runScan();
    });
    invoke<SavedReport[]>("get_saved_reports").then(setSaved).catch(() => {});
  }, [lang, runScan]);

  const setReportLang = (next: Lang) => {
    setReportLangState(next);
    remember(REPORT_LANG_KEY, next);
  };

  const setTheme = (next: Theme) => {
    setThemeState(next);
    remember(THEME_KEY, next);
  };

  const run = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await invoke<Status>(command, args));
      setPayloads(await invoke<SentPayload[]>("get_transparency"));
    } catch (err) {
      setError(String(err));
      // A failed command can still change state: a check-in that learns the
      // workspace removed this device disconnects it.
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  const restartToUpdate = () => {
    invoke("install_update").catch((err) => setError(String(err)));
  };

  const go = (next: Screen) => {
    setScreen(next);
    setError(null);
  };

  if (status === null || firstRun === null) {
    return <div className="first">{s.loading}</div>;
  }

  const ctx: AppContext = {
    s,
    lang,
    status,
    payloads,
    scan,
    scanning,
    saas,
    account,
    saved,
    busy,
    reportLang,
    theme,
    go,
    run,
    refresh,
    runScan,
    refreshScan,
    setSaas,
    setSaved,
    setAccount,
    setStatus,
    setBusy,
    setError,
    setReportLang,
    setTheme,
    openSend: () => setSendOpen(true),
    restartToUpdate,
  };

  if (firstRun) {
    const done = (next: Screen) => {
      invoke("finish_first_run").catch(() => {});
      setFirstRun(false);
      go(next);
    };
    return <FirstRun ctx={ctx} onDone={done} />;
  }

  return (
    <div className="shell">
      <Sidebar ctx={ctx} screen={screen} />
      <main className="main">
        {error ? <p className="error">{error}</p> : null}
        <Page ctx={ctx} screen={screen} />
      </main>
      {sendOpen && scan ? <SendSheet ctx={ctx} onClose={() => setSendOpen(false)} /> : null}
      {invite && !status.enrolled ? (
        <JoinSheet
          ctx={ctx}
          invite={invite}
          onClose={() => setInvite(null)}
          onJoined={() => {
            setInvite(null);
            go("workspace");
          }}
        />
      ) : null}
    </div>
  );
}

function Page({ ctx, screen }: { ctx: AppContext; screen: Screen }) {
  if (screen.startsWith("check:")) return <CheckPage ctx={ctx} id={screen.slice("check:".length)} />;
  switch (screen) {
    case "risks":
      return <Risks ctx={ctx} />;
    case "software":
      return <SoftwarePage ctx={ctx} />;
    case "saas":
      return <SaasPage ctx={ctx} />;
    case "reports":
      return <Reports ctx={ctx} />;
    case "workspace":
      return <WorkspacePage ctx={ctx} />;
    case "privacy":
      return <PrivacyPage ctx={ctx} />;
    case "settings":
      return <SettingsPage ctx={ctx} />;
    default:
      return <Overview ctx={ctx} />;
  }
}
