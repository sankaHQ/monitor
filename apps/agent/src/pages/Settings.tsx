import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppContext } from "../context";
import { formatWhen, LANGUAGE_NAMES } from "../i18n";
import type { CheckSchedule, Lang, LangChoice, SaasSettings, Theme, UpdateInfo } from "../types";
import { PageTitle, Row, Segmented, Switch } from "../ui";

type Schedule = "off" | CheckSchedule;

/// Preferences: appearance, regular checks, opening at login (connected),
/// the default report language, and the version with its update.
/** The last Check for Updates… request Settings acted on. */
let handledUpdateCheck = 0;

export function SettingsPage({ ctx }: { ctx: AppContext }) {
  const { s, lang, status, saas, busy, theme, reportLang } = ctx;
  const t = s.settings;
  const [saving, setSaving] = useState(false);
  const [checking, setChecking] = useState(false);

  const setSchedule = async (schedule: Schedule) => {
    setSaving(true);
    ctx.setError(null);
    try {
      ctx.setSaas(
        await invoke<SaasSettings>("set_check_schedule", {
          schedule: schedule === "off" ? null : schedule,
          lang,
        }),
      );
    } catch (err) {
      ctx.setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const checkNow = async () => {
    setChecking(true);
    ctx.setError(null);
    try {
      await invoke<UpdateInfo | null>("check_for_update");
    } catch (err) {
      ctx.setError(String(err));
    } finally {
      setChecking(false);
      ctx.refresh();
    }
  };

  // Check for Updates… in the app menu opened Settings: check now. The
  // request usually arrives before this page mounts, so the last one handled
  // is kept outside the component.
  useEffect(() => {
    if (ctx.updateCheckRequest === handledUpdateCheck) return;
    handledUpdateCheck = ctx.updateCheckRequest;
    checkNow();
  });

  const ready = status.updateReady;
  return (
    <>
      <PageTitle icon="settings">{s.nav.settings}</PageTitle>
      <div className="list">
        <Row title={t.appearance}>
          <Segmented<Theme>
            value={theme}
            label={t.appearance}
            options={(["system", "light", "dark"] as const).map((value) => ({ value, label: t.themes[value] }))}
            onChange={ctx.setTheme}
          />
        </Row>
        <Row
          title={t.regular}
          detail={
            saas?.lastScheduledCheckAt ? t.lastRegular(formatWhen(saas.lastScheduledCheckAt, lang)) : t.regularHint
          }
        >
          <Segmented<Schedule>
            value={saas?.schedule ?? "off"}
            label={t.regular}
            disabled={saas === null || saving}
            options={[
              { value: "off", label: t.off },
              { value: "daily", label: t.daily },
              { value: "weekly", label: t.weekly },
            ]}
            onChange={setSchedule}
          />
        </Row>
        {status.enrolled ? (
          <Row title={t.launchAtLogin} detail={t.launchHint}>
            <Switch
              checked={status.launchAtLogin}
              label={t.launchAtLogin}
              disabled={busy}
              onChange={(enabled) => ctx.run("set_launch_at_login", { enabled })}
            />
          </Row>
        ) : null}
        <Row title={t.language}>
          <Segmented<LangChoice>
            value={ctx.langChoice}
            label={t.language}
            options={[
              { value: "system", label: t.languageSystem },
              ...(Object.keys(LANGUAGE_NAMES) as Lang[]).map((code) => ({
                value: code,
                label: LANGUAGE_NAMES[code],
              })),
            ]}
            onChange={ctx.setLangChoice}
          />
        </Row>
        <Row title={t.reportLanguage}>
          <Segmented<Lang>
            value={reportLang}
            label={t.reportLanguage}
            options={(Object.keys(LANGUAGE_NAMES) as Lang[]).map((code) => ({
              value: code,
              label: LANGUAGE_NAMES[code],
            }))}
            onChange={ctx.setReportLang}
          />
        </Row>
      </div>
      <div className="list">
        <Row
          title={t.version(status.agentVersion)}
          detail={ready ? t.downloaded(ready.version) : t.upToDate(formatWhen(status.lastUpdateCheckAt, lang))}
        >
          {ready || status.updateRequired ? (
            <button className="button primary" onClick={ctx.restartToUpdate}>
              {t.restartToUpdate}
            </button>
          ) : (
            <button className="button" onClick={checkNow} disabled={checking}>
              {checking ? t.checking : t.checkNow}
            </button>
          )}
        </Row>
      </div>
    </>
  );
}
