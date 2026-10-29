import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppContext } from "../context";
import type { ContextHistory, ContextPreview, ContextReceipt, ContextWorkflow } from "../types";
import { CONTEXT_STRINGS, contextLabel } from "../i18n";
import { PageTitle } from "../ui";
const APPS = [
  ["web:google-sheets", "Google Sheets (Chrome / Edge)"],
  ["com.apple.TextEdit", "TextEdit (macOS)"],
  ["notepad.exe", "Notepad (Windows)"],
] as const;
const toggle = (items: string[], value: string) =>
  items.includes(value) ? items.filter((v) => v !== value) : [...items, value];
const blank = (): ContextWorkflow => ({
  id: crypto.randomUUID(),
  name: "",
  purpose: "",
  appIds: [],
  steps: [],
  exceptions: "",
  requiredFeatures: "",
  frequencyPerWeek: null,
  minutesPerRun: null,
  confirmed: false,
  ownsCustomCode: null,
  episodeIds: [],
});
export function History({ ctx }: { ctx: AppContext }) {
  const t = CONTEXT_STRINGS[ctx.lang];
  const [history, setHistory] = useState<ContextHistory | null>(null);
  const [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const [ownLogin, setOwnLogin] = useState(false),
    [erase, setErase] = useState(false);
  const [allowed, setAllowed] = useState<string[]>([]),
    [selected, setSelected] = useState<string[]>([]);
  const [apps, setApps] = useState<string[]>([]),
    [episodes, setEpisodes] = useState<string[]>([]),
    [workflows, setWorkflows] = useState<string[]>([]);
  const [audience, setAudience] = useState("personal"),
    [scan, setScan] = useState(false);
  const [preview, setPreview] = useState<ContextPreview | null>(null),
    [receipt, setReceipt] = useState<ContextReceipt | null>(null);
  const [editor, setEditor] = useState<ContextWorkflow | null>(null);
  const initialized = useRef(false);
  const refresh = useCallback(async () => {
    const data = await invoke<ContextHistory>("get_context_history");
    setHistory(data);
    if (!initialized.current) {
      setAllowed(data.settings.allowedAppIds);
      setOwnLogin(data.ownOsLogin === true);
      initialized.current = true;
    }
  }, []);
  useEffect(() => {
    refresh().catch((e) => setMessage(String(e)));
    const timer = setInterval(() => {
      if (!document.hidden) refresh().catch(() => {});
    }, 5000);
    return () => clearInterval(timer);
  }, [refresh, ctx.account]);
  const invalidate = () => {
    setPreview(null);
    invoke("cancel_context_preview").catch(() => {});
  };
  useEffect(() => {
    initialized.current = false;
    setPreview(null);
    setReceipt(null);
    setAllowed([]);
    setApps([]);
    setEpisodes([]);
    setWorkflows([]);
    setOwnLogin(false);
  }, [ctx.account]);
  async function run(action: () => Promise<unknown>) {
    setBusy(true);
    setMessage("");
    try {
      await action();
      await refresh();
    } catch (e) {
      setMessage(String(e));
    } finally {
      await refresh().catch(() => {});
      setBusy(false);
    }
  }
  const settings = (enabled: boolean, paused: boolean) =>
    run(async () => {
      invalidate();
      await invoke("set_context_settings", {
        settings: {
          enabled,
          paused,
          allowedAppIds: allowed,
          allowedDomains: allowed.includes("web:google-sheets") ? ["docs.google.com"] : [],
        },
        ownOsLogin: ownLogin,
      });
    });
  const check = (label: string, value: boolean, change: () => void) => (
    <label className="row">
      <input type="checkbox" checked={value} disabled={busy} onChange={change} /> {label}
    </label>
  );
  const field = (
    label: string,
    value: string,
    change: (s: string) => void,
    maxLength: number,
    multi = false,
  ) => (
    <label className="field">
      {label}
      {multi ? (
        <textarea value={value} maxLength={maxLength} onChange={(e) => change(e.target.value)} />
      ) : (
        <input value={value} maxLength={maxLength} onChange={(e) => change(e.target.value)} />
      )}
    </label>
  );
  return (
    <>
      <PageTitle icon="reports">{t.title}</PageTitle>
      <p>{t.privacy}</p>
      <p>{t.support}</p>
      <details>
        <summary>{t.permissions}</summary>
        <p>{t.permission}</p>
      </details>
      <p role="status">
        {history?.active ? t.active : t.paused}
        {history?.gap ? ` · ${contextLabel(ctx.lang, history.gap)}` : ""}
      </p>
      {message || history?.error ? <p role="alert">{message || history?.error}</p> : null}
      <fieldset className="box" disabled={busy}>
        <legend>{t.captureApps}</legend>
        {APPS.map(([id, name]) => (
          <div key={id}>
            {check(name, allowed.includes(id), () => {
              const next = toggle(allowed, id);
              setAllowed(next);
              invalidate();
              if (history?.active)
                void run(() =>
                  invoke("set_context_settings", {
                    settings: {
                      enabled: true,
                      paused: true,
                      allowedAppIds: next,
                      allowedDomains: next.includes("web:google-sheets") ? ["docs.google.com"] : [],
                    },
                    ownOsLogin: ownLogin,
                  }),
                );
            })}
          </div>
        ))}
        {check(t.ownLogin, ownLogin, () => {
          setOwnLogin(!ownLogin);
          if (ownLogin) void settings(false, true);
        })}
        <button
          className="button"
          disabled={!ownLogin || !allowed.length}
          onClick={() => settings(true, false)}
        >
          {t.enable}
        </button>{" "}
        <button className="button" onClick={() => settings(true, true)}>
          {t.pause}
        </button>{" "}
        <button className="button" onClick={() => settings(false, true)}>
          {t.off}
        </button>
      </fieldset>
      <details>
        <summary>{t.register}</summary>
        <p>{t.registration}</p>
        {["chrome", "edge"].map((browser) => (
          <button
            className="button"
            key={browser}
            disabled={busy}
            onClick={() =>
              run(async () =>
                setMessage(await invoke<string>("register_context_browser_host", { browser })),
              )
            }
          >
            {browser === "chrome" ? "Chrome" : "Edge"}
          </button>
        ))}
      </details>
      <h2>{t.events}</h2>
      <button className="button" disabled={busy} onClick={() => run(refresh)}>
        {t.refresh}
      </button>
      <div className="list">
        {history?.events
          .slice(-100)
          .reverse()
          .map((event) => (
            <div key={event.id}>
              {check(
                `${new Date(event.at).toLocaleString(ctx.lang)} · ${event.gap ? contextLabel(ctx.lang, event.gap) : `${event.appName}: ${contextLabel(ctx.lang, event.action)} (${contextLabel(ctx.lang, event.provenance)})`}`,
                selected.includes(event.id),
                () => setSelected(toggle(selected, event.id)),
              )}
            </div>
          ))}
      </div>
      <button
        className="button"
        disabled={busy || !selected.length}
        onClick={() =>
          run(async () => {
            invalidate();
            await invoke("delete_context_history", { ids: selected, workflowId: null });
            setSelected([]);
          })
        }
      >
        {t.deleteSelected}
      </button>
      <h2>{t.candidates}</h2>
      {!history?.candidates.length ? <p>{t.insufficient}</p> : null}
      {history?.candidates.map((candidate) => (
        <div className="box" key={candidate.id}>
          <p>
            {candidate.steps
              .map(
                (step) =>
                  `${APPS.find(([id]) => id === step.appId)?.[1] ?? step.appId}: ${contextLabel(ctx.lang, step.action)}`,
              )
              .join(" → ")}{" "}
            · {candidate.repeatCount} / {candidate.observedDays}
          </p>
          {candidate.evidenceExpired ? <p>{t.expired}</p> : null}
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              setEditor({
                ...blank(),
                appIds: [...new Set(candidate.steps.map((s) => s.appId))],
                steps: candidate.steps.map(
                  (s) =>
                    `${APPS.find(([id]) => id === s.appId)?.[1] ?? s.appId}: ${contextLabel(ctx.lang, s.action)}`,
                ),
                episodeIds: candidate.episodes.map((e) => e.id),
              })
            }
          >
            {t.useCandidate}
          </button>
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              run(async () => {
                invalidate();
                await invoke("delete_context_history", {
                  ids: candidate.eventIds,
                  workflowId: null,
                });
              })
            }
          >
            {t.delete}
          </button>
          <fieldset className="box">
            <legend>{t.selectedEpisodes}</legend>
            {candidate.episodes.map((episode) => (
              <div key={episode.id}>
                {check(
                  `${episode.day} · ${episode.repeatCount}`,
                  episodes.includes(episode.id),
                  () => {
                    setEpisodes(toggle(episodes, episode.id));
                    invalidate();
                  },
                )}
              </div>
            ))}
          </fieldset>
        </div>
      ))}
      <h2>{t.workflows}</h2>
      <button className="button" disabled={busy} onClick={() => setEditor(blank())}>
        {t.newWorkflow}
      </button>
      {history?.workflows.map((workflow) => (
        <div className="box" key={workflow.id}>
          {check(workflow.name, workflows.includes(workflow.id), () => {
            setWorkflows(toggle(workflows, workflow.id));
            invalidate();
          })}
          <p>{workflow.purpose}</p>
          {!workflow.episodeIds.length ? <p>{t.expired}</p> : null}
          <button className="button" onClick={() => setEditor(workflow)}>
            {t.useCandidate}
          </button>
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              run(async () => {
                invalidate();
                await invoke("delete_context_history", { ids: null, workflowId: workflow.id });
                setWorkflows(workflows.filter((id) => id !== workflow.id));
              })
            }
          >
            {t.delete}
          </button>
        </div>
      ))}
      {editor ? (
        <fieldset className="box" disabled={busy}>
          <legend>{t.newWorkflow}</legend>
          {field(t.name, editor.name, (name) => setEditor({ ...editor, name }), 200)}
          {field(
            t.purpose,
            editor.purpose,
            (purpose) => setEditor({ ...editor, purpose }),
            1000,
            true,
          )}
          {APPS.map(([id, name]) => (
            <div key={id}>
              {check(name, editor.appIds.includes(id), () =>
                setEditor({ ...editor, appIds: toggle(editor.appIds, id) }),
              )}
            </div>
          ))}
          {field(
            t.steps,
            editor.steps.join("\n"),
            (steps) => setEditor({ ...editor, steps: steps.split("\n") }),
            15000,
            true,
          )}
          {field(
            t.exceptions,
            editor.exceptions,
            (exceptions) => setEditor({ ...editor, exceptions }),
            1000,
            true,
          )}
          {field(
            t.required,
            editor.requiredFeatures,
            (requiredFeatures) => setEditor({ ...editor, requiredFeatures }),
            1000,
            true,
          )}
          <label className="field">
            {t.frequency}
            <input
              type="number"
              min="0"
              max="10000"
              value={editor.frequencyPerWeek ?? ""}
              onChange={(e) =>
                setEditor({
                  ...editor,
                  frequencyPerWeek: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            />
          </label>
          <label className="field">
            {t.minutes}
            <input
              type="number"
              min="0"
              max="10080"
              value={editor.minutesPerRun ?? ""}
              onChange={(e) =>
                setEditor({
                  ...editor,
                  minutesPerRun: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            />
          </label>
          <label className="field">
            {t.customCode}
            <select
              aria-label={t.customCode}
              value={editor.ownsCustomCode === null ? "unknown" : String(editor.ownsCustomCode)}
              onChange={(e) =>
                setEditor({
                  ...editor,
                  ownsCustomCode: e.target.value === "unknown" ? null : e.target.value === "true",
                })
              }
            >
              <option value="unknown">{t.unknown}</option>
              <option value="true">{t.yes}</option>
              <option value="false">{t.no}</option>
            </select>
          </label>
          {check(t.confirmed, editor.confirmed, () =>
            setEditor({ ...editor, confirmed: !editor.confirmed }),
          )}
          <button
            className="button"
            onClick={() =>
              run(async () => {
                invalidate();
                await invoke("save_context_workflow", { workflow: editor });
                setEditor(null);
              })
            }
          >
            {t.save}
          </button>
          <button className="button" onClick={() => setEditor(null)}>
            {ctx.s.cancel}
          </button>
        </fieldset>
      ) : null}
      <h2>{t.sharing}</h2>
      <p>{t.disclosure}</p>
      <p>{t.localOnly}</p>
      <fieldset className="box" disabled={busy}>
        <legend>{t.selectApps}</legend>
        {APPS.map(([id, name]) => (
          <div key={id}>
            {check(name, apps.includes(id), () => {
              setApps(toggle(apps, id));
              invalidate();
            })}
          </div>
        ))}
        <label className="field">
          {t.audience}
          <select
            aria-label={t.audience}
            value={audience}
            onChange={(e) => {
              setAudience(e.target.value);
              invalidate();
            }}
          >
            <option value="personal">{t.personal}</option>
            <option value="company">{t.company}</option>
          </select>
        </label>
        {check(t.includeScan, scan, () => {
          setScan(!scan);
          invalidate();
        })}
        <button
          className="button"
          disabled={!ownLogin}
          onClick={() =>
            run(async () =>
              setPreview(
                await invoke<ContextPreview>("prepare_context_package", {
                  ownOsLogin: ownLogin,
                  selection: {
                    appIds: apps,
                    episodeIds: episodes,
                    workflowIds: workflows,
                    audience,
                    includeScan: scan,
                  },
                }),
              ),
            )
          }
        >
          {t.preview}
        </button>
      </fieldset>
      {preview ? (
        <div className="box">
          <p>SHA-256: {preview.sha256}</p>
          <pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>
            {JSON.stringify(JSON.parse(preview.packageJson), null, 2)}
          </pre>
          <button
            className="button"
            disabled={busy || !ownLogin}
            onClick={() =>
              run(async () =>
                setReceipt(
                  await invoke<ContextReceipt>("send_context_package", { token: preview.token }),
                ),
              )
            }
          >
            {t.send}
          </button>
          <button className="button" disabled={busy} onClick={invalidate}>
            {t.cancel}
          </button>
          <p>{t.recurringHint}</p>
          <button
            className="button"
            disabled={
              busy ||
              !ownLogin ||
              audience !== "company" ||
              !!episodes.length ||
              scan ||
              history?.recurring
            }
            onClick={() =>
              run(() =>
                invoke("enable_context_summary", { token: preview.token, ownOsLogin: ownLogin }),
              )
            }
          >
            {t.recurring}
          </button>
        </div>
      ) : null}
      {history?.recurring ? <p>{t.recurringOn}</p> : null}
      {receipt ? (
        <div className="box">
          <p>
            {t.received} · {receipt.receivedAt}
          </p>
          <button
            className="button"
            disabled={busy}
            onClick={() => run(() => invoke("open_context_result", { receipt }))}
          >
            {t.result}
          </button>
          <button
            className="button"
            disabled={busy}
            onClick={() =>
              run(async () => {
                await invoke("delete_shared_context", { packageId: receipt.packageId });
                setReceipt(null);
                invalidate();
              })
            }
          >
            {t.removeRemote}
          </button>
        </div>
      ) : null}
      <p>{t.remoteHint}</p>
      <button
        className="button"
        disabled={busy}
        onClick={() =>
          run(async () => {
            invalidate();
            await invoke("delete_shared_context", { packageId: null });
            setReceipt(null);
          })
        }
      >
        {t.revoke}
      </button>
      <div className="box">
        {check(t.eraseConfirm, erase, () => setErase(!erase))}
        <button
          className="button"
          disabled={busy || !erase}
          onClick={() =>
            run(async () => {
              invalidate();
              await invoke("delete_context_history", { ids: null, workflowId: null });
              setSelected([]);
              setEpisodes([]);
              setWorkflows([]);
              setErase(false);
            })
          }
        >
          {t.erase}
        </button>
      </div>
      {busy ? <p role="status">{t.working}</p> : null}
    </>
  );
}
