import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatTime, type Strings } from "./i18n";
import type { CheckSchedule, Lang, SaasSettings, ScanView } from "./types";

type SettingsProps = {
  s: Strings;
  lang: Lang;
  settings: SaasSettings | null;
  onSettings: (settings: SaasSettings) => void;
  setError: (error: string | null) => void;
};

/// The browser check, as the SaaS tab and the Privacy pane both show it:
/// the person's checkbox and exactly what it reads and keeps. Turning it on
/// scans again; turning it off drops its results from the scan shown. It
/// cannot change while a scan runs.
export function BrowserCheck({
  s,
  lang,
  settings,
  onSettings,
  setError,
  scan,
  scanning,
  connected,
  onScan,
  onScanChanged,
}: SettingsProps & {
  scan: ScanView | null;
  scanning: boolean;
  /** Connected to a workspace: sharing the SaaS this PC used is offered. */
  connected: boolean;
  onScan: () => void;
  onScanChanged: () => void;
}) {
  const t = s.saas;
  const [saving, setSaving] = useState(false);
  const toggle = async (enabled: boolean) => {
    setSaving(true);
    setError(null);
    try {
      onSettings(await invoke<SaasSettings>("set_browser_check", { enabled, lang }));
      if (enabled) onScan();
      else onScanChanged();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };
  const share = async (enabled: boolean) => {
    setSaving(true);
    setError(null);
    try {
      onSettings(await invoke<SaasSettings>("set_saas_sharing", { enabled, lang }));
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };
  const safariBlocked = scan?.saasCheck.browsers.some(
    (b) => b.browser === "safari" && b.status === "needs_full_disk_access",
  );
  return (
    <>
      <h2 className="gap">{t.browserTitle}</h2>
      <label className="consent">
        <input
          type="checkbox"
          checked={settings?.browserCheck ?? false}
          disabled={settings === null || saving || scanning}
          onChange={(event) => toggle(event.target.checked)}
        />
        <span>
          <strong className="consent-label">{t.browserCheck}</strong>
          {t.browserExplain}
        </span>
      </label>
      {scan && settings?.browserCheck && scan.saasCheck.browserCheck ? (
        <p className="hint">{t.thisScan(scan.saasCheck.browserSummary)}</p>
      ) : null}
      {safariBlocked ? <p className="hint">{t.safariHint}</p> : null}
      {connected && settings?.browserCheck ? (
        <label className="consent">
          <input
            type="checkbox"
            checked={settings.saasSharing}
            disabled={saving || scanning}
            onChange={(event) => share(event.target.checked)}
          />
          <span>
            <strong className="consent-label">{t.shareCheck}</strong>
            {t.shareExplain}
          </span>
        </label>
      ) : null}
      {connected && settings?.saasSharingHeld ? (
        <p className="hint">{t.shareHeld}</p>
      ) : null}
    </>
  );
}

/// Regular checks: off, daily or weekly. Shown on the Scan tab.
export function RegularChecks({ s, lang, settings, onSettings, setError }: SettingsProps) {
  const t = s.checks;
  const [saving, setSaving] = useState(false);
  const set = async (schedule: CheckSchedule | null) => {
    setSaving(true);
    setError(null);
    try {
      onSettings(await invoke<SaasSettings>("set_check_schedule", { schedule, lang }));
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };
  const schedule = settings?.schedule ?? null;
  return (
    <section>
      <h2>{t.title}</h2>
      <label className="consent">
        <input
          type="checkbox"
          checked={schedule !== null}
          disabled={settings === null || saving}
          onChange={(event) => set(event.target.checked ? "daily" : null)}
        />
        <span>
          {t.regular} {t.regularHint}
        </span>
      </label>
      {schedule !== null ? (
        <div className="row">
          <div className="seg" role="group" aria-label={t.title}>
            {(["daily", "weekly"] as const).map((option) => (
              <button
                key={option}
                className={schedule === option ? "seg-item active" : "seg-item"}
                disabled={saving}
                onClick={() => set(option)}
              >
                {option === "daily" ? t.daily : t.weekly}
              </button>
            ))}
          </div>
          {settings?.lastScheduledCheckAt ? (
            <span className="hint">{t.last(formatTime(settings.lastScheduledCheckAt, lang))}</span>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

/// What the regular checks found that the scan before did not have.
export function Nudges({ s, lang, settings, onSettings, setError }: SettingsProps) {
  const t = s.checks;
  const nudges = settings?.nudges ?? [];
  if (nudges.length === 0) return null;
  const dismiss = async () => {
    try {
      onSettings(await invoke<SaasSettings>("dismiss_nudges", { lang }));
    } catch (err) {
      setError(String(err));
    }
  };
  return (
    <section className="notice">
      <h2>{t.nudgesTitle}</h2>
      <ul className="nudges">
        {nudges.map((nudge) => (
          <li key={nudge.id}>
            <span className={`sev ${nudge.severity}`}>{formatTime(nudge.at, lang)}</span>{" "}
            <strong>{lang === "ja" ? nudge.titleJa : nudge.titleEn}</strong>
            <span className="detail">{lang === "ja" ? nudge.fixJa : nudge.fixEn}</span>
          </li>
        ))}
      </ul>
      <button className="link" onClick={dismiss}>
        {t.dismiss}
      </button>
    </section>
  );
}

export function SaasPane({
  s,
  lang,
  scan,
  scanning,
  connected,
  settings,
  onSettings,
  setError,
  onScan,
  onScanChanged,
}: SettingsProps & {
  scan: ScanView | null;
  scanning: boolean;
  connected: boolean;
  onScan: () => void;
  onScanChanged: () => void;
}) {
  const t = s.saas;
  const [onlyNotAllowed, setOnlyNotAllowed] = useState(false);
  const [busy, setBusy] = useState(false);
  const allowlist = settings?.allowlist;

  const allowlistCommand = async (command: "choose_allowlist" | "clear_allowlist") => {
    setBusy(true);
    setError(null);
    try {
      onSettings(await invoke<SaasSettings>(command, { lang }));
      onScanChanged();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };
  const openMigration = (key: string) =>
    invoke("open_migration_page", { key, lang }).catch((err) => setError(String(err)));

  const notAllowed = scan?.saas.filter((item) => item.allowed === false) ?? [];
  const rows = onlyNotAllowed ? notAllowed : (scan?.saas ?? []);

  return (
    <section>
      <p className="hint">{t.intro}</p>

      <BrowserCheck
        s={s}
        lang={lang}
        settings={settings}
        onSettings={onSettings}
        setError={setError}
        scan={scan}
        scanning={scanning}
        connected={connected}
        onScan={onScan}
        onScanChanged={onScanChanged}
      />

      <h2 className="gap">{t.allowlistTitle}</h2>
      <p className="hint">{t.allowlistExplain}</p>
      {allowlist ? (
        <>
          <p>{allowlist.error ? null : allowlist.summary}</p>
          {allowlist.error ? <p className="error">{t.allowlistError(allowlist.error)}</p> : null}
          {allowlist.origin === "machine" || allowlist.origin === "workspace" ? (
            <p className="hint">
              {allowlist.origin === "machine" ? t.machineInUse : t.workspaceInUse}
            </p>
          ) : (
            <div className="row">
              <button onClick={() => allowlistCommand("choose_allowlist")} disabled={busy}>
                {t.chooseFile}
              </button>
              {allowlist.chosen ? (
                <button
                  className="link"
                  onClick={() => allowlistCommand("clear_allowlist")}
                  disabled={busy}
                >
                  {t.stopUsing}
                </button>
              ) : null}
            </div>
          )}
          {allowlist.machinePath && allowlist.origin !== "machine" ? (
            <p className="hint">{t.machinePath(allowlist.machinePath)}</p>
          ) : null}
        </>
      ) : null}

      <h2 className="gap">{t.listTitle}</h2>
      {scan === null ? (
        <p className="hint">{t.empty}</p>
      ) : (
        <>
          {notAllowed.length > 0 ? (
            <label className="consent">
              <input
                type="checkbox"
                checked={onlyNotAllowed}
                onChange={(event) => setOnlyNotAllowed(event.target.checked)}
              />
              <span>{t.onlyNotAllowed(notAllowed.length)}</span>
            </label>
          ) : null}
          {rows.length === 0 ? (
            <p className="hint">{t.none}</p>
          ) : (
            <ul className="software saas">
              {rows.map((item) => (
                <li key={item.key}>
                  <div>
                    <strong>
                      {item.product}
                      {item.ai ? <span className="badge">AI</span> : null}
                    </strong>
                    <span className="detail">
                      {item.vendor} · {item.categoryLabel}
                    </span>
                    <span className="detail">{item.sources}</span>
                    {item.migratable ? (
                      <button className="link" onClick={() => openMigration(item.key)}>
                        {t.migrationPage} · {item.migrationLabel}
                      </button>
                    ) : null}
                  </div>
                  <span className={item.allowed === false ? "sw-status flagged" : "sw-status"}>
                    {item.allowedLabel}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <p className="hint">{t.footer(settings?.catalogVerifiedOn ?? "—")}</p>
        </>
      )}
    </section>
  );
}
