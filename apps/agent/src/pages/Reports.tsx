import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppContext } from "../context";
import { formatTime, formatWhen, LANGUAGE_NAMES } from "../i18n";
import type { Account, Lang, ReportFormat, SavedReport, SendPreview, SendResult } from "../types";
import { formatBytes, PageTitle, Row, Segmented } from "../ui";

const FORMATS: ReportFormat[] = ["xlsx", "html", "csv"];

/// Everything that takes the scan off the screen: Send to Sanka, saving a
/// report (Excel ledger, one-page HTML, CSV) in a chosen language, and the
/// reports saved this session.
export function Reports({ ctx }: { ctx: AppContext }) {
  const { s, lang, scan, scanning, account, saved, reportLang } = ctx;
  const [saving, setSaving] = useState<ReportFormat | null>(null);

  const save = async (format: ReportFormat) => {
    setSaving(format);
    ctx.setError(null);
    try {
      const result = await invoke<SavedReport | null>("save_report", { format, lang: reportLang });
      if (result) ctx.setSaved(await invoke<SavedReport[]>("get_saved_reports"));
    } catch (err) {
      ctx.setError(String(err));
    } finally {
      setSaving(null);
    }
  };
  const reveal = (path: string) => invoke("reveal_report", { path }).catch((err) => ctx.setError(String(err)));
  const open = (path: string) => invoke("open_report", { path }).catch((err) => ctx.setError(String(err)));
  const who = [account?.userName, account?.userEmail ? `<${account.userEmail}>` : null].filter(Boolean).join(" ");

  return (
    <>
      <PageTitle icon="reports">{s.nav.reports}</PageTitle>
      <div className="label">{s.reports.send}</div>
      <div className="list">
        <Row
          title={s.reports.sendThis}
          detail={account?.signedIn ? s.reports.signedInAs(who, account.workspaceName) : s.reports.sendHint}
        >
          <button className="button primary" onClick={ctx.openSend} disabled={!scan || scanning}>
            {s.reports.sendButton}
          </button>
        </Row>
      </div>

      <div className="label">
        <span>{s.reports.save}</span>
        <Segmented<Lang>
          value={reportLang}
          label={s.reports.language}
          options={(Object.keys(LANGUAGE_NAMES) as Lang[]).map((code) => ({
            value: code,
            label: LANGUAGE_NAMES[code],
          }))}
          onChange={ctx.setReportLang}
        />
      </div>
      <div className="list">
        {FORMATS.map((format) => (
          <Row key={format} title={s.reports.formats[format].label} detail={s.reports.formats[format].hint}>
            <button className="button" onClick={() => save(format)} disabled={!scan || saving !== null || scanning}>
              {saving === format ? s.reports.saving : s.reports.saveButton}
            </button>
          </Row>
        ))}
      </div>
      <p className="hint">{s.reports.emailInstead}</p>

      {saved.length > 0 ? (
        <>
          <div className="label">{s.reports.saved}</div>
          <div className="list">
            {saved.map((report) => (
              <Row
                key={report.path}
                title={report.fileName}
                detail={`${formatBytes(report.sizeBytes)} · ${formatWhen(report.savedAt, lang)}`}
              >
                <button className="link" onClick={() => open(report.path)}>
                  {s.open}
                </button>
                <button className="link" onClick={() => reveal(report.path)}>
                  {s.reports.showInFolder}
                </button>
              </Row>
            ))}
          </div>
        </>
      ) : null}
    </>
  );
}

