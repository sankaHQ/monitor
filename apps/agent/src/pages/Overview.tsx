import { invoke } from "@tauri-apps/api/core";
import lens from "../assets/lens.svg";
import { type AppContext, workspaceName } from "../context";
import { formatTime, formatWhen } from "../i18n";
import type { SaasSettings, ScanCheck, ScanView, Severity } from "../types";
import { Icon, PageTitle, Row } from "../ui";

/// Banners every page can show at the top: an update, a just-installed
/// version's notes, and the workspace's word about this computer.
export function Banners({ ctx, onWorkspace }: { ctx: AppContext; onWorkspace?: boolean }) {
  const { s, lang, status } = ctx;
  return (
    <>
      {status.updateRequired ? (
        <div className="banner bad">
          <div className="text">{s.banners.updateRequired(status.minAgentVersion)}</div>
          <button className="button primary" onClick={ctx.restartToUpdate}>
            {s.banners.updateNow}
          </button>
        </div>
      ) : status.updateReady ? (
        <div className="banner">
          <div className="text">
            <b>{s.banners.updateReady(status.updateReady.version)}</b> {s.banners.updateReadyHint}
          </div>
          <button className="button" onClick={ctx.restartToUpdate}>
            {s.banners.restartNow}
          </button>
        </div>
      ) : null}
      {status.justUpdated ? (
        <div className="banner ok">
          <div className="text">
            <b>{s.banners.justUpdated(status.justUpdated.version)}</b>
            {status.justUpdated.notes ? <span className="notes">{status.justUpdated.notes}</span> : null}
          </div>
          <button
            className="link"
            onClick={() => invoke("dismiss_update_notes").then(ctx.refresh).catch(() => {})}
          >
            {s.dismiss}
          </button>
        </div>
      ) : null}
      {status.removedByWorkspaceAt && !status.enrolled && !onWorkspace ? (
        <div className="banner warn">
          <div className="text">{s.banners.removed(formatTime(status.removedByWorkspaceAt, lang))}</div>
          <button className="button" onClick={() => ctx.go("workspace")}>
            {s.banners.details}
          </button>
        </div>
      ) : null}
      {status.paused ? <div className="banner warn">{s.banners.paused}</div> : null}
      {status.pendingApproval ? (
        <div className="banner">{s.banners.pending(workspaceName(ctx))}</div>
      ) : null}
    </>
  );
}

const SEVERITY_ORDER: Record<Severity, number> = { high: 0, medium: 1, low: 2 };

function sortedFindings(scan: ScanView) {
  return [...scan.findings].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);
}

