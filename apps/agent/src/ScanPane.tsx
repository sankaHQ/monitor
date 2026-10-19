import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatTime, type Strings } from "./i18n";
import type { Lang, ReportFormat, SavedReport, ScanView } from "./types";
import { formatBytes, Tick } from "./ui";
import { UploadPanel, type UploadPreview } from "./UploadPanel";

const FORMATS: ReportFormat[] = ["xlsx", "html", "csv"];

export function ScanPane({
  s,
  scan,
  scanning,
  lang,
  onScan,
  saved,
  onSaved,
  setError,
}: {
  s: Strings;
  scan: ScanView | null;
  scanning: boolean;
  lang: Lang;
  onScan: () => void;
  saved: SavedReport[];
  onSaved: (saved: SavedReport[]) => void;
  setError: (error: string | null) => void;
}) {
  const [saving, setSaving] = useState<ReportFormat | null>(null);
  const [upload, setUpload] = useState<UploadPreview | null>(null);
  const t = s.scan;

  // A saved report (`path`) or one the person picks (`null`).
  const prepareUpload = async (path: string | null) => {
    setError(null);
    try {
      const preview = await invoke<UploadPreview | null>("prepare_upload", { path });
      if (preview) setUpload(preview);
    } catch (err) {
      setError(String(err));
    }
  };

  const save = async (format: ReportFormat) => {
    setSaving(format);
    setError(null);
    try {
      const result = await invoke<SavedReport | null>("save_report", { format, lang });
      if (result) onSaved(await invoke<SavedReport[]>("get_saved_reports"));
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(null);
    }
  };

  const reveal = (path: string) =>
    invoke("reveal_report", { path }).catch((err) => setError(String(err)));
  const open = (path: string) =>
    invoke("open_report", { path }).catch((err) => setError(String(err)));

  return (
    <section>
      <div className="row">
        <button onClick={onScan} disabled={scanning}>
          {scanning ? t.scanning : scan ? t.scanAgain : t.scanNow}
        </button>
      </div>
      <p className="hint">{t.intro}</p>

      {scan === null ? (
        scanning ? <p className="hint">{t.reading}</p> : null
      ) : (
        <>
          <div className="tiles">
            <div className="tile high">
              <b>{scan.counts.high}</b>
              {t.high}
            </div>
            <div className="tile medium">
              <b>{scan.counts.medium}</b>
              {t.medium}
            </div>
            <div className="tile low">
              <b>{scan.counts.low}</b>
              {t.low}
            </div>
            <div className="tile">
              <b>
                {scan.counts.programsPastSupport}/{scan.counts.programs}
              </b>
              {t.pastSupport}
            </div>
          </div>
          <dl className="facts">
            <div>
              <dt>{t.computer}</dt>
              <dd>{scan.device.hostname}</dd>
            </div>
            <div>
              <dt>{t.scanned}</dt>
              <dd>{formatTime(scan.scannedAt, lang)}</dd>
            </div>
            <div>
              <dt>{t.os}</dt>
              <dd>{scan.osName}</dd>
            </div>
            <div>
              <dt>{t.osSupport}</dt>
              <dd>
                {scan.osSupport
                  ? `${scan.osSupport.label} (${scan.osSupport.supportEndsOn})`
                  : t.notAssessed}
              </dd>
            </div>
          </dl>

          <h2 className="gap">{t.risks}</h2>
          {scan.findings.length === 0 ? (
            <p className="hint">{t.noRisks}</p>
          ) : (
            <ul className="findings">
              {scan.findings.map((finding, index) => (
                <li key={index}>
                  <span className={`sev ${finding.severity}`}>{finding.severityLabel}</span>
                  <div>
                    <strong>{finding.title}</strong>
                    {finding.evidence ? <span className="detail">{finding.evidence}</span> : null}
                    <span className="fix">{finding.fix}</span>
                  </div>
                </li>
              ))}
            </ul>
          )}

          <h2 className="gap">{t.checks}</h2>
          <ul className="checks">
            {scan.checks.map((check) => (
              <li key={check.label}>
                <Tick pass={check.pass} />
                <span>{check.label}</span>
                {check.detail ? <span className="detail">{check.detail}</span> : null}
              </li>
            ))}
          </ul>

          <h2 className="gap">{t.saveTitle}</h2>
          <ul className="formats">
            {FORMATS.map((format) => (
              <li key={format}>
                <div>
                  <strong>{t.formats[format].label}</strong>
                  <span className="detail">{t.formats[format].hint}</span>
                </div>
                <button onClick={() => save(format)} disabled={saving !== null || scanning}>
                  {saving === format ? t.saving : t.save}
                </button>
              </li>
            ))}
          </ul>
          <p className="hint">{t.saveHint}</p>

          {saved.length > 0 ? (
            <>
              <h2 className="gap">{t.savedTitle}</h2>
              <ul className="saved">
                {saved.map((report) => (
                  <li key={report.path}>
                    <div>
                      <strong>{report.fileName}</strong>
                      <span className="detail">
                        {formatBytes(report.sizeBytes)} · {formatTime(report.savedAt, lang)}
                      </span>
                    </div>
                    <span className="links">
                      <button className="link" onClick={() => open(report.path)}>
                        {t.open}
                      </button>
                      <button className="link" onClick={() => reveal(report.path)}>
                        {t.showInFolder}
                      </button>
                      <button className="link" onClick={() => prepareUpload(report.path)}>
                        {t.uploadToSanka}
                      </button>
                    </span>
                  </li>
                ))}
              </ul>
            </>
          ) : null}

          <h2 className="gap">{t.uploadTitle}</h2>
          {upload ? (
            <UploadPanel s={s} lang={lang} preview={upload} onClose={() => setUpload(null)} />
          ) : (
            <>
              <p className="hint">{t.uploadIntro}</p>
              <button onClick={() => prepareUpload(null)}>{t.chooseFile}</button>
            </>
          )}
        </>
      )}
    </section>
  );
}

export function SoftwarePane({ s, scan }: { s: Strings; scan: ScanView | null }) {
  const [onlyFlagged, setOnlyFlagged] = useState(false);
  const t = s.software;
  if (scan === null) {
    return (
      <section>
        <p className="hint">{t.empty}</p>
      </section>
    );
  }
  const flagged = scan.software.filter((item) => item.flagged);
  const rows = onlyFlagged ? flagged : scan.software;
  return (
    <section>
      <label className="consent">
        <input
          type="checkbox"
          checked={onlyFlagged}
          onChange={(event) => setOnlyFlagged(event.target.checked)}
        />
        <span>{t.onlyFlagged(flagged.length)}</span>
      </label>
      {rows.length === 0 ? (
        <p className="hint">{t.nothing}</p>
      ) : (
        <ul className="software">
          {rows.map((item, index) => (
            <li key={`${item.name}-${item.version}-${index}`}>
              <div>
                <strong>{item.name}</strong>
                <span className="detail">
                  {[item.version, item.publisher].filter(Boolean).join(" · ")}
                </span>
                {item.flagged && item.successor ? (
                  <span className="fix">→ {item.successor}</span>
                ) : null}
              </div>
              <span className={item.flagged ? "sw-status flagged" : "sw-status"}>
                {item.statusLabel}
                {item.flagged && item.supportEndsOn ? ` · ${item.supportEndsOn}` : ""}
              </span>
            </li>
          ))}
        </ul>
      )}
      <p className="hint">{t.footer(scan.catalogVerifiedOn ?? "—")}</p>
    </section>
  );
}
