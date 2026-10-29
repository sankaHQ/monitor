const ja = chrome.i18n.getUILanguage().startsWith("ja");
const copy = ja
  ? {
      intro:
        "Google スプレッドシートの一部の操作をローカルで記録します。セルの内容、文書名、キー入力は収集しません。",
      coverage:
        "ChromeとEdgeに対応しています。英語と日本語の一部のメニュー操作のみです。操作の完了は確認できません。",
      setup:
        "Sanka Monitorアプリでブラウザ連携を登録し、作業履歴とGoogle スプレッドシートを有効にしてください。共有する前にアプリで内容を確認できます。ブラウザの再起動後は再度有効にしてください。",
      on: "このブラウザで有効にする",
      off: "記録を停止",
      paused: "記録は停止しています。",
      active: "ローカル記録が有効です。",
      not_running: "アプリとの接続を待っています。",
      unsupported: "アプリの設定を確認してください。",
      invalid_message: "接続できませんでした。",
      error: "変更できませんでした。もう一度お試しください。",
    }
  : {
      intro:
        "Recognize selected Google Sheets commands locally. No cell contents, document names or keystrokes are collected.",
      coverage:
        "Chrome and Edge. Selected English and Japanese menu commands only; a click does not prove completion.",
      setup:
        "Register this browser in Sanka Monitor, then enable Work History and Google Sheets there. Review in the app before sharing. Re-enable here after restarting your browser.",
      on: "Enable in this browser",
      off: "Stop capture",
      paused: "Capture is paused.",
      active: "Local capture is active.",
      not_running: "Waiting for the desktop app connection.",
      unsupported: "Check the desktop app settings.",
      invalid_message: "Could not connect.",
      error: "Could not change capture. Please try again.",
    };
document.documentElement.lang = ja ? "ja" : "en";
for (const key of ["intro", "coverage", "setup"])
  document.getElementById(key).textContent = copy[key];
const button = document.getElementById("toggle");
let current = false;
async function refresh() {
  const state = await chrome.runtime.sendMessage({ kind: "status" });
  current = state.enabled === true;
  button.textContent = current ? copy.off : copy.on;
  document.getElementById("status").textContent = current
    ? (copy[state.status] ?? copy.not_running)
    : copy.paused;
  button.disabled = false;
}
button.addEventListener("click", async () => {
  button.disabled = true;
  try {
    if (
      !current &&
      !(await chrome.permissions.request({
        origins: ["https://docs.google.com/*"],
      }))
    )
      return;
    const result = await chrome.runtime.sendMessage({
      kind: "set-enabled",
      enabled: !current,
    });
    if (!result?.ok) throw new Error("capture unavailable");
  } catch {
    document.getElementById("status").textContent = copy.error;
  } finally {
    await refresh();
  }
});
void refresh();