export function Overview({ ctx }: { ctx: AppContext }) {
  const { s, lang, status, scan, scanning } = ctx;
  const checks = scan?.checks ?? [];
  const passed = checks.filter((check) => check.state === "pass").length;
  const findings = scan ? sortedFindings(scan) : [];
  const attention = new Set(checks.filter((check) => check.state === "attention").map((check) => check.id));
  const nudges = status.lastCheckin?.nudges ?? [];
  const workspace = status.enrolled ? (
    <>
      {workspaceName(ctx)}
      {status.managed ? <span className="tag">{s.overview.managed}</span> : null}
    </>
  ) : status.removedByWorkspaceAt ? (
    s.overview.removed
  ) : (
    <>
      {s.overview.notConnected}
      <button className="link accent" onClick={() => ctx.go("workspace")}>
        {s.overview.connect}
      </button>
    </>
  );

  return (
    <>
      <Banners ctx={ctx} />
      <div className="summary">
        <img src={lens} alt="" />
        <div className="text">
          <b>
            {scanning
              ? s.overview.scanning
              : scan
                ? s.overview.verdict(findings.length)
                : s.overview.notScanned}
          </b>
          <span>
            {scanning
              ? s.overview.scanningHint
              : scan
                ? s.overview.summary(passed, checks.length, scan.counts.programsPastSupport, scan.counts.programsAssessed, scan.counts.programsNotAssessed)
                : null}
          </span>
        </div>
        <button className={scanning ? "button spin" : "button"} onClick={ctx.runScan} disabled={scanning}>
          <Icon name="refresh" />
          {scanning ? s.overview.scanningButton : s.overview.scanAgain}
        </button>
      </div>

      <dl className="facts">
        <dt>{s.overview.lastScan}</dt>
        <dd>{scan ? formatWhen(scan.scannedAt, lang) : s.never}</dd>
        {scan ? (
          <>
            <dt>{s.overview.risks}</dt>
            <dd>{s.overview.riskCounts(scan.counts.high, scan.counts.medium, scan.counts.low)}</dd>
            <dt>{s.overview.programs}</dt>
            <dd>{s.overview.programCounts(scan.counts.programs, scan.counts.programsPastSupport, scan.counts.programsAssessed, scan.counts.programsNotAssessed)}</dd>
          </>
        ) : null}
        <hr />
        <dt>{s.overview.device}</dt>
        <dd>{scan?.device.hostname ?? status.device.hostname}</dd>
        <dt>{s.overview.os}</dt>
        <dd>
          {scan?.osName ?? status.device.osVersion}
          <span className="tag">
            {scan?.osSupport
              ? `${scan.osSupport.label} · ${scan.osSupport.supportEndsOn}`
              : s.overview.osSupport}
          </span>
        </dd>
        <dt>{s.overview.workspace}</dt>
        <dd>{workspace}</dd>
        <dt>{s.overview.version}</dt>
        <dd>{status.agentVersion}</dd>
      </dl>

      <RegularCheckNudges ctx={ctx} />

      {scan ? (
        findings.length > 0 ? (
          <>
            <div className="label">{s.overview.needsAttention}</div>
            <div className="list">
              {findings.map((finding, index) => (
                <button
                  key={index}
                  className="row"
                  onClick={() => ctx.go(finding.check ? `check:${finding.check}` : "risks")}
                >
                  <span
                    className={`status ${finding.check && attention.has(finding.check) ? "attention" : "fail"}`}
                  />
                  <span className="text">
                    <b>{finding.title}</b>
                    <span>{finding.fix}</span>
                  </span>
                  <span className={`severity ${finding.severity}`}>{finding.severityLabel}</span>
                  <span className="chevron">›</span>
                </button>
              ))}
            </div>
          </>
        ) : (
          <p className="hint">{s.overview.noRisks}</p>
        )
      ) : null}

      {status.enrolled && nudges.length > 0 ? (
        <>
          <div className="label">{s.overview.fromWorkspace}</div>
          <div className="list">
            {nudges.map((nudge) => (
              <Row key={nudge.id} title={nudge.title} detail={nudge.body}>
                {nudge.url ? (
                  <button
                    className="button"
                    onClick={() =>
                      invoke("open_nudge", { id: nudge.id }).catch((err) => ctx.setError(String(err)))
                    }
                  >
                    {s.open}
                  </button>
                ) : null}
              </Row>
            ))}
          </div>
        </>
      ) : null}
    </>
  );
}

/// What a regular check found that the scan before it did not have.
function RegularCheckNudges({ ctx }: { ctx: AppContext }) {
  const { s, lang, saas } = ctx;
  const nudges = saas?.nudges ?? [];
  if (nudges.length === 0) return null;
  const dismiss = async () => {
    try {
      ctx.setSaas(await invoke<SaasSettings>("dismiss_nudges", { lang }));
    } catch (err) {
      ctx.setError(String(err));
    }
  };
  return (
    <>
      <div className="label">
        <span>{s.banners.newSince}</span>
        <button className="link" onClick={dismiss}>
          {s.dismiss}
        </button>
      </div>
      <div className="list">
        {nudges.map((nudge) => (
          <Row
            key={nudge.id}
            title={lang === "ja" ? nudge.titleJa : nudge.titleEn}
            detail={lang === "ja" ? nudge.fixJa : nudge.fixEn}
          >
            <span className="aside">{formatWhen(nudge.at, lang)}</span>
          </Row>
        ))}
      </div>
    </>
  );
}

