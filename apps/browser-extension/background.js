importScripts("rules.js");
const HOST = "com.sanka.monitor.context";
const MATCH = "https://docs.google.com/spreadsheets/*";
const ORIGIN = "https://docs.google.com/*";
let generation = 0;
let port = null;
let busy = false;
let lastSent = 0;
let nativeStatus = "not_running";
const script = {
  id: "sheets-context",
  matches: [MATCH],
  js: ["rules.js", "content.js"],
  allFrames: false,
  runAt: "document_idle",
  world: "ISOLATED",
  persistAcrossSessions: false,
};

function bridge(message) {
  try {
    if (!port) {
      port = chrome.runtime.connectNative(HOST);
      port.onMessage.addListener((reply) => {
        if (
          reply?.schemaVersion === 1 &&
          [
            "active",
            "paused",
            "not_running",
            "unsupported",
            "invalid_message",
          ].includes(reply.status)
        )
          nativeStatus = reply.status;
      });
      port.onDisconnect.addListener(() => {
        void chrome.runtime.lastError;
        port = null;
        nativeStatus = "not_running";
      });
    }
    port.postMessage({ schemaVersion: 1, ...message });
  } catch {
    port = null;
    nativeStatus = "not_running";
  }
}
function pause() {
  generation++;
  if (port) bridge({ kind: "pause", site: "google-sheets" });
}
async function enabled() {
  return (
    (await chrome.storage.session.get("enabled")).enabled === true &&
    (await chrome.permissions.contains({ origins: [ORIGIN] }))
  );
}
async function syncScripts() {
  await chrome.scripting
    .unregisterContentScripts({ ids: [script.id] })
    .catch(() => {});
  const active = await enabled();
  if (active) await chrome.scripting.registerContentScripts([script]);
  for (const tab of await chrome.tabs.query(active ? { url: MATCH } : {})) {
    if (tab.incognito || !tab.id) continue;
    if (active)
      await chrome.scripting
        .executeScript({ target: { tabId: tab.id }, files: script.js })
        .catch(() => {});
    await chrome.tabs
      .sendMessage(tab.id, { kind: "capture-state", enabled: active })
      .catch(() => {});
  }
}
async function setEnabled(value) {
  pause();
  await chrome.storage.session.set({ enabled: value === true });
  await syncScripts();
  if (value === true && (await enabled())) bridge({ kind: "status" });
  else if (port) {
    port.disconnect();
    port = null;
    nativeStatus = "paused";
  }
}
async function observe(message, sender) {
  if (busy || !MonitorContextRules.observation(message)) return;
  busy = true;
  const current = generation;
  try {
    if (!sender.tab?.id || !(await enabled())) return;
    const tab = await chrome.tabs.get(sender.tab.id);
    const window = await chrome.windows.get(tab.windowId);
    if (
      !MonitorContextRules.allowedSender(
        sender,
        tab,
        window,
        chrome.runtime.id,
      ) ||
      current !== generation ||
      Date.now() - lastSent < 1000 ||
      !(await enabled())
    )
      return;
    if (current !== generation) return;
    lastSent = Date.now();
    bridge({
      kind: "observation",
      site: "google-sheets",
      action: message.action,
      objectCategory: "spreadsheet",
      provenance: "inferred",
    });
  } catch {
    pause();
  } finally {
    busy = false;
  }
}
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (sender.id !== chrome.runtime.id) return false;
  if (sender.tab) {
    void observe(message, sender);
    return false;
  }
  if (sender.url !== chrome.runtime.getURL("popup.html")) return false;
  if (message?.kind === "set-enabled") {
    void setEnabled(message.enabled)
      .then(() => reply({ ok: true }))
      .catch(() => reply({ ok: false }));
    return true;
  }
  if (message?.kind === "status") {
    void enabled().then((value) =>
      reply({ enabled: value, status: nativeStatus }),
    );
    return true;
  }
  return false;
});
chrome.tabs.onActivated.addListener(pause);
chrome.windows.onFocusChanged.addListener(pause);
chrome.tabs.onUpdated.addListener((_id, changes) => {
  if (changes.status === "loading" || changes.url) pause();
});
chrome.permissions.onRemoved.addListener(() => {
  void setEnabled(false);
});
chrome.runtime.onStartup.addListener(() => {
  void setEnabled(false);
});
// Reestablish scripts after a worker restart, but never turn capture on.
void syncScripts().catch(() => {});
