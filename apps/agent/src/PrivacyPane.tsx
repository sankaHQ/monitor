import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatDay, formatTime, type Strings } from "./i18n";
import { BrowserCheck } from "./SaasPane";
import type {
  Lang,
  PersonView,
  PersonViews,
  PersonViewsError,
  SaasSettings,
  ScanView,
  SentPayload,
  Status,
} from "./types";

export function PrivacyPane({
  s,
  lang,
  status,
  payloads,
  busy,
  run,
  refresh,
  scan,
  scanning,
  saas,
  onSaas,
  onScan,
  onScanChanged,
  setError,
}: {
  s: Strings;
  lang: Lang;
  status: Status;
  payloads: SentPayload[];
  busy: boolean;
  run: (command: string, args?: Record<string, unknown>) => Promise<void>;
  refresh: () => Promise<void>;
  scan: ScanView | null;
  scanning: boolean;
  saas: SaasSettings | null;
  onSaas: (settings: SaasSettings) => void;
  onScan: () => void;
  onScanChanged: () => void;
  setError: (error: string | null) => void;
}) {
  const t = s.privacy;
  return (
    <section>
      <h2>{t.leavesTitle}</h2>
      <p className="hint">{status.enrolled ? t.leavesConnected : t.leavesLocal}</p>

      <label className="consent">
        <input
          type="checkbox"
          checked={status.updateChecks}
          onChange={(event) => run("set_update_checks", { enabled: event.target.checked })}
          disabled={busy}
        />
        <span>
          {t.updateChecks}
          {status.updateChecks
            ? t.lastCheck(formatTime(status.lastUpdateCheckAt, lang))
            : t.checksOff}
          {status.updateChecks && status.updateCheckError
            ? t.lastAttemptFailed(status.updateCheckError)
            : ""}
        </span>
      </label>

      {status.enrolled ? (
        <WhoOpened s={s} lang={lang} deviceId={status.deviceId} refresh={refresh} />
      ) : null}

      <BrowserCheck
        s={s}
        lang={lang}
        settings={saas}
        onSettings={onSaas}
        setError={setError}
        scan={scan}
        scanning={scanning}
        connected={status.enrolled}
        onScan={onScan}
        onScanChanged={onScanChanged}
      />

      <h2 className="gap">{t.readsTitle}</h2>
      <ul>
        {t.reads.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>

      <h2 className="gap">{t.neverTitle}</h2>
      <ul className="never">
        {t.never.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>

      {status.enrolled || payloads.length > 0 ? (
        <>
          <h2 className="gap">{t.sentTitle}</h2>
          {payloads.length === 0 ? (
            <p className="hint">{t.nothingSent}</p>
          ) : (
            payloads.map((payload) => (
              <details key={payload.sentAt + payload.endpoint}>
                <summary>
                  {formatTime(payload.sentAt, lang)} · POST {payload.endpoint}
                  {payload.delivered ? "" : t.deliveryUnconfirmed}
                </summary>
                <pre>{JSON.stringify(payload.body, null, 2)}</pre>
              </details>
            ))
          )}
        </>
      ) : null}
    </section>
  );
}

function viewsError(err: unknown): PersonViewsError {
  if (err && typeof err === "object" && "message" in err) {
    const { offline, message } = err as Partial<PersonViewsError>;
    return { offline: offline === true, message: String(message) };
  }
  return { offline: false, message: String(err) };
}

/// "Who opened your data": connected mode only. Asked from the workspace
/// each time this pane opens (or the person asks again), never in the
/// background; a computer that is not connected never renders this.
function WhoOpened({
  s,
  lang,
  deviceId,
  refresh,
}: {
  s: Strings;
  lang: Lang;
  deviceId: string | null;
  refresh: () => Promise<void>;
}) {
  const t = s.privacy;
  const [views, setViews] = useState<PersonViews | null>(null);
  const [failure, setFailure] = useState<PersonViewsError | null>(null);
  const [loading, setLoading] = useState(true);
  const latest = useRef(0);

  const load = useCallback(async () => {
    const request = ++latest.current;
    setLoading(true);
    setFailure(null);
    try {
      const answer = await invoke<PersonViews | null>("person_views");
      if (request !== latest.current) return;
      setViews(answer);
      // Not connected any more: the status refresh hides this section.
      if (answer === null) await refresh();
    } catch (err) {
      if (request !== latest.current) return;
      setViews(null);
      setFailure(viewsError(err));
      // A device the workspace removed has just disconnected itself.
      await refresh();
    } finally {
      if (request === latest.current) setLoading(false);
    }
  }, [refresh]);

  useEffect(() => {
    load();
  }, [deviceId, load]);

  const viewer = (view: PersonView) => view.viewerName?.trim() || t.viewerUnnamed;
  const days = (view: PersonView) =>
    view.from === view.to
      ? t.viewedDay(formatDay(view.from, lang))
      : t.viewedDays(formatDay(view.from, lang), formatDay(view.to, lang));

  return (
    <>
      <h2 className="gap">{t.viewsTitle}</h2>
      <p className="hint">{t.viewsIntro}</p>
      {loading ? (
        <p className="hint">{s.loading}</p>
      ) : failure ? (
        <div className="row views-failed">
          <p className="error">{failure.offline ? t.viewsOffline : t.viewsError(failure.message)}</p>
          <button className="link" onClick={load}>
            {t.viewsRetry}
          </button>
        </div>
      ) : views === null ? null : views.views.length === 0 ? (
        <p className="hint">{t.viewsEmpty}</p>
      ) : (
        <>
          <ul className="views">
            {views.views.map((view, index) => (
              <li key={`${view.viewedAt}-${index}`}>
                <div>
                  <span className="view-head">
                    <b>{viewer(view)}</b>
                    <span className="detail">{formatTime(view.viewedAt, lang)}</span>
                  </span>
                  <span className="detail">{days(view)}</span>
                  <span>{t.viewReason(view.reason)}</span>
                </div>
              </li>
            ))}
          </ul>
          {views.total > views.views.length ? (
            <p className="hint">{t.viewsShown(views.views.length, views.total)}</p>
          ) : null}
        </>
      )}
    </>
  );
}
