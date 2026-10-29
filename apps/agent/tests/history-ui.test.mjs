import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const { chromium } = await import(
  process.env.MONITOR_PLAYWRIGHT_MODULE || "playwright"
);
const testUrl = new URL(process.env.MONITOR_UI_URL || "http://127.0.0.1:1439");
if (
  testUrl.protocol !== "http:" ||
  !["127.0.0.1", "[::1]"].includes(testUrl.hostname) ||
  testUrl.username ||
  testUrl.password ||
  testUrl.pathname !== "/" ||
  testUrl.search ||
  testUrl.hash
) {
  throw new Error(
    "MONITOR_UI_URL must be an HTTP loopback IP origin, without credentials or a path",
  );
}
const base = testUrl.origin;
const fixture = JSON.parse(
  await readFile(
    new URL("../src-tauri/fixtures/context-package-v1.json", import.meta.url),
    "utf8",
  ),
);
async function scenario(run, lang = "en", theme = "light") {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({
      viewport: { width: 860, height: 580 },
    });
    page.setDefaultTimeout(5000);
    await page.route("**/*", (r) =>
      r.request().url().startsWith(`${base}/`) ? r.continue() : r.abort(),
    );
    await page.addInitScript(
      ({ fixture, lang, theme }) => {
        localStorage.setItem("sanka-monitor.app-lang", lang);
        localStorage.setItem("sanka-monitor.theme", theme);
        fixture.workflows[0] = {
          ...fixture.workflows[0],
          name: lang === "ja" ? "週次売上レポート" : "Weekly sales report",
          purpose:
            lang === "ja"
              ? "営業チームの実績をまとめ、週次会議で確認します。"
              : "Prepare the sales team's results for the weekly review.",
          frequencyPerWeek: 1,
          minutesPerRun: 30,
        };
        let current = "a",
          nextCallback = 1,
          deferHistory = false,
          failNew = false,
          deferPreview = false;
        let releaseHistory, releasePreview;
        const callbacks = new Map(),
          events = new Map(),
          calls = [];
        const account = () => ({
          signedIn: true,
          accountScope: current,
          userName: "Same display name",
          workspaceName: "Same workspace name",
        });
        const histories = Object.fromEntries(
          ["a", "b"].map((id) => [
            id,
            {
              ownOsLogin: true,
              settings: {
                enabled: false,
                paused: true,
                allowedAppIds: [],
                allowedDomains: [],
              },
              active: false,
              events: [],
              candidates: [],
              episodes: [],
              workflows: [
                {
                  ...fixture.workflows[0],
                  name: `Account ${id.toUpperCase()} workflow`,
                },
              ],
              recurring: false,
              revocationPending: false,
              error: null,
              gap: null,
            },
          ]),
        );
        window.contextTest = {
          calls,
          deferHistory: () => {
            deferHistory = true;
          },
          deferPreview: () => {
            deferPreview = true;
          },
          releaseHistory: () => releaseHistory?.(),
          releasePreview: () => releasePreview?.(),
          switchAccount: (fail = false) => {
            current = "b";
            failNew = fail;
            for (const handler of events.get("account-changed") || [])
              callbacks.get(handler)({ payload: account() });
          },
          seedDetected: () => {
            const history = histories[current];
            const now = new Date("2026-10-29T03:00:00Z");
            history.events = [];
            history.workflows = [];
            history.candidates = [
              ["export", "import"],
              ["create", "send"],
            ].map((actions, ci) => {
              const steps = actions.map((action, ordinal) => ({
                ordinal,
                appId: "web:google-sheets",
                action,
                objectCategory: "spreadsheet",
                provenance: "inferred",
              }));
              const episodes = [0, 1, 2].map((n) => ({
                id: `00000000-0000-4000-8000-0000000000${ci}${n}`,
                day: n === 0 ? "2026-10-28" : "2026-10-29",
                repeatCount: 1,
                extractionVersion: 1,
                steps,
                gaps: [],
              }));
              const events = episodes.flatMap((episode, n) =>
                steps.map((step, i) => ({
                  ...step,
                  id: `event-${ci}-${n}-${i}`,
                  at: new Date(
                    now.getTime() - n * 3600000 + i * 1000,
                  ).toISOString(),
                  appName: "Google Sheets",
                  gap: null,
                })),
              );
              history.events.push(...events);
              return {
                id: `00000000-0000-4000-8000-0000000001${ci}0`,
                steps,
                eventIds: events.map((e) => e.id),
                episodes,
                observedDays: 2,
                repeatCount: 3,
                evidenceExpired: false,
              };
            });
            history.episodes = history.candidates.flatMap((c) => c.episodes);
          },
          enableRecurring: () => {
            histories[current].recurring = true;
          },
        };
        window.__TAURI_INTERNALS__ = {
          metadata: {
            currentWindow: { label: "main" },
            currentWebview: { label: "main" },
          },
          transformCallback: (cb) => {
            const id = nextCallback++;
            callbacks.set(id, cb);
            return id;
          },
          invoke: async (cmd, args) => {
            calls.push({ cmd, args, account: current });
            if (cmd === "plugin:event|listen") {
              events.set(args.event, [
                ...(events.get(args.event) || []),
                args.handler,
              ]);
              return nextCallback++;
            }
            if (cmd === "get_status")
              return {
                firstRun: false,
                enrolled: false,
                agentVersion: "synthetic",
                posture: [],
                todayUsage: [],
                device: {
                  platform: "macos",
                  osVersion: "synthetic",
                  hostname: "synthetic",
                },
                lastCheckin: null,
                updateReady: null,
                justUpdated: null,
              };
            if (cmd === "get_account") return account();
            if (cmd === "get_transparency" || cmd === "get_saved_reports")
              return [];
            if (cmd === "get_saas_settings")
              return { lastScheduledCheckAt: null };
            if (cmd === "get_context_history") {
              const result = structuredClone(histories[current]);
              if (deferHistory) {
                deferHistory = false;
                await new Promise((resolve) => {
                  releaseHistory = resolve;
                });
              } else if (failNew && current === "b")
                throw new Error("Synthetic new account unavailable");
              return result;
            }
            if (cmd === "save_context_workflow") {
              const history = histories[current];
              history.workflows = [
                ...history.workflows.filter((w) => w.id !== args.workflow.id),
                structuredClone(args.workflow),
              ];
              history.candidates = history.candidates.filter(
                (c) => c.id !== args.workflow.id,
              );
              return null;
            }
            if (cmd === "dismiss_context_candidate") {
              const history = histories[current];
              const removed = history.candidates.find((c) => c.id === args.id);
              history.candidates = history.candidates.filter(
                (c) => c.id !== args.id,
              );
              history.episodes = history.episodes.filter(
                (e) => !removed?.episodes.some((x) => x.id === e.id),
              );
              return null;
            }
            if (cmd === "export_context_history") return true;
            if (cmd === "prepare_context_package") {
              if (deferPreview) {
                deferPreview = false;
                await new Promise((resolve) => {
                  releasePreview = resolve;
                });
              }
              return {
                token: "old-account-preview",
                sha256: "synthetic",
                packageJson: JSON.stringify(fixture),
              };
            }
            if (cmd === "invalidate_context_selection") {
              if (
                !histories[current].recurring &&
                !histories[current].revocationPending
              )
                return null;
              histories[current].recurring = false;
              histories[current].revocationPending = true;
              histories[current].error =
                "Daily sharing stopped locally; remote revocation pending";
              throw new Error(histories[current].error);
            }
            return null;
          },
        };
      },
      { fixture, lang, theme },
    );
    await page.goto(base);
    await page
      .getByRole("button", {
        name: lang === "ja" ? "作業履歴" : "Work History",
        exact: true,
      })
      .click();
    await page
      .getByRole("button", {
        name: lang === "ja" ? "繰り返す作業" : "Repeated tasks",
        exact: true,
      })
      .click();
    await page.getByText("Account A workflow", { exact: true }).waitFor();
    await run(page);
    await page.close();
  } finally {
    await browser.close();
  }
}

