import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatTime, type Strings } from "./i18n";
import type { Lang, Status } from "./types";

type RunCommand = (command: string, args?: Record<string, unknown>) => Promise<void>;

/// Connected mode: optional. Enrolling with a workspace's code sends this
/// device's checks and installed software to monitor.sanka.com; usage is
/// shared only if the person also opts in.
export function ConnectPane({
  s,
  lang,
  status,
  busy,
  run,
  setStatus,
  setBusy,
  setError,
}: {
  s: Strings;
  lang: Lang;
  status: Status;
  busy: boolean;
  run: RunCommand;
  setStatus: (status: Status) => void;
  setBusy: (busy: boolean) => void;
  setError: (error: string | null) => void;
}) {
  return status.enrolled ? (
    <Connected s={s} lang={lang} status={status} busy={busy} run={run} setError={setError} />
  ) : (
    <Enroll s={s} lang={lang} status={status} busy={busy} setStatus={setStatus} setBusy={setBusy} />
  );
}

function Enroll({
  s,
  lang,
  status,
  busy,
  setStatus,
  setBusy,
}: {
  s: Strings;
  lang: Lang;
  status: Status;
  busy: boolean;
  setStatus: (status: Status) => void;
  setBusy: (busy: boolean) => void;
}) {
  const [code, setCode] = useState("");
  const [consent, setConsent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const t = s.connect;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await invoke<Status>("enroll", { code: code.trim(), sharingConsent: consent }));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section>
      <p className="hint">{t.intro}</p>

      {status.removedByWorkspaceAt ? (
        <p className="notice">{t.removed(formatTime(status.removedByWorkspaceAt, lang))}</p>
      ) : null}
      {status.revokePending ? <p className="notice">{t.revokePending(status.revokeError)}</p> : null}

      <h2 className="gap">{t.codeTitle}</h2>
      <p className="hint">{t.codeHint}</p>
      <div className="enroll-row">
        <input
          type="text"
          value={code}
          placeholder={t.codePlaceholder}
          onChange={(event) => setCode(event.target.value)}
          disabled={busy}
        />
        <button onClick={submit} disabled={busy || code.trim().length < 20}>
          {busy ? t.connecting : t.connect}
        </button>
      </div>
      {error ? <p className="error">{error}</p> : null}
      <label className="consent">
        <input
          type="checkbox"
          checked={consent}
          onChange={(event) => setConsent(event.target.checked)}
          disabled={busy}
        />
        <span>{t.shareOnConnect}</span>
      </label>
    </section>
  );
}

function Connected({
  s,
  lang,
  status,
  busy,
  run,
  setError,
}: {
  s: Strings;
  lang: Lang;
  status: Status;
  busy: boolean;
  run: RunCommand;
  setError: (error: string | null) => void;
}) {
  const nudges = status.lastCheckin?.nudges ?? [];
  const t = s.connect;
  return (
    <section>
      {status.paused ? <p className="notice">{t.paused}</p> : null}
      {status.updateRequired ? (
        <div className="notice">
          <p>{t.updateRequired(status.minAgentVersion)}</p>
          <button
            onClick={() => invoke("install_update").catch((err) => setError(String(err)))}
            disabled={busy}
          >
            {t.updateNow}
          </button>
        </div>
      ) : null}
      {status.lastError ? <p className="error">{t.lastError(status.lastError)}</p> : null}

      {nudges.length > 0 ? (
        <>
          <h2>{t.fromWorkspace}</h2>
          <ul className="nudges">
            {nudges.map((nudge) => (
              <li key={nudge.id}>
                <strong>{nudge.title}</strong>
                <span className="detail">{nudge.body}</span>
                {nudge.url ? (
                  <button
                    className="link"
                    onClick={() =>
                      invoke("open_nudge", { id: nudge.id }).catch((err) => setError(String(err)))
                    }
                  >
                    {t.open}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
        </>
      ) : null}

      <dl className="facts">
        <div>
          <dt>{t.workspace}</dt>
          <dd>{status.workspaceId ?? "—"}</dd>
        </div>
        <div>
          <dt>{t.lastCheckin}</dt>
          <dd>{formatTime(status.lastCheckinAt, lang)}</dd>
        </div>
        <div>
          <dt>{t.installedSoftware}</dt>
          <dd>
            {status.softwareSentAt
              ? t.softwareSent(status.softwareCount ?? 0, formatTime(status.softwareSentAt, lang))
              : t.notSent}
          </dd>
        </div>
        <div>
          <dt>{t.usageSharing}</dt>
          <dd>{status.sharingConsent ? t.sharingOn : t.sharingOff}</dd>
        </div>
      </dl>
      <div className="actions">
        <button onClick={() => run("checkin_now")} disabled={busy}>
          {busy ? t.working : t.checkinNow}
        </button>
      </div>
      <label className="consent">
        <input
          type="checkbox"
          checked={status.launchAtLogin}
          onChange={(event) => run("set_launch_at_login", { enabled: event.target.checked })}
          disabled={busy}
        />
        <span>{t.launchAtLogin}</span>
      </label>

      <h2 className="gap">{t.usageTitle}</h2>
      <label className="consent">
        <input
          type="checkbox"
          checked={status.sharingConsent}
          onChange={(event) => run("set_sharing_consent", { enabled: event.target.checked })}
          disabled={busy}
        />
        <span>{t.shareUsage}</span>
      </label>
      {status.sharingConsent ? (
        <>
          <p className="hint">{t.appsToday}</p>
          {status.todayUsage.length === 0 ? (
            <p className="hint">{status.paused ? t.pausedShort : t.noneToday}</p>
          ) : (
            <ul className="usage">
              {status.todayUsage.map((app) => (
                <li key={app.appId}>
                  <span>{app.appName}</span>
                </li>
              ))}
            </ul>
          )}
          <p className="hint">
            {t.sharedFrom(formatTime(status.sharingConsentAt, lang), status.pendingUploadCount)}
          </p>
        </>
      ) : null}

      <p className="hint gap">
        <button className="link" onClick={() => run("unenroll")} disabled={busy}>
          {t.disconnect}
        </button>
        {t.disconnectExplain}
      </p>
    </section>
  );
}
