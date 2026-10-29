import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppContext } from "../context";
import type {
  ContextHistory,
  ContextPreview,
  ContextReceipt,
  ContextWorkflow,
} from "../types";
import { CONTEXT_STRINGS, contextLabel } from "../i18n";
import { Icon, PageTitle, Row, Segmented } from "../ui";
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
  if (!ctx.account?.accountScope) return <p>{ctx.s.loading}</p>;
  return <AccountHistory key={ctx.account.accountScope} ctx={ctx} />;
}
function AccountHistory({ ctx }: { ctx: AppContext }) {
  const t = CONTEXT_STRINGS[ctx.lang];
  const mounted = useRef(true);
  useLayoutEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const invokeCurrent = useCallback(
    async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
      if (!mounted.current) throw new Error("Account changed");
      const result = await invoke<T>(command, {
        ...args,
        accountScope: ctx.account?.accountScope,
      });
      if (!mounted.current) throw new Error("Account changed");
      return result;
    },
    [ctx.account?.accountScope],
  );
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
  const [section, setSection] = useState<
    "activity" | "workflows" | "sharing" | "settings"
  >("activity");
  const pageRef = useRef<HTMLDivElement>(null);
  const newWorkflowButton = useRef<HTMLButtonElement>(null);
  const wasEditing = useRef(false);
  useEffect(() => {
    if (!editor && wasEditing.current) newWorkflowButton.current?.focus();
    wasEditing.current = !!editor;
  }, [!!editor]);
  useEffect(() => {
    pageRef.current?.closest("main")?.scrollTo({ top: 0 });
  }, [section, !!preview, !!editor]);
  const previewHeading = useRef<HTMLHeadingElement>(null);
  const previewButton = useRef<HTMLButtonElement>(null);
  const wasPreviewing = useRef(false);
  useEffect(() => {
    if (preview) previewHeading.current?.focus();
    else if (wasPreviewing.current) previewButton.current?.focus();
    wasPreviewing.current = !!preview;
  }, [!!preview]);
  const initialized = useRef(false);
  const refresh = useCallback(async () => {
    if (!mounted.current) return;
    const data = await invokeCurrent<ContextHistory>("get_context_history");
    setHistory(data);
    if (!initialized.current) {
      setAllowed(data.settings.allowedAppIds);
      setOwnLogin(data.ownOsLogin === true);
      initialized.current = true;
    }
  }, [invokeCurrent]);
  useEffect(() => {
    refresh().catch((e) => {
      if (mounted.current) setMessage(String(e));
    });
    const timer = setInterval(() => {
      if (!document.hidden) refresh().catch(() => {});
    }, 5000);
    return () => clearInterval(timer);
  }, [refresh]);
  const invalidate = () => {
    if (!mounted.current) return;
    setPreview(null);
    invokeCurrent("cancel_context_preview").catch(() => {});
  };
  async function run(action: () => Promise<unknown>) {
    if (!mounted.current) return;
    setBusy(true);
    setMessage("");
    try {
      await action();
      await refresh();
    } catch (e) {
      if (mounted.current) setMessage(String(e));
    } finally {
      if (mounted.current) {
        await refresh().catch(() => {});
        if (mounted.current) setBusy(false);
      }
    }
  }
  const changeSelection = () => {
    if (!mounted.current) return;
    setPreview(null);
    void run(() =>
      invokeCurrent("invalidate_context_selection", {
        accountScope: ctx.account?.accountScope,
      }),
    );
  };
  const settings = (enabled: boolean, paused: boolean) =>
    run(async () => {
      invalidate();
      await invokeCurrent("set_context_settings", {
        settings: {
          enabled,
          paused,
          allowedAppIds: allowed,
          allowedDomains: allowed.includes("web:google-sheets")
            ? ["docs.google.com"]
            : [],
        },
        ownOsLogin: ownLogin,
      });
    });
  const appName = (id: string) => APPS.find(([key]) => key === id)?.[1] ?? id;
  const check = (
    label: string,
    value: boolean,
    change: () => void,
    detail?: string,
  ) => (
    <label className="row history-check">
      <input
        type="checkbox"
        checked={value}
        disabled={busy}
        onChange={change}
      />
      <span className="text">
        <b>{label}</b>
        {detail ? <span>{detail}</span> : null}
      </span>
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
        <textarea
          className="input"
          autoFocus={label === t.name}
          value={value}
          maxLength={maxLength}
          onChange={(e) => change(e.target.value)}
        />
      ) : (
        <input
          className="input"
          autoFocus={label === t.name}
          value={value}
          maxLength={maxLength}
          onChange={(e) => change(e.target.value)}
        />
      )}
    </label>
  );
  const frozen: {
    audience: string;
    apps?: { appId: string; name: string }[];
    episodes?: { id: string; day: string; repeatCount: number }[];
    workflows?: ContextWorkflow[];
    scan: unknown;
  } | null = preview ? JSON.parse(preview.packageJson) : null;
  return (
    <div className="history-page" ref={pageRef}>
      <PageTitle icon="reports">{t.title}</PageTitle>
      <p className="history-intro">{t.intro}</p>
      <div className="history-status">
        <span role="status">
          <Icon name="lock" /> {history?.active ? t.active : t.paused}
        </span>
        <button className="link accent" onClick={() => setSection("settings")}>
          {t.manageCapture}
        </button>
      </div>
      <div className="history-navigation">
        <Segmented
          value={section}
          label={t.sections}
          options={[
            { value: "activity", label: t.activity },
            { value: "workflows", label: t.workflowsTab },
            { value: "sharing", label: t.sharingTab },
            { value: "settings", label: t.settingsTab },
          ]}
          onChange={setSection}
        />
      </div>
      {message || history?.error ? (
        <p className="error" role="alert">
          {message || history?.error}
        </p>
      ) : null}
      <div aria-busy={busy}>
        {section === "activity" ? (
          <section aria-label={t.activity}>
            <div className="history-heading">
              <h2>{t.events}</h2>
              <button
                className="button"
                disabled={busy}
                onClick={() => run(refresh)}
              >
                <Icon name="refresh" />
                {t.refresh}
              </button>
            </div>
            <p className="history-description">{t.eventsHint}</p>
            {history?.gap ? (
              <p className="history-note">
                {contextLabel(ctx.lang, history.gap)}
              </p>
            ) : null}
            <div className="list">
              {!history?.events.length ? (
                <div className="history-empty">
                  <Icon name="reports" />
                  <h3>{t.noActivity}</h3>
                  <p>{t.noActivityHint}</p>
                  <button
                    className="button"
                    onClick={() => setSection("settings")}
                  >
                    {t.manageCapture}
                  </button>
                </div>
              ) : (
                history.events
                  .slice(-100)
                  .reverse()
                  .map((event) => (
                    <div key={event.id}>
                      {check(
                        event.gap
                          ? contextLabel(ctx.lang, event.gap)
                          : `${event.appName} — ${contextLabel(ctx.lang, event.action)}`,
                        selected.includes(event.id),
                        () => setSelected(toggle(selected, event.id)),
                        `${new Date(event.at).toLocaleString(ctx.lang)}${event.gap ? "" : ` · ${contextLabel(ctx.lang, event.provenance)}`}`,
                      )}
                    </div>
                  ))
              )}
            </div>
            {selected.length ? (
              <div className="history-actions">
                <span className="history-description">
                  {t.selectedCount(selected.length)}
                </span>
                <button
                  className="button danger"
                  disabled={busy}
                  onClick={() =>
                    run(async () => {
                      invalidate();
                      await invokeCurrent("delete_context_history", {
                        ids: selected,
                        workflowId: null,
                      });
                      setSelected([]);
                    })
                  }
                >
                  {t.deleteSelected}
                </button>
              </div>
            ) : null}
          </section>
        ) : null}
        {section === "workflows" ? (
          <section aria-label={t.workflowsTab}>
            {editor ? (
              <fieldset className="history-editor" disabled={busy}>
                <legend>{t.editWorkflow}</legend>
                <p className="history-description">{t.workflowHint}</p>
                {field(
                  t.name,
                  editor.name,
                  (name) => setEditor({ ...editor, name }),
                  200,
                )}
                {field(
                  t.purpose,
                  editor.purpose,
                  (purpose) => setEditor({ ...editor, purpose }),
                  1000,
                  true,
                )}
                <fieldset className="history-group">
                  <legend>{t.workflowApps}</legend>
                  <div className="list">
                    {APPS.map(([id, name]) => (
                      <div key={id}>
                        {check(name, editor.appIds.includes(id), () =>
                          setEditor({
                            ...editor,
                            appIds: toggle(editor.appIds, id),
                          }),
                        )}
                      </div>
                    ))}
                  </div>
                </fieldset>
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
                  (requiredFeatures) =>
                    setEditor({ ...editor, requiredFeatures }),
                  1000,
                  true,
                )}
                <div className="history-columns">
                  <label className="field">
                    {t.frequency}
                    <input
                      className="input"
                      type="number"
                      min="0"
                      max="10000"
                      value={editor.frequencyPerWeek ?? ""}
                      onChange={(e) =>
                        setEditor({
                          ...editor,
                          frequencyPerWeek:
                            e.target.value === ""
                              ? null
                              : Number(e.target.value),
                        })
                      }
                    />
                  </label>
                  <label className="field">
                    {t.minutes}
                    <input
                      className="input"
                      type="number"
                      min="0"
                      max="10080"
                      value={editor.minutesPerRun ?? ""}
                      onChange={(e) =>
                        setEditor({
                          ...editor,
                          minutesPerRun:
                            e.target.value === ""
                              ? null
                              : Number(e.target.value),
                        })
                      }
                    />
                  </label>
                </div>
                <label className="field">
                  {t.customCode}
                  <select
                    className="input"
                    value={
                      editor.ownsCustomCode === null
                        ? "unknown"
                        : String(editor.ownsCustomCode)
                    }
                    onChange={(e) =>
                      setEditor({
                        ...editor,
                        ownsCustomCode:
                          e.target.value === "unknown"
                            ? null
                            : e.target.value === "true",
                      })
                    }
                  >
                    <option value="unknown">{t.unknown}</option>
                    <option value="true">{t.yes}</option>
                    <option value="false">{t.no}</option>
                  </select>
                </label>
                <div className="list">
                  {check(t.confirmed, editor.confirmed, () =>
                    setEditor({ ...editor, confirmed: !editor.confirmed }),
                  )}
                </div>
                <div className="history-actions">
                  <button
                    className="button primary"
                    onClick={() =>
                      run(async () => {
                        invalidate();
                        await invokeCurrent("save_context_workflow", {
                          workflow: editor,
                          accountScope: ctx.account?.accountScope,
                        });
                        setEditor(null);
                      })
                    }
                  >
                    {t.save}
                  </button>
                  <button className="button" onClick={() => setEditor(null)}>
                    {ctx.s.cancel}
                  </button>
                </div>
              </fieldset>
            ) : (
              <>
                <div className="history-heading">
                  <h2>{t.workflows}</h2>
                  <button
                    className="button primary"
                    disabled={busy}
                    ref={newWorkflowButton}
                    onClick={() => setEditor(blank())}
                  >
                    {t.newWorkflow}
                  </button>
                </div>
                <p className="history-description">{t.workflowHint}</p>
                <div className="list">
                  {!history?.workflows.length ? (
                    <div className="history-empty">
                      <p>{t.noWorkflows}</p>
                    </div>
                  ) : (
                    history.workflows.map((workflow) => (
                      <Row
                        key={workflow.id}
                        title={workflow.name}
                        detail={workflow.purpose}
                      >
                        <button
                          className="button"
                          disabled={busy}
                          aria-label={t.editNamed(workflow.name)}
                          onClick={() => setEditor(workflow)}
                        >
                          {t.edit}
                        </button>
                        <button
                          className="link history-danger"
                          disabled={busy}
                          aria-label={t.deleteNamed(workflow.name)}
                          onClick={() =>
                            run(async () => {
                              invalidate();
                              await invokeCurrent("delete_context_history", {
                                ids: null,
                                workflowId: workflow.id,
                              });
                              setWorkflows(
                                workflows.filter((id) => id !== workflow.id),
                              );
                            })
                          }
                        >
                          {t.delete}
                        </button>
                      </Row>
                    ))
                  )}
                </div>
                <h2 className="history-subheading">{t.candidates}</h2>
                {!history?.candidates.length ? (
                  <p className="history-description">{t.insufficient}</p>
                ) : null}
                {history?.candidates.map((candidate) => (
                  <div className="list" key={candidate.id}>
                    <Row
                      title={candidate.steps
                        .map(
                          (step) =>
                            `${appName(step.appId)}: ${contextLabel(ctx.lang, step.action)}`,
                        )
                        .join(" → ")}
                      detail={t.repetitions(
                        candidate.repeatCount,
                        candidate.observedDays,
                      )}
                      top
                    >
                      <button
                        className="button"
                        disabled={busy}
                        onClick={() =>
                          setEditor({
                            ...blank(),
                            appIds: [
                              ...new Set(candidate.steps.map((s) => s.appId)),
                            ],
                            steps: candidate.steps.map(
                              (s) =>
                                `${appName(s.appId)}: ${contextLabel(ctx.lang, s.action)}`,
                            ),
                            episodeIds: candidate.episodes.map((e) => e.id),
                          })
                        }
                      >
                        {t.useCandidate}
                      </button>
                    </Row>
                    {candidate.evidenceExpired ? (
                      <p className="history-note">{t.expired}</p>
                    ) : null}
                    <div className="history-actions history-inset">
                      <button
                        className="link history-danger"
                        disabled={busy}
                        onClick={() =>
                          run(async () => {
                            invalidate();
                            await invokeCurrent("delete_context_history", {
                              ids: candidate.eventIds,
                              workflowId: null,
                            });
                          })
                        }
                      >
                        {t.deleteSequence}
                      </button>
                    </div>
                  </div>
                ))}
              </>
            )}
          </section>
        ) : null}
        {section === "sharing" ? (
          <section aria-label={t.sharingTab}>
            <div className="history-heading">
              <h2 ref={previewHeading} tabIndex={-1}>
                {preview ? t.reviewTitle : t.sharing}
              </h2>
            </div>
            <p className="history-description">
              {frozen
                ? frozen.audience === "company"
                  ? t.disclosure
                  : t.personalHint
                : t.sharingHint}
            </p>
            {!ctx.account?.signedIn ? (
              <div className="banner">
                <div className="text">{t.localOnly}</div>
                <button className="button" onClick={() => ctx.go("reports")}>
                  {t.signIn}
                </button>
              </div>
            ) : null}
            {!preview ? (
              <>
                <fieldset className="history-group" disabled={busy}>
                  <legend>{t.selectApps}</legend>
                  <div className="list">
                    {APPS.map(([id, name]) => (
                      <div key={id}>
                        {check(name, apps.includes(id), () => {
                          setApps(toggle(apps, id));
                          changeSelection();
                        })}
                      </div>
                    ))}
                  </div>
                </fieldset>
                {!!history?.workflows.length ? (
                  <fieldset className="history-group" disabled={busy}>
                    <legend>{t.workflows}</legend>
                    <div className="list">
                      {history.workflows.map((workflow) => (
                        <div key={workflow.id}>
                          {check(
                            workflow.name,
                            workflows.includes(workflow.id),
                            () => {
                              setWorkflows(toggle(workflows, workflow.id));
                              changeSelection();
                            },
                            workflow.purpose,
                          )}
                        </div>
                      ))}
                    </div>
                  </fieldset>
                ) : null}
                {!!history?.candidates.some(
                  (candidate) => candidate.episodes.length,
                ) ? (
                  <details className="more">
                    <summary>{t.selectedEpisodes}</summary>
                    <div className="list">
                      {history.candidates
                        .flatMap((candidate) => candidate.episodes)
                        .map((episode) => (
                          <div key={episode.id}>
                            {check(
                              `${episode.day} — ${t.repetitions(episode.repeatCount, 1)}`,
                              episodes.includes(episode.id),
                              () => {
                                setEpisodes(toggle(episodes, episode.id));
                                changeSelection();
                              },
                            )}
                          </div>
                        ))}
                    </div>
                  </details>
                ) : null}
                <div className="list">
                  {check(t.includeScan, scan, () => {
                    setScan(!scan);
                    changeSelection();
                  })}
                </div>
                <label className="field">
                  {t.audience}
                  <select
                    className="input"
                    aria-label={t.audience}
                    value={audience}
                    onChange={(e) => {
                      setAudience(e.target.value);
                      changeSelection();
                    }}
                  >
                    <option value="personal">{t.personal}</option>
                    <option value="company">{t.company}</option>
                  </select>
                </label>
                <p className="history-description">
                  {audience === "company" ? t.disclosure : t.personalHint}
                </p>
                {!ownLogin ? (
                  <p className="history-description">{t.loginRequired}</p>
                ) : null}
                <button
                  className="button primary"
                  ref={previewButton}
                  disabled={busy || !ownLogin}
                  onClick={() =>
                    run(async () =>
                      setPreview(
                        await invokeCurrent<ContextPreview>(
                          "prepare_context_package",
                          {
                            ownOsLogin: ownLogin,
                            selection: {
                              appIds: apps,
                              episodeIds: episodes,
                              workflowIds: workflows,
                              audience,
                              includeScan: scan,
                            },
                          },
                        ),
                      ),
                    )
                  }
                >
                  {t.preview}
                </button>
              </>
            ) : frozen ? (
              <>
                <div className="list">
                  <Row title={t.audience}>
                    <span className="tag">
                      {frozen.audience === "company" ? t.company : t.personal}
                    </span>
                  </Row>
                  <Row title={t.selectApps}>
                    <span className="history-value">
                      {(frozen.apps ?? []).map((app) => app.name).join(", ") ||
                        t.none}
                    </span>
                  </Row>
                  <Row title={t.workflows}>
                    <span className="history-value">
                      {(frozen.workflows ?? [])
                        .map((workflow) => workflow.name)
                        .join(", ") || t.none}
                    </span>
                  </Row>
                  <Row title={t.selectedEpisodes}>
                    <span className="history-value">
                      {t.sequenceCount((frozen.episodes ?? []).length)}
                    </span>
                  </Row>
                  <Row title={t.scanLabel}>
                    <span className="history-value">
                      {frozen.scan ? t.included : t.notIncluded}
                    </span>
                  </Row>
                </div>
                {(frozen.workflows ?? []).map((workflow) => (
                  <details className="more" key={workflow.id}>
                    <summary>{workflow.name}</summary>
                    <p>{workflow.purpose}</p>
                    <ol>
                      {workflow.steps.map((step, index) => (
                        <li key={index}>{step}</li>
                      ))}
                    </ol>
                    {workflow.exceptions ? (
                      <p>
                        {t.exceptions}: {workflow.exceptions}
                      </p>
                    ) : null}
                    {workflow.requiredFeatures ? (
                      <p>
                        {t.required}: {workflow.requiredFeatures}
                      </p>
                    ) : null}
                  </details>
                ))}
                <details className="more history-technical">
                  <summary>{t.technicalDetails}</summary>
                  <p>{t.technicalHint}</p>
                  <p className="mono">SHA-256: {preview.sha256}</p>
                  <pre>{JSON.stringify(frozen, null, 2)}</pre>
                </details>
                <p className="history-description">{t.cloudRetention}</p>
                <details className="more history-recurring">
                  <summary>{t.recurringTitle}</summary>
                  <p>{t.recurringHint}</p>
                  <button
                    className="button"
                    disabled={
                      busy ||
                      !ownLogin ||
                      !ctx.account?.signedIn ||
                      audience !== "company" ||
                      !!episodes.length ||
                      scan ||
                      history?.recurring ||
                      history?.revocationPending
                    }
                    onClick={() =>
                      run(() =>
                        invokeCurrent("enable_context_summary", {
                          token: preview.token,
                          ownOsLogin: ownLogin,
                        }),
                      )
                    }
                  >
                    {t.recurring}
                  </button>
                </details>
              </>
            ) : null}
            {history?.recurring ? (
              <p className="history-note" role="status">
                {t.recurringOn}
              </p>
            ) : null}
            {history?.revocationPending ? (
              <p className="history-note" role="status">
                {t.revocationPending}
              </p>
            ) : null}
            {receipt ? (
              <div className="list history-receipt">
                <Row
                  title={t.received}
                  detail={new Date(receipt.receivedAt).toLocaleString(ctx.lang)}
                >
                  <button
                    className="button primary"
                    disabled={busy}
                    onClick={() =>
                      run(() =>
                        invokeCurrent("open_context_result", { receipt }),
                      )
                    }
                  >
                    {t.result}
                  </button>
                </Row>
                <div className="history-inset">
                  <button
                    className="link history-danger"
                    disabled={busy}
                    onClick={() =>
                      run(async () => {
                        await invokeCurrent("delete_shared_context", {
                          packageId: receipt.packageId,
                        });
                        setReceipt(null);
                        invalidate();
                      })
                    }
                  >
                    {t.removeRemote}
                  </button>
                </div>
              </div>
            ) : null}
            <details className="more history-recurring">
              <summary>{t.manageSharing}</summary>
              <p>{t.remoteHint}</p>
              <button
                className="button danger"
                disabled={busy}
                onClick={() =>
                  run(async () => {
                    invalidate();
                    await invokeCurrent("delete_shared_context", {
                      packageId: null,
                    });
                    setReceipt(null);
                  })
                }
              >
                {t.revoke}
              </button>
            </details>
            {preview ? (
              <div className="history-actions history-preview-actions">
                <button
                  className="button primary"
                  disabled={busy || !ownLogin || !ctx.account?.signedIn}
                  onClick={() =>
                    run(async () =>
                      setReceipt(
                        await invokeCurrent<ContextReceipt>(
                          "send_context_package",
                          { token: preview.token },
                        ),
                      ),
                    )
                  }
                >
                  {t.send}
                </button>
                <button className="button" disabled={busy} onClick={invalidate}>
                  {t.cancel}
                </button>
              </div>
            ) : null}
          </section>
        ) : null}
        {section === "settings" ? (
          <section aria-label={t.settingsTab}>
            <h2 className="history-subheading">{t.captureApps}</h2>
            <p className="history-description">{t.captureHint}</p>
            <fieldset
              className="history-group"
              aria-label={t.captureApps}
              disabled={busy}
            >
              <div className="list">
                {APPS.map(([id, name]) => (
                  <div key={id}>
                    {check(name, allowed.includes(id), () => {
                      const next = toggle(allowed, id);
                      setAllowed(next);
                      invalidate();
                      if (history?.active)
                        void run(() =>
                          invokeCurrent("set_context_settings", {
                            settings: {
                              enabled: true,
                              paused: true,
                              allowedAppIds: next,
                              allowedDomains: next.includes("web:google-sheets")
                                ? ["docs.google.com"]
                                : [],
                            },
                            ownOsLogin: ownLogin,
                          }),
                        );
                    })}
                  </div>
                ))}
              </div>
              <div className="list">
                {check(
                  t.ownLogin,
                  ownLogin,
                  () => {
                    setOwnLogin(!ownLogin);
                    if (ownLogin) void settings(false, true);
                  },
                  t.ownLoginHint,
                )}
              </div>
              <div className="history-actions">
                <button
                  className="button primary"
                  disabled={!ownLogin || !allowed.length || history?.active}
                  onClick={() => settings(true, false)}
                >
                  {t.enable}
                </button>
                <button
                  className="button"
                  disabled={!history?.active}
                  onClick={() => settings(true, true)}
                >
                  {t.pause}
                </button>
                <button
                  className="button"
                  onClick={() => settings(false, true)}
                >
                  {t.off}
                </button>
              </div>
            </fieldset>
            <details className="more history-recurring">
              <summary>{t.register}</summary>
              <p>{t.registration}</p>
              <div className="history-actions">
                {["chrome", "edge"].map((browser) => (
                  <button
                    className="button"
                    key={browser}
                    disabled={busy}
                    onClick={() =>
                      run(async () =>
                        setMessage(
                          await invokeCurrent<string>(
                            "register_context_browser_host",
                            { browser },
                          ),
                        ),
                      )
                    }
                  >
                    {browser === "chrome" ? "Chrome" : "Edge"}
                  </button>
                ))}
              </div>
            </details>
            <details className="more">
              <summary>{t.permissions}</summary>
              <p>{t.support}</p>
              <p>{t.permission}</p>
            </details>
            <details className="more">
              <summary>{t.privacyTitle}</summary>
              <p>{t.privacy}</p>
            </details>
            <details className="more">
              <summary>{t.eraseTitle}</summary>
              <p>{t.eraseHint}</p>
              <div className="list">
                {check(t.eraseConfirm, erase, () => setErase(!erase))}
              </div>
              <button
                className="button danger"
                disabled={busy || !erase}
                onClick={() =>
                  run(async () => {
                    invalidate();
                    await invokeCurrent("delete_context_history", {
                      ids: null,
                      workflowId: null,
                    });
                    setSelected([]);
                    setEpisodes([]);
                    setWorkflows([]);
                    setErase(false);
                  })
                }
              >
                {t.erase}
              </button>
            </details>
          </section>
        ) : null}
      </div>
      {busy ? (
        <p className="history-description" role="status">
          {t.working}
        </p>
      ) : null}
    </div>
  );
}
