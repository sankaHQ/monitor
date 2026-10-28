import { useState } from "react";
import lens from "./assets/lens.svg";
import type { AppContext, Screen } from "./context";
import { stateLabel } from "./pages/Overview";
import { Icon, StatusMark } from "./ui";

/// The sidebar: the Lens and the name, then Overview and Risks, one item per
/// security check (collapsible), Software and SaaS, and the app's own pages.
/// The footer offers a downloaded update and says the agent runs in the
/// background when the computer is connected.
export function Sidebar({ ctx, screen }: { ctx: AppContext; screen: Screen }) {
  const { s, scan, status } = ctx;
  const [checksOpen, setChecksOpen] = useState(true);
  const checks = scan?.checks ?? [];
  const passed = checks.filter((check) => check.state === "pass").length;
  const risks = scan?.findings.length ?? 0;
  const workspaceDot = status.enrolled
    ? status.pendingApproval || status.paused
      ? "warn"
      : "ok"
    : status.removedByWorkspaceAt
      ? "bad"
      : null;

  const item = (to: Screen, label: string, icon: string, aside?: React.ReactNode) => (
    <button
      key={to}
      className={screen === to ? "nav-item on" : "nav-item"}
      aria-current={screen === to ? "page" : undefined}
      onClick={() => ctx.go(to)}
    >
      <Icon name={icon} />
      <span className="label" title={label}>
        {label}
      </span>
      {aside}
    </button>
  );

  return (
    <aside className="side">
      <div className="brand">
        <img src={lens} alt="" />
        <span className="name">
          Sanka <span>{s.appWord}</span>
        </span>
      </div>
      <nav className="nav" aria-label={s.appName}>
        {item("overview", s.nav.overview, "overview")}
        {item(
          "risks",
          s.nav.risks,
          "risks",
          risks > 0 ? <span className="count warn">{risks}</span> : null,
        )}
        {checks.length > 0 ? (
          <>
            <button
              className={checksOpen ? "group" : "group closed"}
              aria-expanded={checksOpen}
              onClick={() => setChecksOpen(!checksOpen)}
            >
              <span>
                <span className="chevron">▾</span>
                {s.nav.checks}
              </span>
              <span>
                {passed}/{checks.length}
              </span>
            </button>
            {checksOpen
              ? checks.map((check) =>
                  item(
                    `check:${check.id}`,
                    check.name,
                    check.id,
                    <StatusMark state={check.state} label={stateLabel(s, check.state)} />,
                  ),
                )
              : null}
          </>
        ) : null}
        <div className="group">{s.nav.inventory}</div>
        {item(
          "software",
          s.nav.software,
          "software",
          scan ? <span className="count">{scan.counts.programs}</span> : null,
        )}
        {item("saas", s.nav.saas, "saas", scan ? <span className="count">{scan.saas.length}</span> : null)}
        <div className="group">{s.nav.monitor}</div>
        {item("reports", s.nav.reports, "reports")}
        {item(
          "workspace",
          status.enrolled && status.workspaceName ? status.workspaceName : s.nav.workspace,
          "workspace",
          workspaceDot ? <span className={`dot ${workspaceDot}`} /> : null,
        )}
        {item("privacy", s.nav.privacy, "privacy")}
        {item("settings", s.nav.settings, "settings")}
      </nav>
      <SidebarFoot ctx={ctx} />
    </aside>
  );
}

function SidebarFoot({ ctx }: { ctx: AppContext }) {
  const { s, status } = ctx;
  const required = status.updateRequired;
  const ready = status.updateReady;
  if (!required && !ready && !status.enrolled) return null;
  return (
    <div className="side-foot">
      {required ? (
        <div className="side-update required">
          <span>{s.foot.updateRequired}</span>
          <button onClick={ctx.restartToUpdate}>{s.foot.update}</button>
        </div>
      ) : ready ? (
        <div className="side-update">
          <span>{s.foot.updateReady(ready.version)}</span>
          <button onClick={ctx.restartToUpdate}>{s.foot.restart}</button>
        </div>
      ) : null}
      {status.enrolled ? <span>{s.foot.background}</span> : null}
    </div>
  );
}
