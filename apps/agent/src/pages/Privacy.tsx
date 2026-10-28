import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { type AppContext, workspaceName } from "../context";
import { formatDay, formatTime, formatWhen } from "../i18n";
import type { PersonView, PersonViews, PersonViewsError } from "../types";
import { PageTitle, Row, Switch } from "../ui";

/// What leaves this computer, the update check (the one automatic request),
/// the log of everything sent exactly as sent, who viewed the person's app
/// usage (connected only), and what the app reads and never reads.
export function PrivacyPage({ ctx }: { ctx: AppContext }) {
  const { s, lang, status, payloads, busy, saas } = ctx;
  const t = s.privacy;
  return (
    <>
      <PageTitle icon="privacy">{s.nav.privacy}</PageTitle>
      <dl className="facts">
        <dt>{t.leaves}</dt>
        <dd>{status.enrolled ? t.leavesConnected(workspaceName(ctx)) : t.leavesNothing}</dd>
        <dt>{t.appUsage}</dt>
        <dd>{status.enrolled && status.sharingConsent ? t.shared : t.notShared}</dd>
        <dt>{t.saas}</dt>
        <dd>{status.enrolled && saas?.saasSharing ? t.shared : t.notShared}</dd>
      </dl>
      <details className="more">
        <summary>{t.details}</summary>
        <p>{status.enrolled ? t.leavesConnectedLong : t.leavesLocalLong}</p>
      </details>

      <div className="list">
        <Row
          title={t.updateChecks}
          detail={
            status.updateChecks
              ? status.updateCheckError
                ? t.lastFailed(status.updateCheckError)
                : t.updateHint(formatWhen(status.lastUpdateCheckAt, lang))
              : t.checksOff
          }
        >
          <Switch
            checked={status.updateChecks}
            label={t.updateChecks}
            disabled={busy}
            onChange={(enabled) => ctx.run("set_update_checks", { enabled })}
          />
        </Row>
      </div>

      <div className="label">{t.sent}</div>
      <div className="list">
        {payloads.length === 0 ? (
          <div className="empty">{t.nothingSent}</div>
        ) : (
          payloads.map((payload) => (
            <details className="payload" key={payload.sentAt + payload.endpoint}>
              <summary>
                <span className="text" style={{ flex: 1, minWidth: 0 }}>
                  <b style={{ fontWeight: 500 }}>POST {payload.endpoint}</b>
                  {payload.delivered ? null : <span className="hint"> · {t.unconfirmed}</span>}
                </span>
                <span className="aside" style={{ color: "var(--tx2)" }}>
                  {formatWhen(payload.sentAt, lang)}
                </span>
              </summary>
              <pre>{JSON.stringify(payload.body, null, 2)}</pre>
            </details>
          ))
        )}
      </div>

      {status.enrolled ? <WhoViewed ctx={ctx} /> : null}

      <div className="box">
        <h3>{t.readsTitle}</h3>
        <p>{t.readsShort}</p>
      </div>
      <div className="box">
        <h3>{t.neverTitle}</h3>
        <p>{t.neverShort}</p>
      </div>
      <details className="more">
        <summary>{t.details}</summary>
        <p>{t.readsTitle}</p>
        <ul>
          {t.reads.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        <p>{t.neverTitle}</p>
        <ul>
          {t.never.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </details>
    </>
  );
}

function viewsError(err: unknown): PersonViewsError {
  if (err && typeof err === "object" && "message" in err) {
    const { offline, message } = err as Partial<PersonViewsError>;
    return { offline: offline === true, message: String(message) };
  }
  return { offline: false, message: String(err) };
}

/// "Who viewed your app usage": connected only. Asked from the workspace each
/// time this page opens (or the person asks again), never in the background;
/// a computer that isn't connected never renders this.
function WhoViewed({ ctx }: { ctx: AppContext }) {
  const { s, lang, status, refresh } = ctx;
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
  }, [status.deviceId, load]);

  const viewer = (view: PersonView) => view.viewerName?.trim() || t.viewerUnnamed;
  const days = (view: PersonView) =>
    view.from === view.to
      ? t.viewedDay(formatDay(view.from, lang))
      : t.viewedDays(formatDay(view.from, lang), formatDay(view.to, lang));

  return (
    <>
      <div className="label">{t.viewsTitle}</div>
      <p className="hint">{t.viewsIntro}</p>
      <div className="list">
        {loading ? (
          <div className="empty">{s.loading}</div>
        ) : failure ? (
          <Row title={failure.offline ? t.viewsOffline : t.viewsError(failure.message)}>
            <button className="button" onClick={load}>
              {t.viewsRetry}
            </button>
          </Row>
        ) : views === null ? null : views.views.length === 0 ? (
          <div className="empty">{t.viewsEmpty}</div>
        ) : (
          views.views.map((view, index) => (
            <Row
              key={`${view.viewedAt}-${index}`}
              top
              title={viewer(view)}
              detail={
                <>
                  {days(view)}
                  <br />
                  {t.viewReason(view.reason)}
                </>
              }
            >
              <span className="aside">{formatTime(view.viewedAt, lang)}</span>
            </Row>
          ))
        )}
      </div>
      {views && views.total > views.views.length ? (
        <p className="hint">{t.viewsShown(views.views.length, views.total)}</p>
      ) : null}
    </>
  );
}