test("account switch discards an open editor and a delayed old refresh, including failed new loads", async () => {
  for (const failNew of [false, true])
    await scenario(async (page) => {
      await page
        .getByRole("button", { name: "Edit Account A workflow", exact: true })
        .click();
      assert.equal(
        await page
          .getByLabel("Task name", { exact: true })
          .evaluate((node) => node === document.activeElement),
        true,
      );
      assert.equal(
        await page.getByLabel("Task name", { exact: true }).inputValue(),
        "Account A workflow",
      );
      await page.getByRole("button", { name: "Activity", exact: true }).click();
      await page.evaluate(() => window.contextTest.deferHistory());
      await page.getByRole("button", { name: "Refresh", exact: true }).click();
      await page.waitForFunction(
        () =>
          window.contextTest.calls.filter(
            (c) => c.cmd === "get_context_history",
          ).length >= 2,
      );
      await page.evaluate(
        (fail) => window.contextTest.switchAccount(fail),
        failNew,
      );
      await page.waitForFunction(() =>
        window.contextTest.calls.some(
          (c) => c.cmd === "get_context_history" && c.account === "b",
        ),
      );
      await page.evaluate(() => window.contextTest.releaseHistory());
      await page.waitForFunction(
        () => !document.querySelector('[aria-busy="true"]'),
      );
      assert.equal(
        await page.getByLabel("Task name", { exact: true }).count(),
        0,
        "old editor must unmount",
      );
      assert.equal(
        await page.getByText("Account A workflow", { exact: true }).count(),
        0,
        "old refresh must not restore old data",
      );
      if (!failNew) {
        await page
          .getByRole("button", { name: "Repeated tasks", exact: true })
          .click();
        await page.getByText("Account B workflow", { exact: true }).waitFor();
      }
      assert.equal(
        (await page.evaluate(() => window.contextTest.calls)).filter(
          (c) => c.cmd === "save_context_workflow",
        ).length,
        0,
      );
    });
});

