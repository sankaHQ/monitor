import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { type AppContext, workspaceName } from "../context";
import { formatTime, formatWhen } from "../i18n";
import type { Account, InvitePreview, Status } from "../types";
import { Icon, PageTitle, Row, Switch } from "../ui";
import { Banners } from "./Overview";

/// Connecting to a company's workspace is optional. Not connected: sign in
/// with the company, or paste a code (invite links open the Join sheet by
/// themselves). Connected: the workspace's facts, Check in now, app-usage
/// sharing (the person's choice, even when device management connected the
/// computer) and Disconnect where allowed.
export function WorkspacePage({ ctx }: { ctx: AppContext }) {
  const { s, status } = ctx;
  return (
    <>
      <Banners ctx={ctx} onWorkspace />
      <PageTitle icon="workspace">{s.nav.workspace}</PageTitle>
      {status.enrolled ? <Connected ctx={ctx} /> : <NotConnected ctx={ctx} />}
    </>
  );
}

function NotConnected({ ctx }: { ctx: AppContext }) {
  const { s, lang, status, account, busy } = ctx;
  const t = s.workspace;
  const [code, setCode] = useState("");
  const [codeOpen, setCodeOpen] = useState(false);
  const [consent, setConsent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [waitingForSignIn, setWaitingForSignIn] = useState(false);

  const enroll = async (command: string, args: Record<string, unknown>) => {
    ctx.setBusy(true);
    setError(null);
    try {
      ctx.setStatus(await invoke<Status>(command, args));
    } catch (err) {
      setError(String(err));
    } finally {
      ctx.setBusy(false);
    }
  };

  // Sign in with the company: the browser sign-in Send to Sanka uses picks
  // the workspace; once the account is back, the computer connects with it.
  const signIn = async () => {
    setError(null);
    if (account?.signedIn) {
      await enroll("enroll_signed_in", { sharingConsent: consent });
      return;
    }
    try {
      ctx.setAccount(await invoke<Account>("begin_sign_in", { lang }));
      setWaitingForSignIn(true);
    } catch (err) {
      setError(String(err));
    }
  };
  useEffect(() => {
    if (waitingForSignIn && account?.signedIn) {
      setWaitingForSignIn(false);
      enroll("enroll_signed_in", { sharingConsent: consent });
    }
  }, [waitingForSignIn, account?.signedIn]);

  return (
    <>
      <dl className="facts">
        <dt>{t.status}</dt>
        <dd>
          <span className="pill unknown">{t.notConnected}</span>
        </dd>
        {status.removedByWorkspaceAt ? (
          <>
            <dt>{t.removedAt}</dt>
            <dd>{formatTime(status.removedByWorkspaceAt, lang)}</dd>
          </>
        ) : null}
      </dl>
      {status.removedByWorkspaceAt ? (
        <div className="box">
          <p>{t.removedNote}</p>
        </div>
      ) : null}
      {status.revokePending ? (
        <div className="banner warn">
          <div className="text">{t.revokePending(status.revokeError)}</div>
        </div>
      ) : null}
      <div className="box">
        <h3>{t.connectTitle}</h3>
        <p>{t.connectHint}</p>
      </div>
      <div className="list">
        <Row title={t.signIn} detail={waitingForSignIn || account?.signingIn ? t.signingIn : t.signInHint}>
          <button className="button primary" onClick={signIn} disabled={busy || waitingForSignIn}>
            {t.signInButton}
          </button>
        </Row>
        <Row title={t.code} detail={t.codeHint}>
          {codeOpen ? null : (
            <button className="button" onClick={() => setCodeOpen(true)}>
              {t.enterCode}
            </button>
          )}
        </Row>
        {codeOpen ? (
          <div className="row">
            <input
              className="input"
              value={code}
              placeholder={t.codePlaceholder}
              aria-label={t.codePlaceholder}
              autoFocus
              onChange={(event) => setCode(event.target.value)}
              disabled={busy}
            />
            <button
              className="button"
              onClick={() => enroll("enroll", { code: code.trim(), sharingConsent: consent })}
              disabled={busy || code.trim().length < 20}
            >
              {busy ? t.connecting : t.connect}
            </button>
          </div>
        ) : null}
        <Row title={t.shareUsage} detail={t.shareUsageHint}>
          <Switch checked={consent} label={t.shareUsage} disabled={busy} onChange={setConsent} />
        </Row>
      </div>
      {error ? <p className="error">{error}</p> : null}
    </>
  );
}

function Connected({ ctx }: { ctx: AppContext }) {
  const { s, lang, status, busy } = ctx;
  const t = s.workspace;
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);
  const name = workspaceName(ctx);
  const pill = status.pendingApproval ? (
    <span className="pill pending">{t.pending}</span>
  ) : (
    <span className="pill pass">{t.connected}</span>
  );

  return (
    <>
      <dl className="facts">
        <dt>{t.status}</dt>
        <dd>
          {pill}
          {status.managed ? <span className="tag">{t.managedBy(name)}</span> : null}
        </dd>
        <dt>{t.workspace}</dt>
        <dd>{status.workspaceName ?? status.workspaceId ?? "—"}</dd>
        {status.ownerEmail ? (
          <>
            <dt>{t.owner}</dt>
            <dd>{status.ownerEmail}</dd>
          </>
        ) : null}
        <dt>{t.lastCheckin}</dt>
        <dd>
          {formatWhen(status.lastCheckinAt, lang)}
          <button className={busy ? "link spin" : "link"} onClick={() => ctx.run("checkin_now")} disabled={busy}>
            <Icon name="refresh" />
            {t.checkinNow}
          </button>
        </dd>
        <dt>{t.programsSent}</dt>
        <dd>
          {status.softwareSentAt
            ? t.softwareSent(status.softwareCount ?? 0, formatWhen(status.softwareSentAt, lang))
            : t.notSent}
        </dd>
        {status.enrollmentMethod ? (
          <>
            <dt>{t.setUpBy}</dt>
            <dd>{t.methods[status.enrollmentMethod]}</dd>
          </>
        ) : null}
      </dl>
      {status.lastError ? <p className="error">{t.lastError(status.lastError)}</p> : null}
      {status.paused ? <p className="hint">{t.paused}</p> : null}

      <div className="label">{t.usage}</div>
      <div className="list">
        <Row title={t.shareUsage} detail={t.shareUsageHint}>
          <Switch
            checked={status.sharingConsent}
            label={t.shareUsage}
            disabled={busy}
            onChange={(enabled) => ctx.run("set_sharing_consent", { enabled })}
          />
        </Row>
        {status.sharingConsent ? (
          <Row
            title={t.usageToday}
            detail={
              status.todayUsage.length > 0
                ? status.todayUsage.map((app) => app.appName).join(", ")
                : status.paused
                  ? t.pausedShort
                  : t.noneToday
            }
          >
            <span className="aside">
              {t.sharedFrom(formatWhen(status.sharingConsentAt, lang), status.pendingUploadCount)}
            </span>
          </Row>
        ) : null}
      </div>

      {status.canDisconnect ? (
        confirmDisconnect ? (
          <div className="banner warn">
            <div className="text">{t.disconnectHint}</div>
            <button className="button" onClick={() => setConfirmDisconnect(false)} disabled={busy}>
              {s.cancel}
            </button>
            <button className="button danger" onClick={() => ctx.run("unenroll")} disabled={busy}>
              {t.disconnect.replace("…", "")}
            </button>
          </div>
        ) : (
          <button className="button danger" onClick={() => setConfirmDisconnect(true)} disabled={busy}>
            {t.disconnect}
          </button>
        )
      ) : (
        <p className="hint">
          <Icon name="lock" /> {t.managedNote}
        </p>
      )}
    </>
  );
}

