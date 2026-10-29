(() => {
  if (globalThis.monitorContextInstalled || window !== window.top) return;
  globalThis.monitorContextInstalled = true;
  let enabled = true;
  chrome.runtime.onMessage.addListener((message) => {
    if (message?.kind === "capture-state") enabled = message.enabled === true;
  });
  document.addEventListener(
    "click",
    (event) => {
      if (
        !enabled ||
        !event.isTrusted ||
        !document.hasFocus() ||
        document.visibilityState !== "visible" ||
        !MonitorContextRules.sheetsUrl(location.href)
      )
        return;
      let node = event.target instanceof Element ? event.target : null;
      let control = null;
      let safeContainer = false;
      let reachedBody = false;
      for (
        let depth = 0;
        node && depth < 20;
        depth++, node = node.parentElement
      ) {
        if (
          node.matches(
            'input,textarea,select,[contenteditable]:not([contenteditable="false"]),[role="textbox"],[role="gridcell"]',
          ) ||
          node.isContentEditable
        )
          return;
        const role = node.getAttribute("role");
        if (!control && (role === "button" || role === "menuitem"))
          control = node;
        if (role === "toolbar" || role === "menu") safeContainer = true;
        if (node === document.body) {
          reachedBody = true;
          break;
        }
      }
      if (!control || !safeContainer || !reachedBody) return;
      // Only exact command labels are interpreted; no page/cell text enters a message.
      const label = control.getAttribute("aria-label");
      let action = MonitorContextRules.action(label);
      if (!action && control.getAttribute("role") === "menuitem") {
        const caption = control.querySelector(
          ":scope > .goog-menuitem-content",
        );
        if (
          caption &&
          caption.childElementCount === 0 &&
          caption.textContent.length <= 80
        )
          action = MonitorContextRules.action(caption.textContent);
      }
      if (action)
        chrome.runtime
          .sendMessage({ kind: "observation", action })
          .catch(() => {});
    },
    true,
  );
})();
