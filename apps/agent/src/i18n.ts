// The app's wording in English and Japanese. The scan's risks, checks and
// software statuses come from Rust already worded in the same language
// (crates/monitor-report/src/copy.rs); everything else the window shows is
// here. Japanese follows the workspace dictionary (見える化, リスク, 登録コード)
// and ですます調.

import type { Lang } from "./types";

export type Strings = {
  appName: string;
  loading: string;
  never: string;
  languageLabel: string;
  headerLocal: string;
  headerConnected: (paused: boolean) => string;
  tabs: { scan: string; software: string; connect: string; workspace: string; privacy: string };
  scan: {
    scanNow: string;
    scanAgain: string;
    scanning: string;
    reading: string;
    intro: string;
    high: string;
    medium: string;
    low: string;
    pastSupport: string;
    computer: string;
    scanned: string;
    os: string;
    osSupport: string;
    notAssessed: string;
    risks: string;
    noRisks: string;
    checks: string;
    saveTitle: string;
    formats: Record<"xlsx" | "html" | "csv", { label: string; hint: string }>;
    save: string;
    saving: string;
    saveHint: string;
    savedTitle: string;
    open: string;
    showInFolder: string;
    uploadToSanka: string;
    uploadTitle: string;
    uploadIntro: string;
    chooseFile: string;
  };
  software: {
    empty: string;
    onlyFlagged: (count: number) => string;
    nothing: string;
    footer: (date: string) => string;
  };
  upload: {
    file: string;
    size: string;
    fingerprint: string;
    explain: string;
    repEmail: string;
    company: string;
    senderEmail: string;
    note: string;
    send: string;
    sending: string;
    tryAgain: string;
    cancel: string;
    received: (fileName: string, size: string, at: string) => string;
    matches: string;
    mismatch: (sha256: string) => string;
    receipt: (id: string) => string;
    done: string;
  };
  connect: {
    intro: string;
    removed: (at: string) => string;
    revokePending: (error: string | null) => string;
    codeTitle: string;
    codeHint: string;
    codePlaceholder: string;
    connect: string;
    connecting: string;
    shareOnConnect: string;
    paused: string;
    updateRequired: (version: string | null) => string;
    updateNow: string;
    lastError: (error: string) => string;
    fromWorkspace: string;
    open: string;
    workspace: string;
    lastCheckin: string;
    installedSoftware: string;
    softwareSent: (count: number, at: string) => string;
    notSent: string;
    usageSharing: string;
    sharingOn: string;
    sharingOff: string;
    checkinNow: string;
    working: string;
    launchAtLogin: string;
    usageTitle: string;
    shareUsage: string;
    appsToday: string;
    pausedShort: string;
    noneToday: string;
    sharedFrom: (at: string, pending: number) => string;
    disconnect: string;
    disconnectExplain: string;
  };
  privacy: {
    leavesTitle: string;
    leavesConnected: string;
    leavesLocal: string;
    updateChecks: string;
    lastCheck: (at: string) => string;
    checksOff: string;
    lastAttemptFailed: (error: string) => string;
    viewsTitle: string;
    viewsIntro: string;
    viewsEmpty: string;
    viewerUnnamed: string;
    viewedDay: (day: string) => string;
    viewedDays: (from: string, to: string) => string;
    viewReason: (reason: string) => string;
    viewsShown: (shown: number, total: number) => string;
    viewsOffline: string;
    viewsError: (error: string) => string;
    viewsRetry: string;
    readsTitle: string;
    reads: string[];
    neverTitle: string;
    never: string[];
    sentTitle: string;
    nothingSent: string;
    deliveryUnconfirmed: string;
  };
};