test("account switch discards a delayed old preview action result", async () =>
  scenario(async (page) => {
    await page.getByRole("button", { name: "Sharing", exact: true }).click();
    await page.evaluate(() => window.contextTest.deferPreview());
    await page
      .getByRole("button", { name: "Review before sharing", exact: true })
      .click();
    await page.waitForFunction(() =>
      window.contextTest.calls.some((c) => c.cmd === "prepare_context_package"),
    );
    await page.evaluate(() => window.contextTest.switchAccount());
    await page
      .getByRole("button", { name: "Repeated tasks", exact: true })
      .click();
    await page.getByText("Account B workflow", { exact: true }).waitFor();
    await page.evaluate(() => window.contextTest.releasePreview());
    assert.equal(
      await page
        .getByRole("button", { name: "Share with Sanka Monitor", exact: true })
        .count(),
      0,
    );
    assert.equal(
      (await page.evaluate(() => window.contextTest.calls)).filter(
        (c) => c.cmd === "send_context_package",
      ).length,
      0,
    );
  }));

test("disclosure selection and audience changes invalidate daily consent even offline", async () =>
  scenario(async (page) => {
    await page.getByRole("button", { name: "Activity", exact: true }).click();
    await page.evaluate(() => window.contextTest.enableRecurring());
    await page.getByRole("button", { name: "Refresh", exact: true }).click();
    await page.getByRole("button", { name: "Sharing", exact: true }).click();
    await page
      .getByText("Daily sharing is on for this session.", { exact: true })
      .waitFor();
    await page
      .getByRole("group", { name: "Apps to include", exact: true })
      .getByLabel("Google Sheets (Chrome / Edge)", { exact: true })
      .check();
    await page.waitForFunction(() =>
      window.contextTest.calls.some(
        (c) => c.cmd === "invalidate_context_selection",
      ),
    );
    await page
      .getByRole("status")
      .filter({ hasText: "Cloud removal is not confirmed" })
      .waitFor();
    assert.equal(
      await page
        .getByText("Daily sharing is on for this session.", { exact: true })
        .count(),
      0,
    );
    await page
      .getByLabel("Who can view this?", { exact: true })
      .selectOption("company");
    await page
      .getByLabel("Who can view this?", { exact: true })
      .selectOption("personal");
    assert.ok(
      (await page.evaluate(() => window.contextTest.calls)).filter(
        (c) => c.cmd === "invalidate_context_selection",
      ).length >= 3,
    );
  }));

