import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const { chromium } = await import(process.env.MONITOR_PLAYWRIGHT_MODULE || "playwright");
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
  await readFile(new URL("../src-tauri/fixtures/context-package-v1.json", import.meta.url), "utf8"),
);
async function scenario(run) {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 860, height: 580 } });
    page.setDefaultTimeout(5000);
    await page.route("**/*", (r) =>
      r.request().url().startsWith(`${base}/`) ? r.continue() : r.abort(),
    );
    await page.addInitScript(
      ({ fixture }) => {
        localStorage.setItem("sanka-monitor.app-lang", "en");
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
              settings: { enabled: false, paused: true, allowedAppIds: [], allowedDomains: [] },
              active: false,
              events: [],
              candidates: [],
              workflows: [
                { ...fixture.workflows[0], name: `Account ${id.toUpperCase()} workflow` },
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
          enableRecurring: () => {
            histories[current].recurring = true;
          },
        };
        window.__TAURI_INTERNALS__ = {
          metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
          transformCallback: (cb) => {
            const id = nextCallback++;
            callbacks.set(id, cb);
            return id;
          },
          invoke: async (cmd, args) => {
            calls.push({ cmd, args, account: current });
            if (cmd === "plugin:event|listen") {
              events.set(args.event, [...(events.get(args.event) || []), args.handler]);
              return nextCallback++;
            }
            if (cmd === "get_status")
              return {
                firstRun: false,
                enrolled: false,
                agentVersion: "synthetic",
                posture: [],
                todayUsage: [],
                device: { platform: "macos", osVersion: "synthetic", hostname: "synthetic" },
                lastCheckin: null,
                updateReady: null,
                justUpdated: null,
              };
            if (cmd === "get_account") return account();
            if (cmd === "get_transparency" || cmd === "get_saved_reports") return [];
            if (cmd === "get_saas_settings") return { lastScheduledCheckAt: null };
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
              histories[current].recurring = false;
              histories[current].revocationPending = true;
              histories[current].error = "Daily sharing stopped locally; remote revocation pending";
              throw new Error(histories[current].error);
            }
            return null;
          },
        };
      },
      { fixture },
    );
    await page.goto(base);
    await page.getByRole("button", { name: "Work History", exact: true }).click();
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
      await page.getByRole("button", { name: "Describe / edit workflow", exact: true }).click();
      assert.equal(
        await page.getByLabel("Name", { exact: true }).inputValue(),
        "Account A workflow",
      );
      await page.evaluate(() => window.contextTest.deferHistory());
      await page.getByRole("button", { name: "Refresh", exact: true }).click();
      await page.waitForFunction(
        () => window.contextTest.calls.filter((c) => c.cmd === "get_context_history").length >= 2,
      );
      await page.evaluate((fail) => window.contextTest.switchAccount(fail), failNew);
      await page.waitForFunction(() =>
        window.contextTest.calls.some((c) => c.cmd === "get_context_history" && c.account === "b"),
      );
      await page.evaluate(() => window.contextTest.releaseHistory());
      await page.waitForFunction(() => !document.querySelector('[aria-busy="true"]'));
      assert.equal(
        await page.getByLabel("Name", { exact: true }).count(),
        0,
        "old editor must unmount",
      );
      assert.equal(
        await page.getByText("Account A workflow", { exact: true }).count(),
        0,
        "old refresh must not restore old data",
      );
      if (!failNew) await page.getByText("Account B workflow", { exact: true }).waitFor();
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
    await page
      .getByLabel("I use my own OS login (shared logins must remain inventory-only)")
      .check();
    await page.evaluate(() => window.contextTest.deferPreview());
    await page.getByRole("button", { name: "Prepare exact preview", exact: true }).click();
    await page.waitForFunction(() =>
      window.contextTest.calls.some((c) => c.cmd === "prepare_context_package"),
    );
    await page.evaluate(() => window.contextTest.switchAccount());
    await page.getByText("Account B workflow", { exact: true }).waitFor();
    await page.evaluate(() => window.contextTest.releasePreview());
    assert.equal(
      await page.getByRole("button", { name: "Send this package", exact: true }).count(),
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
    await page.evaluate(() => window.contextTest.enableRecurring());
    await page.getByRole("button", { name: "Refresh", exact: true }).click();
    await page
      .getByText("Daily company sharing enabled for this app session", { exact: true })
      .waitFor();
    await page
      .getByRole("group", { name: "Apps to include", exact: true })
      .getByLabel("Google Sheets (Chrome / Edge)", { exact: true })
      .check();
    await page.waitForFunction(() =>
      window.contextTest.calls.some((c) => c.cmd === "invalidate_context_selection"),
    );
    await page.getByRole("status").filter({ hasText: "Remote revocation is pending" }).waitFor();
    assert.equal(
      await page
        .getByText("Daily company sharing enabled for this app session", { exact: true })
        .count(),
      0,
    );
    await page.getByLabel("Audience", { exact: true }).selectOption("company");
    await page.getByLabel("Audience", { exact: true }).selectOption("personal");
    assert.ok(
      (await page.evaluate(() => window.contextTest.calls)).filter(
        (c) => c.cmd === "invalidate_context_selection",
      ).length >= 3,
    );
  }));