export function Risks({ ctx }: { ctx: AppContext }) {
  const { s, scan } = ctx;
  const findings = scan ? sortedFindings(scan) : [];
  return (
    <>
      <PageTitle icon="risks">{s.nav.risks}</PageTitle>
      {findings.length === 0 ? (
        <p className="hint">{scan ? s.risks.none : s.reports.noScan}</p>
      ) : (
        <div className="list">
          {findings.map((finding, index) => (
            <button
              key={index}
              className="row top"
              onClick={() => (finding.check ? ctx.go(`check:${finding.check}`) : ctx.go("software"))}
            >
              <span className={`severity ${finding.severity}`} style={{ width: 64, flex: "none" }}>
                {finding.severityLabel}
              </span>
              <span className="text">
                <b>{finding.title}</b>
                {finding.evidence ? <span>{finding.evidence}</span> : null}
                <span className="fix">{s.risks.fix(finding.fix)}</span>
              </span>
              <span className="chevron">›</span>
            </button>
          ))}
        </div>
      )}
    </>
  );
}

/// One security check, Oneleet-style: when it ran, its result with Re-run,
/// why it matters and, when it fails, how to fix it. Re-run scans again (a
/// few seconds), which re-runs every check.
export function CheckPage({ ctx, id }: { ctx: AppContext; id: string }) {
  const { s, lang, scan, scanning } = ctx;
  const check = scan?.checks.find((item) => item.id === id);
  if (!scan || !check) {
    return <p className="hint">{s.reports.noScan}</p>;
  }
  const kind = check.state;
  const label = stateLabel(s, check.state);
  return (
    <>
      <PageTitle icon={check.id}>{check.name}</PageTitle>
      <dl className="facts">
        <dt>{s.check.lastCheck}</dt>
        <dd>{scanning ? s.check.checking : formatTime(scan.scannedAt, lang)}</dd>
        <dt>{s.check.status}</dt>
        <dd>
          <span className={`pill ${kind}`}>{label}</span>
          <button className={scanning ? "link spin" : "link"} onClick={ctx.runScan} disabled={scanning}>
            <Icon name="refresh" />
            {scanning ? s.check.checking : s.check.rerun}
          </button>
        </dd>
        {check.detail ? (
          <>
            <dt>{s.check.found}</dt>
            <dd>{check.detail}</dd>
          </>
        ) : null}
      </dl>
      <div className="box">
        <h3>{s.check.why}</h3>
        <p>{check.why}</p>
      </div>
      <Acknowledgement ctx={ctx} check={check} />
      {check.state !== "pass" && check.pass === false && check.fix ? (
        <div className="box fix">
          <h3>{s.check.howToFix}</h3>
          <p>{check.fix}</p>
        </div>
      ) : null}
    </>
  );
}

export function stateLabel(s: AppContext["s"], state: ScanCheck["state"]): string {
  return state === "pass"
    ? s.check.passing
    : state === "attention"
      ? s.check.attention
      : state === "fail"
        ? s.check.failing
        : s.check.unknown;
}

/// Standard account and Device management: the person can accept the risk.
/// An administrator account passes for a week once acknowledged; device
/// management needs attention instead of failing (acks.rs has the rules).
function Acknowledgement({ ctx, check }: { ctx: AppContext; check: ScanCheck }) {
  const { s, lang } = ctx;
  const { ack } = check;
  const when = (iso: string | null) => formatTime(iso, lang);
  const admin = check.id === "local_admin";
  let text: string | null = null;
  if (check.pass !== false) return null;
  if (admin) {
    if (ack.validUntil) text = s.check.ack.adminAccepted(when(ack.acknowledgedAt), when(ack.validUntil));
    else if (ack.dueAt) text = s.check.ack.adminDue(when(ack.dueAt));
    else if (ack.canAcknowledge) text = s.check.ack.adminFailing;
  } else if (check.id === "device_management") {
    text = ack.acknowledgedAt ? s.check.ack.mdmAccepted(when(ack.acknowledgedAt)) : s.check.ack.mdmOptional;
  }
  if (!text) return null;
  return (
    <div className={`box ack ${check.state}`}>
      <h3>{s.check.ack.title}</h3>
      <p>{text}</p>
      {ack.canAcknowledge ? (
        <button className="button" onClick={() => ctx.acknowledgeCheck(check.id)}>
          {s.check.ack.button}
        </button>
      ) : null}
    </div>
  );
}