export const STRINGS: Record<Lang, Strings> = {
  en: {
    appName: "Sanka Monitor",
    loading: "Loading…",
    never: "never",
    languageLabel: "Language",
    headerLocal: "local scan, nothing leaves this computer",
    headerConnected: (paused) =>
      `connected to a workspace${paused ? " (paused by workspace)" : ""}`,
    tabs: {
      scan: "Scan",
      software: "Software",
      connect: "Connect",
      workspace: "Workspace",
      privacy: "Privacy",
    },
    scan: {
      scanNow: "Scan this computer",
      scanAgain: "Scan again",
      scanning: "Scanning…",
      reading: "Reading this computer…",
      intro:
        "The scan reads this computer's security settings and installed programs, and checks them against vendor support dates bundled in the app. Nothing leaves this computer.",
      high: "High",
      medium: "Medium",
      low: "Low",
      pastSupport: "Programs past support",
      computer: "Computer",
      scanned: "Scanned",
      os: "OS",
      osSupport: "OS support",
      notAssessed: "Not assessed",
      risks: "Risks",
      noRisks:
        "No risks found. Risks come from vendor support dates and this computer's own settings; no risk does not prove the computer is safe.",
      checks: "Security checks",
      saveTitle: "Save a report",
      formats: {
        xlsx: { label: "Excel (IT asset ledger)", hint: "Devices, software, risks and checks" },
        html: { label: "One-page report", hint: "Opens in a browser; print it to PDF" },
        csv: { label: "CSV", hint: "One row per record, for merging" },
      },
      save: "Save…",
      saving: "Saving…",
      saveHint:
        "Reports are written only where you choose, in the language above. To get your risks reviewed, send the Excel file to your Sanka contact, or upload it below.",
      savedTitle: "Saved this session",
      open: "Open",
      showInFolder: "Show in folder",
      uploadToSanka: "Upload to Sanka…",
      uploadTitle: "Upload a report to Sanka",
      uploadIntro:
        "Only when you choose to: pick a report file (for example the Excel ledger after you filled in users and locations), check it, and send it. Nothing is sent until you press Send.",
      chooseFile: "Choose a report file…",
    },
    software: {
      empty: "Scan this computer to list its installed programs.",
      onlyFlagged: (count) => `Only programs past or near the end of support (${count})`,
      nothing: "Nothing to show.",
      footer: (date) =>
        `Support dates checked on ${date}. Programs the catalog does not know show as not assessed.`,
    },
    upload: {
      file: "File",
      size: "Size",
      fingerprint: "SHA-256",
      explain:
        "Sanka receives this file, byte for byte, and only the details you add below. Nothing else from this computer is sent. Only Sanka staff can open it, and it is used for the assessment you ask for, never for outreach to other companies.",
      repEmail: "Your Sanka contact's email (optional)",
      company: "Company (optional)",
      senderEmail: "Your email, for a reply (optional)",
      note: "Note (optional)",
      send: "Send to Sanka",
      sending: "Sending…",
      tryAgain: "Try again",
      cancel: "Cancel",
      received: (fileName, size, at) => `Sanka received ${fileName} (${size}, ${at}).`,
      matches: "Sanka's fingerprint of the file matches the file you chose, byte for byte.",
      mismatch: (sha256) =>
        `Sanka's fingerprint (${sha256}) does not match the file you chose. Send it again or send the file another way.`,
      receipt: (id) => `Receipt ${id}. Keep it if you contact Sanka about this report.`,
      done: "Done",
    },
    connect: {
      intro:
        "Sanka Monitor works on its own: scanning and saving reports needs no account. Connect this computer only if your company uses the Sanka Monitor console at monitor.sanka.com for continuous visibility. Connected computers send their security checks and installed programs to the workspace; app usage is shared only if you also opt in below.",
      removed: (at) =>
        `Your workspace removed this device (${at}), so Sanka Monitor disconnected it and deleted its usage history and credential here.`,
      revokePending: (error) =>
        `This device is disconnected and its usage history here is deleted, but your workspace has not confirmed yet. Sanka Monitor keeps only the device credential and tries again every hour and at login; then your workspace removes the device and deletes the app usage it received.${error ? ` Last attempt: ${error}` : ""}`,
      codeTitle: "Enrollment code",
      codeHint:
        "Ask a workspace admin for a one-time enrollment code, or generate one from Sanka Monitor → Devices.",
      codePlaceholder: "Paste enrollment code",
      connect: "Connect",
      connecting: "Connecting…",
      shareOnConnect:
        "Also share which apps I use each day with my workspace, from now on. Off by default; nothing about app usage is recorded unless this is on, and you can turn it off at any time.",
      paused:
        "Your workspace has paused Sanka Monitor. App usage, security checks and installed software are not collected or shared until it is turned back on. Check-ins still send this device's name, model, serial number and OS version, to learn when that happens.",
      updateRequired: (version) =>
        `Your workspace needs Sanka Monitor ${version ?? "a newer version"} or newer. Sharing waits until this device is updated.`,
      updateNow: "Update now",
      lastError: (error) => `Last check-in failed: ${error}`,
      fromWorkspace: "From your workspace",
      open: "Open",
      workspace: "Workspace",
      lastCheckin: "Last check-in",
      installedSoftware: "Installed software",
      softwareSent: (count, at) => `${count} programs · sent ${at}`,
      notSent: "Not sent yet",
      usageSharing: "Usage sharing",
      sharingOn: "On (opted in)",
      sharingOff: "Off",
      checkinNow: "Check in now",
      working: "Working…",
      launchAtLogin: "Open Sanka Monitor when I log in",
      usageTitle: "App usage",
      shareUsage:
        "Share which apps I use each day with my workspace. The workspace console shows daily app presence (which apps, on which days), never how long. Only usage recorded after you turn this on is shared; while it is off, no app usage is recorded at all.",
      appsToday: "Apps used today on this device:",
      pausedShort: "Paused by your workspace.",
      noneToday: "None recorded yet today.",
      sharedFrom: (at, pending) =>
        `Shared from ${at} · ${pending} record(s) waiting to upload. This device keeps its own history for 14 days.`,
      disconnect: "Disconnect this device",
      disconnectExplain:
        " removes it from your workspace, which deletes the app usage it received, then deletes its usage history and credential here. Scanning and local reports keep working.",
    },
    privacy: {
      leavesTitle: "What leaves this computer",
      leavesConnected:
        "This computer is connected to a workspace: check-ins send its security checks and installed programs, and app usage only if you opted in. Every request is listed below, exactly as sent.",
      leavesLocal:
        "Nothing, unless you upload a report to Sanka yourself. Scans and reports stay on this computer. The only automatic network request is the update check below, which downloads the latest version number and sends nothing about this computer.",
      updateChecks:
        "Check for updates automatically (hourly, from github.com/sankaHQ/monitor-releases).",
      lastCheck: (at) => ` Last check: ${at}.`,
      checksOff: " Off: install new versions yourself.",
      lastAttemptFailed: (error) => ` Last attempt failed: ${error}`,
      viewsTitle: "Who opened your data",
      viewsIntro:
        "When an IT admin in your workspace opens your app usage (which apps, on which days), Sanka records when, who, the days they viewed and the reason they gave. This list is loaded from your workspace when you open this tab, and past views stay listed even if you stop sharing app usage.",
      viewsEmpty: "No one has opened your app data.",
      viewerUnnamed: "An IT admin",
      viewedDay: (day) => `Day viewed: ${day}`,
      viewedDays: (from, to) => `Days viewed: ${from} – ${to}`,
      viewReason: (reason) => `Reason: ${reason}`,
      viewsShown: (shown, total) => `Showing the latest ${shown} of ${total}.`,
      viewsOffline: "Could not reach your workspace. Check your network connection and try again.",
      viewsError: (error) => `Could not load this list: ${error}`,
      viewsRetry: "Try again",
      readsTitle: "What a scan reads",
      reads: [
        "Security settings: disk encryption, firewall, Gatekeeper or SmartScreen, screen lock, OS updates, automatic updates, antivirus, admin rights, device management, Secure Boot or System Integrity Protection",
        "Installed apps and programs: name, version and publisher",
        "Device identity: model, OS version and edition, computer name, serial number",
      ],
      neverTitle: "Never collected",
      never: [
        "Window titles, URLs, or document names",
        "Screen contents, screenshots, or recordings",
        "Keystrokes or clipboard contents",
        "Where programs are installed, or the files on this device",
        "How long any app was used",
      ],
      sentTitle: "Exactly what this app sent",
      nothingSent: "Nothing sent yet.",
      deliveryUnconfirmed: " · delivery unconfirmed",
    },
  },
  ja: {
    appName: "Sanka Monitor",
    loading: "読み込み中…",
    never: "なし",
    languageLabel: "表示言語",
    headerLocal: "ローカルスキャン、データはこのPCの外に出ません",
    headerConnected: (paused) =>
      `ワークスペースに接続中${paused ? "（ワークスペースにより一時停止中）" : ""}`,
    tabs: {
      scan: "スキャン",
      software: "ソフトウェア",
      connect: "接続",
      workspace: "ワークスペース",
      privacy: "プライバシー",
    },
    scan: {
      scanNow: "このPCをスキャン",
      scanAgain: "もう一度スキャン",
      scanning: "スキャン中…",
      reading: "このPCを読み取っています…",
      intro:
        "このPCのセキュリティ設定とインストール済みのソフトウェアを読み取り、アプリに同梱した提供元のサポート期限と照らし合わせます。データはこのPCの外に出ません。",
      high: "高",
      medium: "中",
      low: "低",
      pastSupport: "サポート終了のソフトウェア",
      computer: "コンピューター",
      scanned: "スキャン日時",
      os: "OS",
      osSupport: "OSサポート状況",
      notAssessed: "未判定",
      risks: "リスク",
      noRisks:
        "リスクは見つかりませんでした。リスクは提供元のサポート期限とこのPCの設定から判定しています。リスクが表示されなくても、PCが安全とは限りません。",
      checks: "セキュリティチェック",
      saveTitle: "レポートを保存",
      formats: {
        xlsx: {
          label: "Excel（IT資産台帳）",
          hint: "デバイス、ソフトウェア、リスク、チェック結果",
        },
        html: { label: "1ページのレポート", hint: "ブラウザーで開き、PDFとして印刷できます" },
        csv: { label: "CSV", hint: "1行1レコード。複数のPCをまとめるときに使います" },
      },
      save: "保存…",
      saving: "保存中…",
      saveHint:
        "レポートは指定した場所にだけ、上で選んだ言語で保存します。リスクの確認を依頼するときは、ExcelファイルをSankaの担当者に送るか、下からアップロードしてください。",
      savedTitle: "このセッションで保存したレポート",
      open: "開く",
      showInFolder: "フォルダーで表示",
      uploadToSanka: "Sankaにアップロード…",
      uploadTitle: "レポートをSankaにアップロード",
      uploadIntro:
        "送ると決めたときだけ使います。レポートファイル（使用者や設置場所を記入したExcelの台帳など）を選んで内容を確認し、送信します。［送信］を押すまで何も送信しません。",
      chooseFile: "レポートファイルを選ぶ…",
    },
    software: {
      empty: "このPCをスキャンすると、インストール済みのソフトウェアが一覧表示されます。",
      onlyFlagged: (count) => `サポート終了・終了間近のソフトウェアだけを表示（${count}件）`,
      nothing: "表示するものはありません。",
      footer: (date) =>
        `サポート期限の確認日：${date}。カタログにないソフトウェアは未判定と表示されます。`,
    },
    upload: {
      file: "ファイル",
      size: "サイズ",
      fingerprint: "SHA-256",
      explain:
        "Sankaが受け取るのは、このファイルそのものと、下に入力した情報だけです。このPCのそれ以外の情報は送信しません。開けるのはSankaの社員だけで、ご依頼いただいたアセスメントのためだけに使い、他社への営業には使いません。",
      repEmail: "Sankaの担当者のメールアドレス（任意）",
      company: "会社名（任意）",
      senderEmail: "返信先のメールアドレス（任意）",
      note: "メモ（任意）",
      send: "Sankaに送信",
      sending: "送信中…",
      tryAgain: "もう一度送信",
      cancel: "キャンセル",
      received: (fileName, size, at) => `Sankaが ${fileName} を受け取りました（${size}、${at}）。`,
      matches: "Sankaが記録したフィンガープリントは、選んだファイルと完全に一致しています。",
      mismatch: (sha256) =>
        `Sankaが記録したフィンガープリント（${sha256}）が、選んだファイルと一致しません。もう一度送信するか、別の方法でファイルを送ってください。`,
      receipt: (id) =>
        `受付番号 ${id}。このレポートについてSankaに問い合わせるときにお使いください。`,
      done: "閉じる",
    },
    connect: {
      intro:
        "Sanka Monitorは単独で使えます。スキャンとレポートの保存にアカウントは不要です。このPCを接続するのは、会社が monitor.sanka.com のSanka Monitorコンソールで継続的に見える化している場合だけです。接続したPCは、セキュリティチェックとインストール済みのソフトウェアをワークスペースに送信します。アプリの利用データは、下で共有をオンにした場合だけ共有します。",
      removed: (at) =>
        `ワークスペースがこのデバイスを削除したため（${at}）、Sanka Monitorは接続を解除し、このPC内の利用履歴と認証情報を削除しました。`,
      revokePending: (error) =>
        `このデバイスは接続を解除し、PC内の利用履歴も削除しましたが、ワークスペースの確認がまだです。Sanka Monitorはデバイスの認証情報だけを残し、1時間ごととログイン時に再試行します。確認が取れると、ワークスペースはデバイスを削除し、受け取った利用データを削除します。${error ? `前回の試行：${error}` : ""}`,
      codeTitle: "登録コード",
      codeHint:
        "ワークスペースの管理者にワンタイムの登録コードを依頼するか、Sanka Monitorのデバイス画面で発行してください。",
      codePlaceholder: "登録コードを貼り付け",
      connect: "接続",
      connecting: "接続中…",
      shareOnConnect:
        "今後、日ごとに使ったアプリもワークスペースと共有します。初期状態ではオフで、オンにしない限りアプリの利用データは記録しません。いつでもオフにできます。",
      paused:
        "ワークスペースがSanka Monitorを一時停止しています。再開されるまで、アプリの利用データ、セキュリティチェック、インストール済みのソフトウェアは収集・共有しません。再開を知るため、チェックインではこのデバイスの名前・機種・シリアル番号・OSのバージョンだけを送信します。",
      updateRequired: (version) =>
        `ワークスペースはSanka Monitor ${version ?? "の新しいバージョン"}以降を求めています。このデバイスを更新するまで共有を停止します。`,
      updateNow: "今すぐ更新",
      lastError: (error) => `前回のチェックインに失敗しました：${error}`,
      fromWorkspace: "ワークスペースからのお知らせ",
      open: "開く",
      workspace: "ワークスペース",
      lastCheckin: "最終チェックイン",
      installedSoftware: "インストール済みソフトウェア",
      softwareSent: (count, at) => `${count}件 · ${at}に送信`,
      notSent: "まだ送信していません",
      usageSharing: "利用データの共有",
      sharingOn: "オン（共有中）",
      sharingOff: "オフ",
      checkinNow: "今すぐチェックイン",
      working: "処理中…",
      launchAtLogin: "ログイン時にSanka Monitorを開く",
      usageTitle: "アプリの利用",
      shareUsage:
        "日ごとに使ったアプリをワークスペースと共有します。コンソールに表示されるのは、どの日にどのアプリを使ったかだけで、利用時間は表示されません。共有されるのはオンにした後に記録されたデータだけで、オフの間はアプリの利用データを一切記録しません。",
      appsToday: "このPCで今日使ったアプリ：",
      pausedShort: "ワークスペースにより一時停止中です。",
      noneToday: "今日はまだ記録がありません。",
      sharedFrom: (at, pending) =>
        `${at}から共有中 · アップロード待ち ${pending}件。このPCは14日分の履歴を保持します。`,
      disconnect: "このデバイスの接続を解除",
      disconnectExplain:
        "すると、ワークスペースから削除され、受け取ったアプリの利用データも削除されます。その後、このPC内の利用履歴と認証情報を削除します。スキャンとローカルのレポートはそのまま使えます。",
    },
    privacy: {
      leavesTitle: "このPCの外に出る情報",
      leavesConnected:
        "このPCはワークスペースに接続しています。チェックインでセキュリティチェックとインストール済みのソフトウェアを送信し、アプリの利用データは共有をオンにした場合だけ送信します。送信したリクエストは、下にそのまま表示します。",
      leavesLocal:
        "Sankaにレポートをアップロードしない限り、ありません。スキャン結果とレポートはこのPCに残ります。自動で行う通信は下のアップデートの確認だけで、最新のバージョン番号を取得し、このPCの情報は送信しません。",
      updateChecks:
        "アップデートを自動で確認する（1時間ごと、github.com/sankaHQ/monitor-releases から）。",
      lastCheck: (at) => `前回の確認：${at}。`,
      checksOff: "オフ：新しいバージョンはご自身でインストールしてください。",
      lastAttemptFailed: (error) => `前回の確認に失敗しました：${error}`,
      viewsTitle: "あなたのデータを開いた人",
      viewsIntro:
        "ワークスペースのIT管理者があなたのアプリの利用データ（どの日にどのアプリを使ったか）を開くと、Sankaは日時、開いた人、対象の期間、管理者が入力した理由を記録します。この一覧は、このタブを開いたときにワークスペースから読み込みます。アプリの利用データの共有をオフにした後も、過去の記録は表示されます。",
      viewsEmpty: "まだ誰もあなたのアプリデータを開いていません。",
      viewerUnnamed: "IT管理者",
      viewedDay: (day) => `対象の日：${day}`,
      viewedDays: (from, to) => `対象の期間：${from}〜${to}`,
      viewReason: (reason) => `理由：${reason}`,
      viewsShown: (shown, total) => `最新の${shown}件を表示しています（全${total}件）。`,
      viewsOffline:
        "ワークスペースに接続できませんでした。ネットワークの接続を確認して、もう一度お試しください。",
      viewsError: (error) => `この一覧を読み込めませんでした：${error}`,
      viewsRetry: "再読み込み",
      readsTitle: "スキャンで読み取る情報",
      reads: [
        "セキュリティ設定：ディスク暗号化、ファイアウォール、Gatekeeper または SmartScreen、画面ロック、OSアップデート、自動アップデート、ウイルス対策、管理者権限、デバイス管理、セキュアブートまたはシステム整合性保護",
        "インストール済みのアプリとソフトウェア：名前、バージョン、提供元",
        "デバイスの情報：機種、OSのバージョンとエディション、コンピューター名、シリアル番号",
      ],
      neverTitle: "収集しない情報",
      never: [
        "ウィンドウタイトル、URL、ドキュメント名",
        "画面の内容、スクリーンショット、録画",
        "キー入力、クリップボードの内容",
        "ソフトウェアのインストール先、このPC内のファイル",
        "アプリの利用時間",
      ],
      sentTitle: "このアプリが送信した内容",
      nothingSent: "まだ何も送信していません。",
      deliveryUnconfirmed: " · 配信未確認",
    },
  },
};

/** The OS language, when the person has not chosen one. */
export function systemLang(): Lang {
  return navigator.language.toLowerCase().startsWith("ja") ? "ja" : "en";
}

/** Dates in the app's language. */
export function formatTime(iso: string | null, lang: Lang): string {
  if (!iso) return STRINGS[lang].never;
  const date = new Date(iso);
  return Number.isNaN(date.getTime())
    ? STRINGS[lang].never
    : date.toLocaleString(lang === "ja" ? "ja-JP" : "en-US");
}

/** A calendar day (YYYY-MM-DD) in the app's language, on that day wherever the computer is. */
export function formatDay(day: string, lang: Lang): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!match) return day;
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return date.toLocaleDateString(lang === "ja" ? "ja-JP" : "en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
