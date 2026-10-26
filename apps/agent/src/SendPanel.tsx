import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatTime, LANGUAGE_NAMES, type Strings } from "./i18n";
import type { Account, Lang, SendPreview, SendResult } from "./types";
import { formatBytes } from "./ui";

/// "Send to Sanka": the account the report goes as, and exactly what will be
/// sent (the device, the counts, the file name, size and SHA-256), built from
/// the latest scan or an Excel ledger the person edited. Nothing is sent until
/// they press Send; signing in happens in the browser.
export function SendPanel({
  s,
  lang,
  reportLang,
  reportId,
  account,
  onAccount,
  onClose,
}: {
  s: Strings;
  lang: Lang;
  /// The language the Excel is built in, chosen next to Save.
  reportLang: Lang;
  /// The latest scan: a new scan or language rebuilds the report shown.
  reportId: string;
  account: Account | null;
  onAccount: (account: Account) => void;
  onClose: () => void;
}) {
  const [preview, setPreview] = useState<SendPreview | null>(null);
  // An edited workbook the person picked stays until they go back to the scan.
  const [edited, setEdited] = useState(false);
  const [note, setNote] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SendResult | null>(null);
  const panel = useRef<HTMLDivElement>(null);
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
  }, [buildFromScan, reportId, edited]);

  useEffect(() => {
    panel.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, []);

  const run = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError(null);
    try {
      onAccount(await invoke<Account>(command, args));
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
    } catch (err) {
      setError(String(err));
      // A send can find the app signed out; show the sign-in again.
      invoke<Account>("get_account").then(onAccount).catch(() => {});
    } finally {
      setSending(false);
    }
  };

  const close = () => {
    invoke("cancel_send").catch(() => {});
    onClose();
  };

  if (result) {
    return (
      <div className="notice upload" ref={panel}>
        <p>
          <strong>
            {t.received(
              result.receipt.fileName,
              formatBytes(result.receipt.sizeBytes),
              formatTime(result.receipt.receivedAt, lang),
            )}
          </strong>
        </p>
        <p className={result.matches ? "hint" : "error"}>
          {result.matches ? t.matches : t.mismatch(result.receipt.sha256)}
        </p>
        <p className="hint">{t.receipt(result.receipt.id)}</p>
        <button onClick={onClose}>{t.done}</button>
      </div>
    );
  }

  return (
    <div className="notice upload" ref={panel}>
      <h2>{t.title}</h2>
      <p className="hint">{t.intro}</p>

      {signedIn ? (
        <div className="row">
          <span>
            {t.signedInAs(
              [account?.userName, account?.userEmail ? `<${account.userEmail}>` : null]
                .filter(Boolean)
                .join(" "),
              account?.workspaceName ?? null,
            )}
          </span>
          <button className="link" onClick={() => run("sign_out")} disabled={busy || sending}>
            {t.signOut}
          </button>
        </div>
      ) : (
        <div className="sign-in">
          <strong>{t.signInTitle}</strong>
          <p className="hint">{t.signInIntro}</p>
          {account?.error ? <p className="error">{account.error}</p> : null}
          {account?.signingIn ? (
            <>
              <p className="hint">{t.waiting}</p>
              <div className="enroll-row">
                <input
                  type="text"
                  value={code}
                  placeholder={t.codePlaceholder}
                  onChange={(e) => setCode(e.target.value)}
                />
                <button
                  onClick={() => run("finish_sign_in", { code })}
                  disabled={busy || !code.trim()}
                >
                  {busy ? t.signingIn : t.useCode}
                </button>
              </div>
              <button className="link" onClick={() => run("cancel_sign_in")} disabled={busy}>
                {t.cancel}
              </button>
            </>
          ) : (
            <div className="row">
              <button onClick={() => run("begin_sign_in", { lang })} disabled={busy}>
                {t.signIn}
              </button>
            </div>
          )}
        </div>
      )}

      {preview ? (
        <div className="gap">
          <p>{t.whatIsSent}</p>
          <dl className="facts single">
            {preview.scan ? (
              <>
                <div>
                  <dt>{t.device}</dt>
                  <dd>
                    {[preview.scan.hostname, preview.scan.deviceModel, preview.scan.osName]
                      .filter(Boolean)
                      .join(" · ")}
                  </dd>
                </div>
                <div>
                  <dt>{t.risks}</dt>
                  <dd>
                    {t.riskCounts(
                      preview.scan.summary.high,
                      preview.scan.summary.medium,
                      preview.scan.summary.low,
                    )}
                  </dd>
                </div>
                <div>
                  <dt>{t.programs}</dt>
                  <dd>
                    {t.programCounts(
                      preview.scan.summary.programsPastSupport,
                      preview.scan.summary.programs,
                    )}
                  </dd>
                </div>
                <div>
                  <dt>{t.saas}</dt>
                  <dd>
                    {t.saasCounts(
                      preview.scan.saas,
                      preview.scan.saasNotAllowed,
                      preview.scan.saasBrowserCheck,
                    )}
                  </dd>
                </div>
                <div>
                  <dt>{t.scanned}</dt>
                  <dd>{formatTime(preview.scan.scannedAt, lang)}</dd>
                </div>
                <div>
                  <dt>{t.language}</dt>
                  <dd>{LANGUAGE_NAMES[reportLang]}</dd>
                </div>
              </>
            ) : null}
            <div>
              <dt>{t.file}</dt>
              <dd>{preview.fileName}</dd>
            </div>
            <div>
              <dt>{t.size}</dt>
              <dd>{formatBytes(preview.sizeBytes)}</dd>
            </div>
            <div>
              <dt>{t.fingerprint}</dt>
              <dd className="mono">{preview.sha256}</dd>
            </div>
          </dl>
          {preview.scan ? null : <p className="hint">{t.edited}</p>}
          <p className="hint">{t.explain}</p>
          <label className="field">
            <span>{t.note}</span>
            <textarea value={note} maxLength={1000} onChange={(e) => setNote(e.target.value)} />
          </label>
        </div>
      ) : null}

      {error ? <p className="error">{error}</p> : null}
      <div className="row">
        <button onClick={send} disabled={!signedIn || !preview || sending || busy}>
          {sending ? t.sending : error && preview ? t.tryAgain : t.send}
        </button>
        <button className="link" onClick={close} disabled={sending}>
          {t.cancel}
        </button>
      </div>
      <p className="hint">
        {edited ? (
          <button className="link" onClick={() => setEdited(false)} disabled={sending}>
            {t.useScan}
          </button>
        ) : (
          <button className="link" onClick={pickEdited} disabled={sending}>
            {t.sendEdited}
          </button>
        )}
      </p>
      <p className="hint">{t.emailInstead}</p>
    </div>
  );
}
