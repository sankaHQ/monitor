// The app's wording in English and Japanese. The scan's risks, checks and
// software statuses come from Rust already worded in the same language
// (crates/monitor-report/src/copy.rs); everything else the window shows is
// here. Japanese follows the workspace dictionary (見える化, リスク, 登録コード)
// and ですます調.

import type { Lang, Theme } from "./types";

export type Strings = {
  appName: string;
  loading: string;
  never: string;
  themeLabel: string;
  themes: Record<Theme, string>;
  headerLocal: string;
  headerConnected: (paused: boolean) => string;
  tabs: {
    scan: string;
    software: string;
    saas: string;
    connect: string;
    workspace: string;
    privacy: string;
  };
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
    reportLanguage: string;
    formats: Record<"xlsx" | "html" | "csv", { label: string; hint: string }>;
    save: string;
    saving: string;
    saveHint: string;
    savedTitle: string;
    open: string;
    showInFolder: string;
    sendToSanka: string;
  };
  software: {
    empty: string;
    onlyFlagged: (count: number) => string;
    nothing: string;
    footer: (date: string) => string;
  };
  saas: {
    intro: string;
    empty: string;
    browserTitle: string;
    browserCheck: string;
    browserExplain: string;
    shareCheck: string;
    shareExplain: string;
    shareHeld: string;
    thisScan: (summary: string) => string;
    safariHint: string;
    allowlistTitle: string;
    allowlistExplain: string;
    machinePath: (path: string) => string;
    machineInUse: string;
    workspaceInUse: string;
    chooseFile: string;
    stopUsing: string;
    allowlistError: (error: string) => string;
    listTitle: string;
    none: string;
    onlyNotAllowed: (count: number) => string;
    migrationPage: string;
    footer: (date: string) => string;
  };
  checks: {
    title: string;
    regular: string;
    regularHint: string;
    daily: string;
    weekly: string;
    last: (at: string) => string;
    nudgesTitle: string;
    dismiss: string;
  };
  send: {
    title: string;
    intro: string;
    signInTitle: string;
    signInIntro: string;
    signIn: string;
    waiting: string;
    codePlaceholder: string;
    useCode: string;
    signingIn: string;
    signedInAs: (who: string, company: string | null) => string;
    signOut: string;
    whatIsSent: string;
    device: string;
    risks: string;
    riskCounts: (high: number, medium: number, low: number) => string;
    programs: string;
    programCounts: (past: number, total: number) => string;
    saas: string;
    saasCounts: (found: number, notAllowed: number, browserCheck: boolean) => string;
    scanned: string;
    language: string;
    file: string;
    size: string;
    fingerprint: string;
    edited: string;
    explain: string;
    note: string;
    send: string;
    sending: string;
    tryAgain: string;
    cancel: string;
    sendEdited: string;
    useScan: string;
    emailInstead: string;
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
    themeLabel: "Appearance",
    themes: { system: "System", light: "Light", dark: "Dark" },
    headerLocal: "local scan, nothing leaves this computer",
    headerConnected: (paused) =>
      `connected to a workspace${paused ? " (paused by workspace)" : ""}`,
    tabs: {
      scan: "Scan",
      software: "Software",
      saas: "SaaS",
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
      reportLanguage: "Report language",
      formats: {
        xlsx: {
          label: "Excel (IT asset ledger)",
          hint: "Devices, software, SaaS, risks and checks",
        },
        html: { label: "One-page report", hint: "Opens in a browser; print it to PDF" },
        csv: { label: "CSV", hint: "One row per record, for merging" },
      },
      save: "Save…",
      saving: "Saving…",
      saveHint:
        "Reports are written only where you choose, in the report language above. To get your risks reviewed, use Send to Sanka, or email the saved Excel to your Sanka contact.",
      savedTitle: "Saved this session",
      open: "Open",
      showInFolder: "Show in folder",
      sendToSanka: "Send to Sanka",
    },
    software: {
      empty: "Scan this computer to list its installed programs.",
      onlyFlagged: (count) => `Only programs past or near the end of support (${count})`,
      nothing: "Nothing to show.",
      footer: (date) =>
        `Support dates checked on ${date}. Programs the catalog does not know show as not assessed.`,
    },
    send: {
      title: "Send to Sanka",
      intro:
        "Sanka reviews the risks with you. Sanka Monitor builds the Excel report from this scan and shows what will be sent; nothing leaves this computer until you press Send to Sanka.",
      signInTitle: "Sign in to send",
      signInIntro:
        "Sending uses your Sanka account, so Sanka receives the report with your name, email address and company. Signing in opens your browser: use Google, Microsoft or email, or create a free account. Scanning never needs an account.",
      signIn: "Sign in with your browser",
      waiting:
        "Finish signing in in your browser. If Sanka Monitor does not come back by itself, paste the sign-in code here.",
      codePlaceholder: "Paste sign-in code",
      useCode: "Sign in with code",
      signingIn: "Signing in…",
      signedInAs: (who, company) => `Signed in as ${who}${company ? ` · ${company}` : ""}`,
      signOut: "Sign out",
      whatIsSent: "Sanka receives this, the app version and, for a scan, its report ID, and your note if you add one:",
      device: "Device",
      risks: "Risks",
      riskCounts: (high, medium, low) => `${high} high · ${medium} medium · ${low} low`,
      programs: "Programs",
      programCounts: (past, total) => `${past} of ${total} past support`,
      saas: "SaaS",
      saasCounts: (found, notAllowed, browserCheck) =>
        `${found} found (${notAllowed} not allowed)` +
        (browserCheck
          ? " · includes each SaaS's last day used and days used in the last 30 (browser check)"
          : ""),
      scanned: "Scanned",
      language: "Language",
      file: "File",
      size: "Size",
      fingerprint: "SHA-256",
      edited: "An edited workbook: sent byte for byte as you saved it, without counts.",
      explain:
        "Nothing else from this computer is sent. Only Sanka staff can open the report, and it is used for the assessment you ask for, never for outreach to other companies.",
      note: "Note (optional)",
      send: "Send to Sanka",
      sending: "Sending…",
      tryAgain: "Try again",
      cancel: "Cancel",
      sendEdited: "Send an edited Excel…",
      useScan: "Send this scan's report instead",
      emailInstead:
        "Prefer not to sign in? Save the Excel below and email it to your Sanka contact.",
      received: (fileName, size, at) => `Sanka received ${fileName} (${size}, ${at}).`,
      matches: "Sanka's fingerprint of the file matches the report shown, byte for byte.",
      mismatch: (sha256) =>
        `Sanka's fingerprint (${sha256}) does not match the report shown. Send it again, or email the saved Excel to your Sanka contact.`,
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
        "This computer is connected to a workspace: check-ins send its security checks and installed programs, and app usage and the SaaS it used only if you turned that sharing on. Check-ins, uploads, the sign-in and sent reports are listed below, exactly as sent.",
      leavesLocal:
        "Nothing, unless you send a report to Sanka yourself. Scans and reports stay on this computer. You sign in to send in your browser; the app itself then sends Sanka only a one-time sign-in code, its version and whether this is a Mac or a Windows PC. The only automatic network request is the update check below, which downloads the latest version number and sends nothing about this computer.",
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
        "Only if you turn on the browser check: your browsers' history on this computer, to find the business SaaS you use (see \"Browser check\")",
      ],
      neverTitle: "Never collected",
      never: [
        "Window titles or document names",
        "Web addresses, page titles or browsing history: even with the browser check on, they are never stored, written to a report or sent",
        "Screen contents, screenshots, or recordings",
        "Keystrokes or clipboard contents",
        "Where programs are installed, or the files on this device",
        "How long any app was used",
      ],
      sentTitle: "Exactly what this app sent",
      nothingSent: "Nothing sent yet.",
      deliveryUnconfirmed: " · delivery unconfirmed",
    },
    saas: {
      intro:
        "The SaaS this computer uses: from its installed desktop apps, which needs no permission, and from the browser check if you turn it on. Nothing leaves this computer.",
      empty: "Scan this computer first.",
      browserTitle: "Browser check",
      browserCheck: "Check my browsers' history on this PC to list the SaaS I use",
      browserExplain:
        "Off unless you turn it on. When it is on, each scan reads the history of Chrome, Edge and Firefox on this computer, and Safari's only if this Mac already gives Sanka Monitor Full Disk Access (the app never asks for it). It matches each site's name against Sanka Monitor's list of business SaaS. For each SaaS it finds, it keeps only the last day you used it and on how many of the last 30 days. Web addresses, page titles and the rest of your history are not stored, are never written to a report and never leave this computer. Reports you save include the SaaS list with those two values. The SaaS list leaves this computer only in a report you send, or, on a connected PC, if you turn on sharing with your workspace below. Turning this off deletes the results, and what you shared.",
      shareCheck: "Share which SaaS this PC used in the last 30 days with my workspace",
      shareExplain:
        "Off by default. When it is on, after each scan Sanka Monitor sends your workspace the SaaS the browser check found in the last 30 days and the last day each was used: never web addresses, page titles or your history. Your workspace's admins see only how many PCs used each SaaS, never which ones. Turning this off stops sharing and deletes what was shared. What it sends is listed under Privacy, exactly as sent.",
      shareHeld:
        "Sharing is on hold: your workspace has paused Sanka Monitor, or this app needs an update. Nothing is sent until then.",
      thisScan: (summary) => `This scan: ${summary}`,
      safariHint:
        "macOS lets an app read Safari's history only with Full Disk Access. Sanka Monitor does not ask for it, because that permission opens every file on this Mac.",
      allowlistTitle: "Company allowlist",
      allowlistExplain:
        "Your IT team can list the SaaS the company allows in a file, placed on this computer by device management or chosen here. SaaS in use that are not on the list appear as risks.",
      machinePath: (path) => `Device management places the company's file at ${path}.`,
      machineInUse: "Your company's file, placed by device management, is in use.",
      workspaceInUse: "Your workspace's list, set in its Sanka Monitor console, is in use.",
      chooseFile: "Choose a file…",
      stopUsing: "Stop using this file",
      allowlistError: (error) => `The allowlist could not be read, so no SaaS is judged: ${error}`,
      listTitle: "SaaS found",
      none: "No SaaS in Sanka Monitor's catalog was found on this computer.",
      onlyNotAllowed: (count) => `Only SaaS not allowed (${count})`,
      migrationPage: "Sanka migration",
      footer: (date) =>
        `SaaS catalog checked on ${date}. Only business SaaS in the catalog are matched; a site it does not list is never recorded.`,
    },
    checks: {
      title: "Regular checks",
      regular:
        "Scan this computer regularly and tell me in Sanka Monitor when a SaaS my company has not allowed, or a new risk, appears.",
      regularHint:
        "Sanka Monitor then starts at login and stays in the menu bar (on Windows, the notification area). Nothing is sent.",
      daily: "Every day",
      weekly: "Every week",
      last: (at) => `Last regular check: ${at}`,
      nudgesTitle: "New since the last scan",
      dismiss: "Dismiss",
    },
  },
  ja: {
    appName: "Sanka Monitor",
    loading: "読み込み中…",
    never: "なし",
    themeLabel: "表示",
    themes: { system: "システム", light: "ライト", dark: "ダーク" },
    headerLocal: "ローカルスキャン、データはこのPCの外に出ません",
    headerConnected: (paused) =>
      `ワークスペースに接続中${paused ? "（ワークスペースにより一時停止中）" : ""}`,
    tabs: {
      scan: "スキャン",
      software: "ソフトウェア",
      saas: "SaaS",
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
      reportLanguage: "レポートの言語",
      formats: {
        xlsx: {
          label: "Excel（IT資産台帳）",
          hint: "デバイス、ソフトウェア、SaaS、リスク、チェック結果",
        },
        html: { label: "1ページのレポート", hint: "ブラウザーで開き、PDFとして印刷できます" },
        csv: { label: "CSV", hint: "1行1レコード。複数のPCをまとめるときに使います" },
      },
      save: "保存…",
      saving: "保存中…",
      saveHint:
        "レポートは指定した場所にだけ、上で選んだレポートの言語で保存します。リスクの確認を依頼するときは［Sankaに送信］を使うか、保存したExcelをSankaの担当者にメールで送ってください。",
      savedTitle: "このセッションで保存したレポート",
      open: "開く",
      showInFolder: "フォルダーで表示",
      sendToSanka: "Sankaに送信",
    },
    software: {
      empty: "このPCをスキャンすると、インストール済みのソフトウェアが一覧表示されます。",
      onlyFlagged: (count) => `サポート終了・終了間近のソフトウェアだけを表示（${count}件）`,
      nothing: "表示するものはありません。",
      footer: (date) =>
        `サポート期限の確認日：${date}。カタログにないソフトウェアは未判定と表示されます。`,
    },
    send: {
      title: "Sankaに送信",
      intro:
        "Sankaがリスクを一緒に確認します。Sanka MonitorがこのスキャンからExcelのレポートを作成し、送信する内容を表示します。［Sankaに送信］を押すまで、このPCからは何も送信しません。",
      signInTitle: "サインインして送信",
      signInIntro:
        "送信にはSankaのアカウントを使います。Sankaは、あなたの名前、メールアドレス、会社名とあわせてレポートを受け取ります。サインインはブラウザーで行います。Google、Microsoft、メールアドレスのいずれかでサインインするか、無料のアカウントを作成してください。スキャンにアカウントは不要です。",
      signIn: "ブラウザーでサインイン",
      waiting:
        "ブラウザーでサインインを完了してください。Sanka Monitorに自動で戻らない場合は、サインインコードをここに貼り付けてください。",
      codePlaceholder: "サインインコードを貼り付け",
      useCode: "コードでサインイン",
      signingIn: "サインイン中…",
      signedInAs: (who, company) => `${who}${company ? `（${company}）` : ""}としてサインイン中`,
      signOut: "サインアウト",
      whatIsSent: "Sankaが受け取るのは、次の内容と、アプリのバージョン、スキャンの場合はレポートのID、入力した場合はメモだけです。",
      device: "デバイス",
      risks: "リスク",
      riskCounts: (high, medium, low) => `高 ${high}・中 ${medium}・低 ${low}`,
      programs: "ソフトウェア",
      programCounts: (past, total) => `${total}件中 ${past}件がサポート終了`,
      saas: "SaaS",
      saasCounts: (found, notAllowed, browserCheck) =>
        `${found}件（未許可 ${notAllowed}件）` +
        (browserCheck
          ? "・ブラウザーの確認による、SaaSごとの最終利用日と直近30日の利用日数を含みます"
          : ""),
      scanned: "スキャン日時",
      language: "言語",
      file: "ファイル",
      size: "サイズ",
      fingerprint: "SHA-256",
      edited: "編集したブックです。保存した内容のまま送信し、件数は送りません。",
      explain:
        "このPCのそれ以外の情報は送信しません。レポートを開けるのはSankaの社員だけで、ご依頼いただいたアセスメントのためだけに使い、他社への営業には使いません。",
      note: "メモ（任意）",
      send: "Sankaに送信",
      sending: "送信中…",
      tryAgain: "もう一度送信",
      cancel: "キャンセル",
      sendEdited: "編集したExcelを送る…",
      useScan: "このスキャンのレポートを送る",
      emailInstead:
        "サインインしたくない場合は、下でExcelを保存し、Sankaの担当者にメールで送ってください。",
      received: (fileName, size, at) => `Sankaが ${fileName} を受け取りました（${size}、${at}）。`,
      matches: "Sankaが記録したフィンガープリントは、表示したレポートと完全に一致しています。",
      mismatch: (sha256) =>
        `Sankaが記録したフィンガープリント（${sha256}）が、表示したレポートと一致しません。もう一度送信するか、保存したExcelをSankaの担当者にメールで送ってください。`,
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
        "このPCはワークスペースに接続しています。チェックインでセキュリティチェックとインストール済みのソフトウェアを送信し、アプリの利用データと使ったSaaSは、それぞれ共有をオンにした場合だけ送信します。チェックイン、アップロード、サインイン、送信したレポートは、下に送信したとおり表示します。",
      leavesLocal:
        "Sankaにレポートを送信しない限り、ありません。スキャン結果とレポートはこのPCに残ります。送信のためのサインインはブラウザーで行い、アプリがSankaに送るのは、ワンタイムのサインインコード、アプリのバージョン、MacかWindowsかの区別だけです。自動で行う通信は下のアップデートの確認だけで、最新のバージョン番号を取得し、このPCの情報は送信しません。",
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
        "ブラウザーの確認をオンにした場合のみ：使っているビジネスSaaSを見つけるための、このPCのブラウザーの閲覧履歴（「ブラウザーの確認」を参照）",
      ],
      neverTitle: "収集しない情報",
      never: [
        "ウィンドウタイトル、ドキュメント名",
        "URL、ページのタイトル、閲覧履歴（ブラウザーの確認をオンにしても、保存、レポートへの記載、送信はしません）",
        "画面の内容、スクリーンショット、録画",
        "キー入力、クリップボードの内容",
        "ソフトウェアのインストール先、このPC内のファイル",
        "アプリの利用時間",
      ],
      sentTitle: "このアプリが送信した内容",
      nothingSent: "まだ何も送信していません。",
      deliveryUnconfirmed: " · 配信未確認",
    },
    saas: {
      intro:
        "このPCで使っているSaaSです。インストール済みのデスクトップアプリ（許可は不要です）と、オンにした場合はブラウザーの確認から見つけます。データはこのPCの外に出ません。",
      empty: "先にこのPCをスキャンしてください。",
      browserTitle: "ブラウザーの確認",
      browserCheck: "このPCのブラウザーの閲覧履歴から、使っているSaaSを確認する",
      browserExplain:
        "オンにしない限り行いません。オンにすると、スキャンのたびにこのPCのChrome、Edge、Firefoxの閲覧履歴を読み取ります。Safariは、このMacでSanka Monitorにフルディスクアクセスがすでに許可されている場合だけ読み取ります（アプリから許可を求めることはありません）。サイト名をSanka MonitorのビジネスSaaSの一覧と照合し、見つかったSaaSごとに、最後に使った日と、直近30日のうち使った日数だけを残します。URL、ページのタイトル、その他の閲覧履歴は保存せず、レポートにも書き込まず、このPCの外に出しません。保存したレポートには、SaaSの一覧とこの2つの値が含まれます。SaaSの一覧がこのPCの外に出るのは、送信したレポートに含まれる場合と、接続中のPCで下の共有をオンにした場合だけです。オフにすると、結果と共有した内容を削除します。",
      shareCheck: "このPCで直近30日に使ったSaaSを、ワークスペースと共有する",
      shareExplain:
        "初期設定はオフです。オンにすると、スキャンのたびに、ブラウザーの確認で見つかった直近30日のSaaSと、それぞれを最後に使った日をワークスペースに送信します。URL、ページのタイトル、閲覧履歴は送信しません。ワークスペースの管理者に表示されるのは、SaaSごとに使ったPCの台数だけで、どのPCかは表示されません。オフにすると共有を止め、共有した内容を削除します。送信した内容は、［プライバシー］にそのまま表示します。",
      shareHeld:
        "共有は保留中です。ワークスペースがSanka Monitorを一時停止しているか、このアプリの更新が必要です。それまでは何も送信しません。",
      thisScan: (summary) => `このスキャン：${summary}`,
      safariHint:
        "macOSでは、フルディスクアクセスを許可したアプリだけがSafariの閲覧履歴を読み取れます。この許可はMac内のすべてのファイルを開けるようにするため、Sanka Monitorからは求めません。",
      allowlistTitle: "会社の許可リスト",
      allowlistExplain:
        "IT部門は、会社が許可するSaaSをファイルにまとめ、デバイス管理でこのPCに配布するか、ここで選択できます。一覧にないSaaSを使っている場合は、リスクとして表示します。",
      machinePath: (path) => `デバイス管理で配布する場合の場所：${path}`,
      machineInUse: "デバイス管理で配布された会社のファイルを使用しています。",
      workspaceInUse:
        "ワークスペースのSanka Monitorコンソールで設定された会社のリストを使用しています。",
      chooseFile: "ファイルを選択…",
      stopUsing: "このファイルの使用をやめる",
      allowlistError: (error) => `許可リストを読み取れないため、SaaSを判定していません：${error}`,
      listTitle: "見つかったSaaS",
      none: "このPCでは、Sanka MonitorのカタログにあるSaaSは見つかりませんでした。",
      onlyNotAllowed: (count) => `許可されていないSaaSのみ（${count}件）`,
      migrationPage: "Sankaの移行ページ",
      footer: (date) =>
        `SaaSカタログの確認日：${date}。カタログにあるビジネスSaaSだけを照合し、カタログにないサイトは記録しません。`,
    },
    checks: {
      title: "定期チェック",
      regular:
        "このPCを定期的にスキャンし、会社が許可していないSaaSや新しいリスクが見つかったら、Sanka Monitorで知らせる",
      regularHint:
        "オンにすると、Sanka Monitorはログイン時に起動し、メニューバー（Windowsでは通知領域）に常駐します。何も送信しません。",
      daily: "毎日",
      weekly: "毎週",
      last: (at) => `前回の定期チェック：${at}`,
      nudgesTitle: "前回のスキャン以降の新しいリスク",
      dismiss: "閉じる",
    },
  },
};

/** Each language's own name, for the report language choice. */
export const LANGUAGE_NAMES: Record<Lang, string> = { en: "English", ja: "日本語" };

/**
 * The OS language, which the app follows. A language without a translation
 * falls back to English, so adding one is a matter of adding its strings.
 */
export function systemLang(): Lang {
  const supported = Object.keys(STRINGS) as Lang[];
  const tags = navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const tag of tags) {
    const primary = tag.toLowerCase().split("-")[0];
    const match = supported.find((code) => code === primary);
    if (match) return match;
  }
  return "en";
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
