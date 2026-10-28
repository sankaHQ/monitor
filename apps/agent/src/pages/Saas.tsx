import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { type AppContext, workspaceName } from "../context";
import type { SaasSettings } from "../types";
import { Icon, PageTitle, Row, Segmented, Switch } from "../ui";

type Filter = "all" | "notAllowed";

/// The SaaS this computer uses: the browser check (off unless turned on),
/// the company allowlist, sharing with a connected workspace, and the list.
export function SaasPage({ ctx }: { ctx: AppContext }) {
  const { s, lang, scan, scanning, saas, status } = ctx;
  const [filter, setFilter] = useState<Filter>("all");
  const [saving, setSaving] = useState(false);
  const allowlist = saas?.allowlist;

  const settingsCommand = async (command: string, args: Record<string, unknown>, after?: () => void) => {
    setSaving(true);
    ctx.setError(null);
    try {
      ctx.setSaas(await invoke<SaasSettings>(command, { ...args, lang }));
      after?.();
    } catch (err) {
      ctx.setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  // Turning the browser check on scans again; turning it off drops its
  // results from the scan shown. It can't change while a scan runs.
  const setBrowserCheck = (enabled: boolean) =>
    settingsCommand("set_browser_check", { enabled }, enabled ? ctx.runScan : ctx.refreshScan);

  const openMigration = (key: string) =>
    invoke("open_migration_page", { key, lang }).catch((err) => ctx.setError(String(err)));

  const safariBlocked = scan?.saasCheck.browsers.some(
    (browser) => browser.browser === "safari" && browser.status === "needs_full_disk_access",
  );
  const notAllowed = scan?.saas.filter((item) => item.allowed === false) ?? [];
  const judged = (scan?.saas ?? []).some((item) => item.allowed !== null);
  const rows = filter === "notAllowed" ? notAllowed : (scan?.saas ?? []);

  const allowlistDetail = allowlist?.error
    ? s.saas.allowlistError(allowlist.error)
    : allowlist?.origin === "machine"
      ? s.saas.machineInUse
      : allowlist?.origin === "workspace"
        ? s.saas.workspaceInUse
        : allowlist?.origin
          ? allowlist.summary
          : s.saas.allowlistNone;

  return (
    <>
      <PageTitle icon="saas">{s.nav.saas}</PageTitle>
      <div className="list">
        <Row
          title={s.saas.browserCheck}
          detail={
            scan && saas?.browserCheck && scan.saasCheck.browserCheck
              ? s.saas.thisScan(scan.saasCheck.browserSummary)
              : s.saas.browserHint
          }
        >
          <Switch
            checked={saas?.browserCheck ?? false}
            label={s.saas.browserCheck}
            disabled={saas === null || saving || scanning}
            onChange={setBrowserCheck}
          />
        </Row>
        <Row title={s.saas.allowlist} detail={allowlistDetail}>
          {allowlist && allowlist.origin !== "machine" && allowlist.origin !== "workspace" ? (
            <>
              {allowlist.chosen ? (
                <button
                  className="link"
                  disabled={saving}
                  onClick={() => settingsCommand("clear_allowlist", {}, ctx.refreshScan)}
                >
                  {s.saas.stopUsing}
                </button>
              ) : null}
              <button
                className="button"
                disabled={saving}
                onClick={() => settingsCommand("choose_allowlist", {}, ctx.refreshScan)}
              >
                {s.saas.chooseFile}
              </button>
            </>
          ) : null}
        </Row>
        {status.enrolled && saas?.browserCheck ? (
          <Row
            title={s.saas.share(workspaceName(ctx))}
            detail={saas.saasSharingHeld ? s.saas.shareHeld : s.saas.shareHint}
          >
            <Switch
              checked={saas.saasSharing}
              label={s.saas.share(workspaceName(ctx))}
              disabled={saving || scanning}
              onChange={(enabled) => settingsCommand("set_saas_sharing", { enabled })}
            />
          </Row>
        ) : null}
      </div>
      {safariBlocked ? <p className="hint">{s.saas.safariHint}</p> : null}
      <details className="more">
        <summary>{s.saas.browserCheck}</summary>
        <p>{s.saas.browserDetails}</p>
        {allowlist?.machinePath && allowlist.origin !== "machine" ? (
          <p>{s.saas.machinePath(allowlist.machinePath)}</p>
        ) : null}
      </details>

      {scan === null ? (
        <p className="hint">{s.saas.empty}</p>
      ) : (
        <>
          {judged ? (
            <div className="toolbar">
              <Segmented
                value={filter}
                label={s.nav.saas}
                options={[
                  { value: "all", label: s.saas.all(scan.saas.length) },
                  { value: "notAllowed", label: s.saas.notAllowed(notAllowed.length) },
                ]}
                onChange={setFilter}
              />
            </div>
          ) : null}
          <div className="list">
            {rows.length === 0 ? (
              <div className="empty">{filter === "notAllowed" ? s.saas.noneNotAllowed : s.saas.none}</div>
            ) : (
              rows.map((item) => (
                <div className="row" key={item.key}>
                  <div className="text">
                    <b>
                      {item.product}
                      {item.ai ? <span className="ai">{s.saas.ai}</span> : null}
                    </b>
                    <span>
                      {item.vendor} · {item.categoryLabel} · {item.sources}
                    </span>
                  </div>
                  <span
                    className={
                      item.allowed === false ? "aside bad" : item.allowed === true ? "aside ok" : "aside"
                    }
                  >
                    {item.allowedLabel}
                  </span>
                  {item.migratable ? (
                    <button
                      className="link accent"
                      title={`${s.saas.migration} · ${item.migrationLabel}`}
                      aria-label={`${s.saas.migration}: ${item.product}`}
                      onClick={() => openMigration(item.key)}
                    >
                      <Icon name="arrow" />
                    </button>
                  ) : null}
                </div>
              ))
            )}
          </div>
          <p className="hint">{s.saas.footer(saas?.catalogVerifiedOn ?? "—")}</p>
        </>
      )}
    </>
  );
}
