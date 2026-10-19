import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatTime, type Strings } from "./i18n";
import type { Lang } from "./types";
import { formatBytes } from "./ui";

export type UploadPreview = {
  token: string;
  fileName: string;
  format: string;
  sizeBytes: number;
  sha256: string;
};

type UploadResult = {
  receipt: {
    id: string;
    receivedAt: string;
    fileName: string;
    sizeBytes: number;
    sha256: string;
  };
  matches: boolean;
};

/// "Upload this report to Sanka": shows exactly the file that will be sent
/// (name, size, SHA-256) and what else goes with it, then sends it on the
/// person's word and shows Sanka's receipt.
export function UploadPanel({
  s,
  lang,
  preview,
  onClose,
}: {
  s: Strings;
  lang: Lang;
  preview: UploadPreview;
  onClose: () => void;
}) {
  const [repEmail, setRepEmail] = useState("");
  const [senderCompany, setSenderCompany] = useState("");
  const [senderEmail, setSenderEmail] = useState("");
  const [note, setNote] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<UploadResult | null>(null);
  const panel = useRef<HTMLDivElement>(null);
  const t = s.upload;

  useEffect(() => {
    panel.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, [preview.token]);

  const send = async () => {
    setSending(true);
    setError(null);
    try {
      setResult(
        await invoke<UploadResult>("send_upload", {
          token: preview.token,
          fields: { repEmail, senderCompany, senderEmail, note },
        }),
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setSending(false);
    }
  };

  const cancel = () => {
    invoke("cancel_upload").catch(() => {});
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
      <dl className="facts single">
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
      <p className="hint">{t.explain}</p>
      <label className="field">
        <span>{t.repEmail}</span>
        <input type="text" value={repEmail} onChange={(e) => setRepEmail(e.target.value)} />
      </label>
      <label className="field">
        <span>{t.company}</span>
        <input type="text" value={senderCompany} onChange={(e) => setSenderCompany(e.target.value)} />
      </label>
      <label className="field">
        <span>{t.senderEmail}</span>
        <input type="text" value={senderEmail} onChange={(e) => setSenderEmail(e.target.value)} />
      </label>
      <label className="field">
        <span>{t.note}</span>
        <textarea value={note} maxLength={1000} onChange={(e) => setNote(e.target.value)} />
      </label>
      {error ? <p className="error">{error}</p> : null}
      <div className="row">
        <button onClick={send} disabled={sending}>
          {sending ? t.sending : error ? t.tryAgain : t.send}
        </button>
        <button className="link" onClick={cancel} disabled={sending}>
          {t.cancel}
        </button>
      </div>
    </div>
  );
}