test("history sections keep sharing review readable, keyboard accessible and within the window", async () => {
  for (const [lang, theme] of [
    ["en", "light"],
    ["ja", "light"],
    ["en", "dark"],
  ]) {
    await scenario(
      async (page) => {
        const ja = lang === "ja";
        const section = page.getByRole("group", {
          name: ja ? "作業履歴の表示" : "Work History sections",
          exact: true,
        });
        const sharing = section.getByRole("button", {
          name: ja ? "共有" : "Sharing",
          exact: true,
        });
        await sharing.focus();
        await page.keyboard.press("Enter");
        await page
          .getByRole("heading", {
            name: ja ? "共有する情報を選択" : "Choose what to share",
            exact: true,
          })
          .waitFor();
        assert.equal(await sharing.getAttribute("aria-pressed"), "true");
        assert.notEqual(
          await sharing.evaluate((node) => getComputedStyle(node).outlineStyle),
          "none",
        );
        const check = page
          .getByRole("group", {
            name: ja ? "含めるアプリ" : "Apps to include",
            exact: true,
          })
          .getByLabel("Google Sheets (Chrome / Edge)", { exact: true });
        await check.focus();
        await page.keyboard.press("Space");
        assert.equal(await check.isChecked(), true);
        await page
          .getByRole("button", {
            name: ja ? "共有内容を確認" : "Review before sharing",
            exact: true,
          })
          .click();
        await page
          .getByRole("heading", {
            name: ja ? "選択した内容を確認" : "Review your selection",
            exact: true,
          })
          .waitFor();
        assert.equal(
          await page
            .getByRole("heading", {
              name: ja ? "選択した内容を確認" : "Review your selection",
              exact: true,
            })
            .evaluate((node) => node === document.activeElement),
          true,
        );
        // The summary must reflect the frozen package, even if the draft audience differs.
        await page
          .getByText(ja ? "会社の管理者" : "Company admins", { exact: true })
          .waitFor();
        assert.equal(
          await page.locator(".history-technical pre").isVisible(),
          false,
        );
        await page
          .getByText(
            ja ? "共有するすべてのデータを表示" : "View full shared data",
            { exact: true },
          )
          .click();
        assert.ok(
          (await page.locator(".history-technical pre").textContent()).includes(
            fixture.packageId,
          ),
        );
        await page
          .getByText(
            ja ? "共有するすべてのデータを表示" : "View full shared data",
            { exact: true },
          )
          .click();
        const snapshot = await page.locator("main").ariaSnapshot();
        assert.ok(snapshot.includes("heading"));
        if (process.env.MONITOR_AXE_PATH) {
          await page.addScriptTag({ path: process.env.MONITOR_AXE_PATH });
          const results = await page.evaluate(async () =>
            window.axe.run(document.querySelector(".history-page"), {
              runOnly: {
                type: "tag",
                values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"],
              },
            }),
          );
          assert.deepEqual(
            results.violations.map((item) => ({
              id: item.id,
              nodes: item.nodes.map((node) => node.target),
            })),
            [],
          );
        }
        if (process.env.MONITOR_SCREENSHOT_DIR) {
          await page.locator("main").evaluate((node) => node.scrollTo(0, 0));
          await page.screenshot({
            path: `${process.env.MONITOR_SCREENSHOT_DIR}/monitor-history-sharing-${lang}-${theme}.png`,
          });
        }
        const overflowing = () =>
          page
            .locator("main")
            .evaluate((node) => node.scrollWidth > node.clientWidth);
        assert.equal(await overflowing(), false);
        await page.setViewportSize({ width: 680, height: 580 });
        assert.equal(await overflowing(), false);
        await page.setViewportSize({ width: 860, height: 580 });
        await page.locator("main").evaluate((node) => {
          node.style.zoom = "2";
        });
        assert.equal(
          await overflowing(),
          false,
          "200% content zoom must reflow without clipping",
        );
        await page.locator("main").evaluate((node) => {
          node.style.zoom = "1";
        });
        await page.emulateMedia({ reducedMotion: "reduce" });
        await page
          .getByRole("button", {
            name: ja ? "選択を変更" : "Change selection",
            exact: true,
          })
          .click();
        assert.equal(
          await page
            .getByRole("button", {
              name: ja ? "Sanka Monitorと共有" : "Share with Sanka Monitor",
              exact: true,
            })
            .count(),
          0,
        );
        assert.equal(
          await page
            .getByRole("button", {
              name: ja ? "共有内容を確認" : "Review before sharing",
              exact: true,
            })
            .evaluate((node) => node === document.activeElement),
          true,
        );
        for (const [value, label] of [
          ["selection", ja ? "共有" : "Sharing"],
          ["workflows", ja ? "繰り返す作業" : "Repeated tasks"],
          ["activity", ja ? "操作履歴" : "Activity"],
          ["settings", ja ? "設定" : "Settings"],
        ]) {
          await section
            .getByRole("button", { name: label, exact: true })
            .click();
          if (process.env.MONITOR_AXE_PATH) {
            const violations = await page.evaluate(
              async () =>
                (
                  await window.axe.run(
                    document.querySelector(".history-page"),
                    {
                      runOnly: {
                        type: "tag",
                        values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"],
                      },
                    },
                  )
                ).violations,
            );
            assert.deepEqual(
              violations.map((item) => ({
                id: item.id,
                nodes: item.nodes.map((node) => node.target),
              })),
              [],
            );
          }
          if (process.env.MONITOR_SCREENSHOT_DIR)
            await page.screenshot({
              path: `${process.env.MONITOR_SCREENSHOT_DIR}/monitor-history-${value}-${lang}-${theme}.png`,
            });
          if (value === "workflows") {
            await page
              .getByRole("button", {
                name: ja
                  ? "Account A workflowを編集"
                  : "Edit Account A workflow",
                exact: true,
              })
              .click();
            assert.equal(
              await page
                .getByLabel(ja ? "作業名" : "Task name", {
                  exact: true,
                })
                .evaluate((node) => node === document.activeElement),
              true,
            );
            if (process.env.MONITOR_AXE_PATH) {
              const violations = await page.evaluate(
                async () =>
                  (
                    await window.axe.run(
                      document.querySelector(".history-page"),
                      {
                        runOnly: {
                          type: "tag",
                          values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"],
                        },
                      },
                    )
                  ).violations,
              );
              assert.deepEqual(
                violations.map((item) => ({
                  id: item.id,
                  nodes: item.nodes.map((node) => node.target),
                })),
                [],
              );
            }
            if (process.env.MONITOR_SCREENSHOT_DIR)
              await page.screenshot({
                path: `${process.env.MONITOR_SCREENSHOT_DIR}/monitor-history-editor-${lang}-${theme}.png`,
              });
            await page
              .getByRole("button", {
                name: ja ? "キャンセル" : "Cancel",
                exact: true,
              })
              .click();
            assert.equal(
              await page
                .getByRole("heading", {
                  name: ja ? "確認する作業" : "Tasks to review",
                  exact: true,
                })
                .evaluate((node) => node === document.activeElement),
              true,
            );
          }
        }
      },
      lang,
      theme,
    );
  }
});

