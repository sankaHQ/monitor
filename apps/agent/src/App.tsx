import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ConnectPane } from "./ConnectPane";
import { STRINGS, systemLang } from "./i18n";
import { PrivacyPane } from "./PrivacyPane";
import { Nudges, RegularChecks, SaasPane } from "./SaasPane";
import { ScanPane, SoftwarePane } from "./ScanPane";
import type {
  Account,
  Lang,
  SaasSettings,
  SavedReport,
  ScanView,
  SentPayload,
  Status,
} from "./types";

type Tab = "scan" | "software" | "saas" | "connect" | "privacy";

// The app's language, which the reports follow too.
const LANG_KEY = "sanka-monitor.report-lang";

function initialLang(): Lang {
  try {
    const saved = localStorage.getItem(LANG_KEY);
    if (saved === "en" || saved === "ja") return saved;
  } catch {
    // Storage can be unavailable; fall back to the system language.
  }
  return systemLang();
}

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [payloads, setPayloads] = useState<SentPayload[]>([]);
  const [tab, setTab] = useState<Tab>("scan");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lang, setLangState] = useState<Lang>(initialLang);
  const [scan, setScan] = useState<ScanView | null>(null);
  const [scanning, setScanning] = useState(false);
  const [saved, setSaved] = useState<SavedReport[]>([]);
  const [account, setAccount] = useState<Account | null>(null);
  const [saas, setSaas] = useState<SaasSettings | null>(null);
  // undefined until the settings first load; then the last regular check's
  // time, or null before the first one.
  const lastRegularCheck = useRef<string | null | undefined>(undefined);
  const scanned = useRef(false);
  const s = STRINGS[lang];

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  const refresh = useCallback(async () => {
    try {
      setStatus(await invoke<Status>("get_status"));
      setPayloads(await invoke<SentPayload[]>("get_transparency"));
    } catch {
      // Status polling failures surface via lastError on the next success.
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 30_000);
    return () => clearInterval(timer);
  }, [refresh]);

  // The account the app sends reports as; a sign-in finishes in the browser.
  useEffect(() => {
    invoke<Account>("get_account").then(setAccount).catch(() => {});
    const unlisten = listen<Account>("account-changed", (event) => {
      setAccount(event.payload);
      // The sign-in request is listed in the Privacy pane.
      refresh();
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [refresh]);

  // The scan the app holds, again: after the browser check or the allowlist
  // changed it, or after a regular check replaced it.
  const refreshScan = useCallback(() => {
    invoke<ScanView | null>("get_scan", { lang })
      .then((view) => view && setScan(view))
      .catch(() => {});
  }, [lang]);

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

  const setLang = (next: Lang) => {
    setLangState(next);
    try {
      localStorage.setItem(LANG_KEY, next);
    } catch {
      // Not remembered; the choice still applies now.
    }
    invoke<ScanView | null>("get_scan", { lang: next }).then((view) => view && setScan(view));
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

  const languageSwitch = (
    <div className="seg" role="group" aria-label={s.languageLabel}>
      {(["en", "ja"] as const).map((code) => (
        <button
          key={code}
          className={lang === code ? "seg-item active" : "seg-item"}
          onClick={() => setLang(code)}
        >
          {code === "en" ? "English" : "日本語"}
        </button>
      ))}
    </div>
  );

  if (status === null) {
    return (
      <main>
        <header className="app-header">
          <div>
            <h1>{s.appName}</h1>
            <p className="status">{s.loading}</p>
          </div>
          {languageSwitch}
        </header>
      </main>
    );
  }

  const tabs: { key: Tab; label: string }[] = [
    { key: "scan", label: s.tabs.scan },
    { key: "software", label: s.tabs.software },
    { key: "saas", label: s.tabs.saas },
    { key: "connect", label: status.enrolled ? s.tabs.workspace : s.tabs.connect },
    { key: "privacy", label: s.tabs.privacy },
  ];

  return (
    <main>
      <header className="app-header">
        <div>
          <h1>{s.appName}</h1>
          <p className="status">
            v{status.agentVersion} ·{" "}
            {status.enrolled ? s.headerConnected(status.paused) : s.headerLocal}
          </p>
        </div>
        {languageSwitch}
      </header>

      <nav className="tabs" aria-label="Panes">
        {tabs.map(({ key, label }) => (
          <button
            key={key}
            className={tab === key ? "tab active" : "tab"}
            onClick={() => setTab(key)}
          >
            {label}
          </button>
        ))}
      </nav>

      {error ? <p className="error">{error}</p> : null}

      <Nudges s={s} lang={lang} settings={saas} onSettings={setSaas} setError={setError} />

      {tab === "scan" ? (
        <>
          <ScanPane
            s={s}
            scan={scan}
            scanning={scanning}
            lang={lang}
            onScan={runScan}
            saved={saved}
            onSaved={setSaved}
            account={account}
            onAccount={setAccount}
            setError={setError}
          />
          <RegularChecks
            s={s}
            lang={lang}
            settings={saas}
            onSettings={setSaas}
            setError={setError}
          />
        </>
      ) : null}
      {tab === "software" ? <SoftwarePane s={s} scan={scan} /> : null}
      {tab === "saas" ? (
        <SaasPane
          s={s}
          lang={lang}
          scan={scan}
          scanning={scanning}
          settings={saas}
          onSettings={setSaas}
          setError={setError}
          onScan={runScan}
          onScanChanged={refreshScan}
        />
      ) : null}
      {tab === "connect" ? (
        <ConnectPane
          s={s}
          lang={lang}
          status={status}
          busy={busy}
          run={run}
          setStatus={setStatus}
          setBusy={setBusy}
          setError={setError}
        />
      ) : null}
      {tab === "privacy" ? (
        <PrivacyPane
          s={s}
          lang={lang}
          status={status}
          payloads={payloads}
          busy={busy}
          run={run}
          refresh={refresh}
          scan={scan}
          scanning={scanning}
          saas={saas}
          onSaas={setSaas}
          onScan={runScan}
          onScanChanged={refreshScan}
          setError={setError}
        />
      ) : null}
    </main>
  );
}