/// An invite link was opened: who it's from, what connecting shares, and
/// usage sharing as the person's own choice (off).
export function JoinSheet({
  ctx,
  invite,
  onClose,
  onJoined,
}: {
  ctx: AppContext;
  invite: string;
  onClose: () => void;
  onJoined: () => void;
}) {
  const { s } = ctx;
  const t = s.join;
  const [preview, setPreview] = useState<InvitePreview | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [consent, setConsent] = useState(false);
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setPreview(null);
    setInvalid(false);
    invoke<InvitePreview>("preview_invite", { invite })
      .then(setPreview)
      .catch(() => setInvalid(true));
  }, [invite]);

  const join = async () => {
    setJoining(true);
    setError(null);
    try {
      ctx.setStatus(await invoke<Status>("enroll_with_invite", { invite, sharingConsent: consent }));
      onJoined();
    } catch (err) {
      setError(String(err));
    } finally {
      setJoining(false);
    }
  };

  return (
    <div className="sheet" role="dialog" aria-modal="true" aria-label={preview ? t.title(preview.workspaceName) : t.loading}>
      <div className="pane">
        {invalid ? (
          <>
            <p>{t.invalid}</p>
            <div className="actions">
              <button className="button" onClick={onClose}>
                {s.dismiss}
              </button>
            </div>
          </>
        ) : !preview ? (
          <p>{t.loading}</p>
        ) : (
          <>
            <h2>{t.title(preview.workspaceName)}</h2>
            <p>
              {[preview.invitedBy ? t.invitedBy(preview.invitedBy) : null, preview.email ? t.forEmail(preview.email) : null]
                .filter(Boolean)
                .join(" · ")}
            </p>
            <dl className="facts">
              <dt>{t.shared}</dt>
              <dd>{t.sharedValue}</dd>
              <dt>{t.notShared}</dt>
              <dd>{t.notSharedValue}</dd>
              <dt>{t.usage}</dt>
              <dd>{t.usageValue}</dd>
            </dl>
            <label className="check">
              <input type="checkbox" checked={consent} onChange={(event) => setConsent(event.target.checked)} />
              <span>{t.shareUsage}</span>
            </label>
            {error ? <p className="error">{error}</p> : null}
            <div className="actions">
              <button className="button" onClick={onClose} disabled={joining}>
                {s.cancel}
              </button>
              <button className="button primary" onClick={join} disabled={joining}>
                {joining ? t.connecting : t.connect}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