test("detected tasks can be confirmed without typing or dismissed without deleting local activity", async () => {
  for (const [lang, theme] of [
    ["en", "light"],
    ["ja", "light"],
    ["en", "dark"],
  ]) {
    await scenario(
      async (page) => {
        const ja = lang === "ja";
        const nav = page.getByRole("group", {
          name: ja ? "作業履歴の表示" : "Work History sections",
        });
        await page.evaluate(() => window.contextTest.seedDetected());
        await nav
          .getByRole("button", {
            name: ja ? "操作履歴" : "Activity",
            exact: true,
          })
          .click();
        await page
          .getByRole("button", { name: ja ? "更新" : "Refresh", exact: true })
          .click();
        await page
          .getByRole("button", {
            name: ja ? "作業を確認" : "Review tasks",
            exact: true,
          })
          .waitFor();
        const eventCount = await page.getByRole("checkbox").count();
        assert.equal(eventCount, 12);
        const capture = async (name) => {
          if (process.env.MONITOR_AXE_PATH) {
            await page.addScriptTag({ path: process.env.MONITOR_AXE_PATH });
            const result = await page.evaluate(async () =>
              window.axe.run(document.querySelector(".history-page"), {
                runOnly: {
                  type: "tag",
                  values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"],
                },
              }),
            );
            assert.deepEqual(
              result.violations.map((v) => ({
                id: v.id,
                nodes: v.nodes.map((n) => n.target),
              })),
              [],
            );
          }
          if (process.env.MONITOR_SCREENSHOT_DIR)
            await page.screenshot({
              path: `${process.env.MONITOR_SCREENSHOT_DIR}/monitor-auto-${name}-${lang}-${theme}.png`,
            });
        };
        await capture("activity");
        await page
          .getByRole("button", {
            name: ja ? "作業を確認" : "Review tasks",
            exact: true,
          })
          .click();
        await capture("detected");
        const review = page.getByRole("button", {
          name: ja ? "作業を確認" : "Review task",
          exact: true,
        });
        assert.equal(await review.count(), 2);
        await review.first().click();
        const heading = page.getByRole("heading", {
          name: ja
            ? "これらは1つの作業ですか？"
            : "Do these actions belong to one task?",
          exact: true,
        });
        assert.equal(
          await heading.evaluate((e) => e === document.activeElement),
          true,
        );
        assert.equal(
          await page.getByRole("textbox").count(),
          0,
          "default review requires no typing",
        );
        await capture("review");
        await page
          .getByRole("button", {
            name: ja ? "はい、この作業を保存" : "Yes, save this task",
            exact: true,
          })
          .click();
        await page
          .getByRole("heading", {
            name: ja ? "確認した作業" : "Reviewed tasks",
            exact: true,
          })
          .waitFor();
        assert.equal(await review.count(), 1);
        const savedName = await page.evaluate(
          () =>
            window.contextTest.calls.find(
              (c) => c.cmd === "save_context_workflow",
            ).args.workflow.name,
        );
        assert.ok(savedName.includes("Google Sheets"));
        await page.getByText(savedName, { exact: true }).waitFor();
        await capture("reviewed");
        await nav
          .getByRole("button", {
            name: ja ? "操作履歴" : "Activity",
            exact: true,
          })
          .click();
        await page
          .getByRole("button", { name: ja ? "更新" : "Refresh", exact: true })
          .click();
        await nav
          .getByRole("button", {
            name: ja ? "繰り返す作業" : "Repeated tasks",
            exact: true,
          })
          .click();
        assert.equal(
          await page.getByText(savedName, { exact: true }).count(),
          1,
          "refresh must not duplicate reviewed tasks",
        );
        assert.equal(
          await review.count(),
          1,
          "reviewed pattern must not be suggested again",
        );
        await review.click();
        await page
          .getByRole("button", {
            name: ja ? "1つの作業ではない" : "Not one task",
            exact: true,
          })
          .click();
        assert.equal(await review.count(), 0);
        await nav
          .getByRole("button", {
            name: ja ? "操作履歴" : "Activity",
            exact: true,
          })
          .click();
        assert.equal(
          await page.getByRole("checkbox").count(),
          eventCount,
          "dismissal must preserve activity",
        );
        await page.getByRole("searchbox").fill("Google Sheets");
        assert.equal(await page.getByRole("checkbox").count(), eventCount);
        await page
          .getByRole("button", {
            name: ja ? "端末にコピーを保存" : "Save local copy",
            exact: true,
          })
          .click();
        await page
          .getByText(
            ja
              ? "端末にコピーを保存しました。アップロードはしていません。"
              : "Local copy saved. Nothing was uploaded.",
            { exact: true },
          )
          .waitFor();
        const calls = await page.evaluate(() => window.contextTest.calls);
        assert.equal(
          calls.filter((c) =>
            [
              "send_context_package",
              "enable_context_summary",
              "prepare_context_package",
            ].includes(c.cmd),
          ).length,
          0,
          "local review/export must not prepare or send cloud data",
        );
      },
      lang,
      theme,
    );
  }
});