/// "Send to Sanka", as a sheet: the account the report goes as, exactly what
/// will be sent (the device, the counts, the file name, size and SHA-256),
/// built from the latest scan or an Excel ledger the person edited, then
/// Sanka's receipt. Nothing is sent until they press Send; signing in happens
/// in the browser.
export function SendSheet({ ctx, onClose }: { ctx: AppContext; onClose: () => void }) {
  const { s, lang, account, scan, reportLang } = ctx;
  const [preview, setPreview] = useState<SendPreview | null>(null);
  // An edited workbook the person picked stays until they go back to the scan.
  const [edited, setEdited] = useState(false);
  const [note, setNote] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SendResult | null>(null);
  const t = s.send;
  const signedIn = account?.signedIn ?? false;

  const buildFromScan = useCallback(async () => {
    setError(null);
    try {
      setPreview(await invoke<SendPreview>("prepare_send", { lang: reportLang }));
    } catch (err) {
      setError(String(err));
    }
  }, [reportLang]);

  useEffect(() => {
    if (!edited) buildFromScan();
  }, [buildFromScan, scan?.reportId, edited]);

  const accountCommand = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError(null);
    try {
      ctx.setAccount(await invoke<Account>(command, args));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const pickEdited = async () => {
    setError(null);
    try {
      const picked = await invoke<SendPreview | null>("prepare_edited_excel");
      if (picked) {
        setPreview(picked);
        setEdited(true);
      }
    } catch (err) {
      setError(String(err));
    }
  };

  const send = async () => {
    if (!preview) return;
    setSending(true);
    setError(null);
    try {
      setResult(await invoke<SendResult>("send_report", { token: preview.token, note }));
      ctx.refresh();
    } catch (err) {
      setError(String(err));
      // A send can find the app signed out; show the sign-in again.
      invoke<Account>("get_account").then(ctx.setAccount).catch(() => {});
    } finally {
      setSending(false);
    }
  };

  const close = () => {
    if (!result) invoke("cancel_send").catch(() => {});
    onClose();
  };

  let body;
  if (result) {
    body = (
      <>
        <h2>{t.received}</h2>
        <p>
          {t.receivedDetail(
            result.receipt.fileName,
            formatBytes(result.receipt.sizeBytes),
            formatTime(result.receipt.receivedAt, lang),
          )}
        </p>
        <div className={result.matches ? "banner ok" : "banner bad"}>
          <div className="text">{result.matches ? t.matches : t.mismatch(result.receipt.sha256)}</div>
        </div>
        <p>{t.receipt(result.receipt.id)}</p>
        <div className="actions">
          <button className="button primary" onClick={onClose}>
            {t.done}
          </button>
        </div>
      </>
    );
  } else if (!signedIn) {
    body = (
      <>
        <h2>{t.title}</h2>
        <h3 className="label" style={{ margin: "12px 0 4px" }}>
          {t.signInTitle}
        </h3>
        <p>{t.signInIntro}</p>
        {account?.error ? <p className="error">{account.error}</p> : null}
        {account?.signingIn ? (
          <>
            <p>{t.waiting}</p>
            <div className="toolbar">
              <input
                className="input"
                value={code}
                placeholder={t.codePlaceholder}
                aria-label={t.codePlaceholder}
                onChange={(event) => setCode(event.target.value)}
              />
              <button
                className="button"
                onClick={() => accountCommand("finish_sign_in", { code })}
                disabled={busy || !code.trim()}
              >
                {busy ? t.signingIn : t.useCode}
              </button>
            </div>
          </>
        ) : null}
        {error ? <p className="error">{error}</p> : null}
        <div className="actions">
          <button
            className="button"
            onClick={() => (account?.signingIn ? accountCommand("cancel_sign_in") : close())}
            disabled={busy}
          >
            {s.cancel}
          </button>
          {account?.signingIn ? null : (
            <button className="button primary" onClick={() => accountCommand("begin_sign_in", { lang })} disabled={busy}>
              {t.signIn}
            </button>
          )}
        </div>
        <p className="hint" style={{ marginTop: 12 }}>
          {s.reports.emailInstead}
        </p>
      </>
    );
  } else {
    const who = [account?.userName, account?.userEmail ? `<${account.userEmail}>` : null].filter(Boolean).join(" ");
    body = (
      <>
        <h2>{t.title}</h2>
        <p>
          {s.reports.signedInAs(who, account?.workspaceName ?? null)}{" "}
          <button className="link accent" onClick={() => accountCommand("sign_out")} disabled={busy || sending}>
            {t.signOut}
          </button>
        </p>
        {preview ? (
          <>
            <p>{t.whatIsSent}</p>
            <dl className="facts">
              {preview.scan ? (
                <>
                  <dt>{t.device}</dt>
                  <dd>
                    {[preview.scan.hostname, preview.scan.deviceModel, preview.scan.osName]
                      .filter(Boolean)
                      .join(" · ")}
                  </dd>
                  <dt>{t.risks}</dt>
                  <dd>
                    {t.riskCounts(preview.scan.summary.high, preview.scan.summary.medium, preview.scan.summary.low)}
                  </dd>
                  <dt>{t.programs}</dt>
                  <dd>{t.programCounts(preview.scan.summary.programsPastSupport, preview.scan.programsAssessed, preview.scan.programsNotAssessed)}</dd>
                  <dt>{t.saas}</dt>
                  <dd>{t.saasCounts(preview.scan.saas, preview.scan.saasNotAllowed, preview.scan.saasBrowserCheck)}</dd>
                  <dt>{t.scanned}</dt>
                  <dd>{formatTime(preview.scan.scannedAt, lang)}</dd>
                  <dt>{t.language}</dt>
                  <dd>{LANGUAGE_NAMES[reportLang]}</dd>
                </>
              ) : null}
              <dt>{t.file}</dt>
              <dd>{preview.fileName}</dd>
              <dt>{t.size}</dt>
              <dd>{formatBytes(preview.sizeBytes)}</dd>
              <dt>{t.fingerprint}</dt>
              <dd className="mono">{preview.sha256}</dd>
            </dl>
            {preview.scan ? null : <p>{t.edited}</p>}
            <p>{t.explain}</p>
            <label className="field">
              <span>{t.note}</span>
              <textarea
                className="input"
                value={note}
                maxLength={1000}
                onChange={(event) => setNote(event.target.value)}
              />
            </label>
          </>
        ) : null}
        {error ? <p className="error">{error}</p> : null}
        <div className="actions split">
          {edited ? (
            <button className="link accent" onClick={() => setEdited(false)} disabled={sending}>
              {t.useScan}
            </button>
          ) : (
            <button className="link accent" onClick={pickEdited} disabled={sending}>
              {t.sendEdited}
            </button>
          )}
          <span className="actions">
            <button className="button" onClick={close} disabled={sending}>
              {s.cancel}
            </button>
            <button className="button primary" onClick={send} disabled={!preview || sending || busy}>
              {sending ? t.sending : error && preview ? t.tryAgain : t.send}
            </button>
          </span>
        </div>
      </>
    );
  }

  return (
    <div className="sheet" role="dialog" aria-modal="true" aria-label={t.title}>
      <div className="pane">{body}</div>
    </div>
  );
}
