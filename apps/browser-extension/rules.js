/* Shared by the isolated content script, service worker and owning Node checks. */
(() => {
  const labels = new Map([
    ["Download", "export"],
    ["ダウンロード", "export"],
    ["Import", "import"],
    ["インポート", "import"],
    ["Make a copy", "create"],
    ["コピーを作成", "create"],
    ["Share", "send"],
    ["共有", "send"],
    ["Print", "export"],
    ["印刷", "export"],
  ]);
  function sheetsUrl(value) {
    try {
      const url = new URL(value);
      return (
        url.protocol === "https:" &&
        url.hostname === "docs.google.com" &&
        url.port === "" &&
        url.username === "" &&
        url.password === "" &&
        url.pathname.startsWith("/spreadsheets/")
      );
    } catch {
      return false;
    }
  }
  function action(label) {
    if (typeof label !== "string" || label.length > 80) return null;
    return labels.get(label.trim().replace(/…$/, "")) ?? null;
  }
  function allowedSender(sender, tab, window, extensionId) {
    return (
      sender?.id === extensionId &&
      sender.frameId === 0 &&
      sender.origin === "https://docs.google.com" &&
      sender.tab?.incognito === false &&
      tab?.incognito === false &&
      sender.tab?.id === tab?.id &&
      tab?.active === true &&
      window?.focused === true &&
      tab?.windowId === window?.id &&
      sheetsUrl(sender.url) &&
      sheetsUrl(tab.url)
    );
  }
  function observation(value) {
    return (
      value &&
      Object.keys(value).sort().join(",") === "action,kind" &&
      value.kind === "observation" &&
      [...labels.values()].includes(value.action)
    );
  }
  const rules = { sheetsUrl, action, allowedSender, observation };
  globalThis.MonitorContextRules = rules;
  if (typeof module !== "undefined") module.exports = rules;
})();
