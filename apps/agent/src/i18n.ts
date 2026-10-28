import type { EnrollmentMethod, Lang, Theme } from "./types";

// The app's words in each language. One line per row; longer explanations
// live in "Why this matters", the sheets and the Privacy page's details.

export type Strings = {
  appName: string;
  appWord: string;
  loading: string;
  never: string;
  cancel: string;
  open: string;
  dismiss: string;
  nav: {
    overview: string;
    risks: string;
    checks: string;
    inventory: string;
    monitor: string;
    software: string;
    saas: string;
    reports: string;
    workspace: string;
    privacy: string;
    settings: string;
  };
  foot: {
    background: string;
    updateReady: (version: string) => string;
    updateRequired: string;
    restart: string;
    update: string;
  };
  banners: {
    updateReady: (version: string) => string;
    updateReadyHint: string;
    restartNow: string;
    updateRequired: (version: string | null) => string;
    updateNow: string;
    removed: (at: string) => string;
    details: string;
    paused: string;
    pending: (workspace: string) => string;
    justUpdated: (version: string) => string;
    newSince: string;
  };
  overview: {
    scanning: string;
    scanningHint: string;
    notScanned: string;
    verdict: (risks: number) => string;
    summary: (passed: number, checks: number, past: number, programs: number) => string;
    scanAgain: string;
    scanningButton: string;
    lastScan: string;
    risks: string;
    riskCounts: (high: number, medium: number, low: number) => string;
    programs: string;
    programCounts: (programs: number, past: number) => string;
    device: string;
    os: string;
    osSupport: string;
    notAssessed: string;
    workspace: string;
    notConnected: string;
    connect: string;
    managed: string;
    removed: string;
    version: string;
    needsAttention: string;
    noRisks: string;
    fromWorkspace: string;
    today: string;
  };
  risks: {
    none: string;
    fix: (fix: string) => string;
  };
  check: {
    lastCheck: string;
    status: string;
    passing: string;
    failing: string;
    unknown: string;
    attention: string;
    rerun: string;
    checking: string;
    found: string;
    why: string;
    howToFix: string;
    ack: {
      title: string;
      button: string;
      adminDue: (due: string) => string;
      adminFailing: string;
      adminAccepted: (at: string, until: string) => string;
      mdmOptional: string;
      mdmAccepted: (at: string) => string;
    };
  };
  software: {
    search: (count: number) => string;
    all: (count: number) => string;
    flagged: (count: number) => string;
    nothing: string;
    nothingFlagged: string;
    supportEnded: (date: string) => string;
    supportEnds: (date: string) => string;
    replaceWith: (successor: string) => string;
    footer: (date: string) => string;
    empty: string;
  };
  saas: {
    browserCheck: string;
    browserHint: string;
    browserDetails: string;
    thisScan: (summary: string) => string;
    safariHint: string;
    allowlist: string;
    allowlistNone: string;
    machineInUse: string;
    workspaceInUse: string;
    chooseFile: string;
    stopUsing: string;
    allowlistError: (error: string) => string;
    machinePath: (path: string) => string;
    share: (workspace: string) => string;
    shareHint: string;
    shareHeld: string;
    all: (count: number) => string;
    notAllowed: (count: number) => string;
    noneNotAllowed: string;
    none: string;
    migration: string;
    footer: (date: string) => string;
    empty: string;
    ai: string;
  };
  reports: {
    send: string;
    sendThis: string;
    sendHint: string;
    signedInAs: (who: string, company: string | null) => string;
    sendButton: string;
    save: string;
    language: string;
    formats: Record<"xlsx" | "html" | "csv", { label: string; hint: string }>;
    saveButton: string;
    saving: string;
    saved: string;
    showInFolder: string;
    emailInstead: string;
    noScan: string;
  };
  send: {
    title: string;
    signInTitle: string;
    signInIntro: string;
    signIn: string;
    waiting: string;
    codePlaceholder: string;
    useCode: string;
    signingIn: string;
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
    sendEdited: string;
    useScan: string;
    received: string;
    receivedDetail: (fileName: string, size: string, at: string) => string;
    matches: string;
    mismatch: (sha256: string) => string;
    receipt: (id: string) => string;
    done: string;
  };
  workspace: {
    status: string;
    notConnected: string;
    connected: string;
    pending: string;
    managedBy: (workspace: string) => string;
    workspace: string;
    owner: string;
    lastCheckin: string;
    checkinNow: string;
    programsSent: string;
    softwareSent: (count: number, at: string) => string;
    notSent: string;
    setUpBy: string;
    methods: Record<EnrollmentMethod, string>;
    removedAt: string;
    removedNote: string;
    revokePending: (error: string | null) => string;
    connectTitle: string;
    connectHint: string;
    signIn: string;
    signInHint: string;
    signInButton: string;
    signingIn: string;
    code: string;
    codeHint: string;
    codePlaceholder: string;
    enterCode: string;
    connect: string;
    connecting: string;
    shareUsage: string;
    shareUsageHint: string;
    usage: string;
    usageToday: string;
    noneToday: string;
    pausedShort: string;
    sharedFrom: (at: string, pending: number) => string;
    disconnect: string;
    disconnectHint: string;
    managedNote: string;
    lastError: (error: string) => string;
    paused: string;
    thisWorkspace: string;
  };
  join: {
    title: (workspace: string) => string;
    invitedBy: (who: string) => string;
    forEmail: (email: string) => string;
    shared: string;
    sharedValue: string;
    notShared: string;
    notSharedValue: string;
    usage: string;
    usageValue: string;
    shareUsage: string;
    connect: string;
    connecting: string;
    loading: string;
    invalid: string;
  };
  privacy: {
    leaves: string;
    leavesNothing: string;
    leavesConnected: (workspace: string) => string;
    appUsage: string;
    shared: string;
    notShared: string;
    saas: string;
    updateChecks: string;
    updateHint: (at: string) => string;
    checksOff: string;
    managedOff: string;
    lastFailed: (error: string) => string;
    sent: string;
    nothingSent: string;
    unconfirmed: string;
    details: string;
    leavesLocalLong: string;
    leavesConnectedLong: string;
    readsTitle: string;
    readsShort: string;
    reads: string[];
    neverTitle: string;
    neverShort: string;
    never: string[];
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
  };
  settings: {
    appearance: string;
    themes: Record<Theme, string>;
    regular: string;
    regularHint: string;
    off: string;
    daily: string;
    weekly: string;
    lastRegular: (at: string) => string;
    launchAtLogin: string;
    launchHint: string;
    reportLanguage: string;
    version: (version: string) => string;
    upToDate: (at: string) => string;
    downloaded: (version: string) => string;
    checkNow: string;
    checking: string;
    restartToUpdate: string;
  };
  first: {
    title: string;
    body: string;
    nothingLeaves: string;
    steps: string[];
    seeResults: string;
    connect: string;
  };
};

export const STRINGS: Record<Lang, Strings> = {
  en: {
    appName: "Sanka Monitor",
    appWord: "Monitor",
    loading: "Loading…",
    never: "never",
    cancel: "Cancel",
    open: "Open",
    dismiss: "Dismiss",
    nav: {
      overview: "Overview",
      risks: "Risks",
      checks: "Security checks",
      inventory: "Inventory",
      monitor: "Sanka Monitor",
      software: "Software",
      saas: "SaaS",
      reports: "Reports",
      workspace: "Workspace",
      privacy: "Privacy",
      settings: "Settings",
    },
    foot: {
      background: "Running in the background",
      updateReady: (version) => `Update ready (${version})`,
      updateRequired: "Update required",
      restart: "Restart",
      update: "Update",
    },
    banners: {
      updateReady: (version) => `Sanka Monitor ${version} is ready.`,
      updateReadyHint: "It installs when the app restarts.",
      restartNow: "Restart now",
      updateRequired: (version) =>
        `Update required. Your workspace needs version ${version ?? "a newer version"} or later. Sharing waits until then.`,
      updateNow: "Update now",
      removed: (at) => `Your workspace removed this computer on ${at}. Scans keep working here.`,
      details: "Details",
      paused:
        "Your workspace paused Sanka Monitor. Nothing is collected or shared until it's turned back on.",
      pending: (workspace) => `Connected to ${workspace}. Waiting for an admin to approve this computer.`,
      justUpdated: (version) => `Updated to Sanka Monitor ${version}.`,
      newSince: "New since the last scan",
    },
    overview: {
      scanning: "Scanning this computer…",
      scanningHint: "This takes a few seconds.",
      notScanned: "Not scanned yet",
      verdict: (risks) =>
        risks === 0 ? "No risks found" : risks === 1 ? "1 risk to review" : `${risks} risks to review`,
      summary: (passed, checks, past, programs) =>
        `${passed} of ${checks} checks passing · ${past} of ${programs} programs past support`,
      scanAgain: "Scan again",
      scanningButton: "Scanning…",
      lastScan: "Last scan",
      risks: "Risks",
      riskCounts: (high, medium, low) => `${high} high · ${medium} medium · ${low} low`,
      programs: "Programs",
      programCounts: (programs, past) => `${programs} installed · ${past} past support`,
      device: "Device",
      os: "OS version",
      osSupport: "Support not assessed",
      notAssessed: "Not assessed",
      workspace: "Workspace",
      notConnected: "Not connected",
      connect: "Connect…",
      managed: "Managed",
      removed: "Removed by your workspace",
      version: "Version",
      needsAttention: "Needs attention",
      noRisks:
        "No risks found. Risks come from vendor support dates and this computer's settings, so no risks doesn't prove it's safe.",
      fromWorkspace: "From your workspace",
      today: "Today",
    },
    risks: {
      none: "No risks found in the last scan.",
      fix: (fix) => `Fix: ${fix}`,
    },
    check: {
      lastCheck: "Last check",
      status: "Status",
      passing: "Passing",
      failing: "Failing",
      unknown: "Can't tell",
      attention: "Needs attention",
      rerun: "Re-run check",
      checking: "Checking…",
      found: "Found",
      why: "Why this matters",
      howToFix: "How to fix",
      ack: {
        title: "Your choice",
        button: "Acknowledge risk",
        adminDue: (due) =>
          `You can accept the risk of working as an administrator, a week at a time. Acknowledge it by ${due}, or this check fails.`,
        adminFailing:
          "Acknowledge the risk to keep working as an administrator for another week, or switch to a standard account.",
        adminAccepted: (at, until) =>
          `You accepted this risk on ${at}. It needs your acknowledgement again on ${until}.`,
        mdmOptional:
          "Device management is optional. If this computer doesn't need it, acknowledge the risk and this check shows as needing attention instead of failing.",
        mdmAccepted: (at) => `You acknowledged on ${at} that this computer isn't managed.`,
      },
    },
    software: {
      search: (count) => `Search ${count} programs`,
      all: (count) => `All ${count}`,
      flagged: (count) => `Past or near end of support · ${count}`,
      nothing: "No programs match.",
      nothingFlagged: "No programs past or near the end of support.",
      supportEnded: (date) => `Support ended ${date}`,
      supportEnds: (date) => `Support ends ${date}`,
      replaceWith: (successor) => `Replace with ${successor}`,
      footer: (date) =>
        `Support dates from the catalog bundled in the app, checked ${date}. Programs it doesn't know show as not assessed.`,
      empty: "Scan this computer to list its programs.",
    },
    saas: {
      browserCheck: "Browser check",
      browserHint: "Finds web apps in Chrome, Edge and Firefox history. Keeps only the last day used.",
      browserDetails:
        "Off unless you turn it on. When it's on, each scan reads the history of Chrome, Edge and Firefox on this computer, and Safari's only if this Mac already gives Sanka Monitor Full Disk Access (the app never asks for it). It matches each site's name against Sanka Monitor's list of business SaaS. For each SaaS it finds, it keeps only the last day you used it and on how many of the last 30 days. Web addresses, page titles and the rest of your history are not stored, are never written to a report and never leave this computer. Turning it off deletes the results, and what you shared.",
      thisScan: (summary) => `This scan: ${summary}`,
      safariHint:
        "macOS lets an app read Safari's history only with Full Disk Access. Sanka Monitor doesn't ask for it, because that permission opens every file on this Mac.",
      allowlist: "Allowlist",
      allowlistNone: "None. Choose your company's file to mark SaaS as allowed or not.",
      machineInUse: "Your company's file, placed by device management",
      workspaceInUse: "Your workspace's list, updated with each check-in",
      chooseFile: "Choose file…",
      stopUsing: "Stop using",
      allowlistError: (error) => `The allowlist couldn't be read, so no SaaS is judged: ${error}`,
      machinePath: (path) => `Device management places the company's file at ${path}.`,
      share: (workspace) => `Share with ${workspace}`,
      shareHint: "SaaS used in the last 30 days and the last day used. Never web addresses.",
      shareHeld: "On hold: your workspace paused Sanka Monitor, or this app needs an update.",
      all: (count) => `All ${count}`,
      notAllowed: (count) => `Not allowed · ${count}`,
      noneNotAllowed: "No SaaS outside the allowlist.",
      none: "No SaaS from Sanka Monitor's catalog was found on this computer.",
      migration: "Sanka migration page",
      footer: (date) =>
        `SaaS catalog checked ${date}. Only business SaaS in the catalog are matched; a site it doesn't list is never recorded.`,
      empty: "Scan this computer first.",
      ai: "AI",
    },
    reports: {
      send: "Send to Sanka",
      sendThis: "Send this scan",
      sendHint: "Sign in once in your browser. You see what's sent first.",
      signedInAs: (who, company) => `Signed in as ${who}${company ? ` · ${company}` : ""}`,
      sendButton: "Send to Sanka…",
      save: "Save a report",
      language: "Report language",
      formats: {
        xlsx: { label: "IT asset ledger", hint: "Excel (.xlsx): devices, software, SaaS, risks and checks" },
        html: { label: "One-page risk report", hint: "HTML: opens in a browser, prints on one A4 page" },
        csv: { label: "CSV", hint: "One row per record, for merging" },
      },
      saveButton: "Save…",
      saving: "Saving…",
      saved: "Saved on this computer",
      showInFolder: "Show in folder",
      emailInstead: "Prefer not to sign in? Save the Excel and email it to your Sanka contact.",
      noScan: "Scan this computer first.",
    },
    send: {
      title: "Send this scan to Sanka",
      signInTitle: "Sign in first",
      signInIntro:
        "Sending uses your Sanka account, so Sanka receives the report with your name, email address and company. Your browser opens; use Google, Microsoft or email, or create a free account.",
      signIn: "Sign in…",
      waiting: "Finish signing in in your browser. If the app doesn't come back by itself, paste the sign-in code.",
      codePlaceholder: "Paste sign-in code",
      useCode: "Sign in with code",
      signingIn: "Signing in…",
      signOut: "Sign out",
      whatIsSent: "This is what Sanka receives, with the app version and the report ID:",
      device: "Device",
      risks: "Risks",
      riskCounts: (high, medium, low) => `${high} high · ${medium} medium · ${low} low`,
      programs: "Programs",
      programCounts: (past, total) => `${past} of ${total} past support`,
      saas: "SaaS",
      saasCounts: (found, notAllowed, browserCheck) =>
        `${found} found (${notAllowed} not allowed)` + (browserCheck ? " · with last day used (browser check)" : ""),
      scanned: "Scanned",
      language: "Language",
      file: "File",
      size: "Size",
      fingerprint: "SHA-256",
      edited: "An edited workbook: sent byte for byte as you saved it, without counts.",
      explain:
        "Nothing else from this computer is sent. Only Sanka staff can open the report, and it's used for the assessment you ask for, never for outreach to other companies.",
      note: "Note (optional)",
      send: "Send",
      sending: "Sending…",
      tryAgain: "Try again",
      sendEdited: "Send an edited workbook…",
      useScan: "Send this scan instead",
      received: "Received by Sanka",
      receivedDetail: (fileName, size, at) => `${fileName} · ${size} · ${at}`,
      matches: "The SHA-256 matches what this computer sent.",
      mismatch: (sha256) =>
        `Sanka's SHA-256 (${sha256}) doesn't match the report shown. Send it again, or email the saved Excel to your Sanka contact.`,
      receipt: (id) => `Receipt ${id}`,
      done: "Done",
    },
    workspace: {
      status: "Status",
      notConnected: "Not connected",
      connected: "Connected",
      pending: "Waiting for approval",
      managedBy: (workspace) => `Managed by ${workspace} IT`,
      workspace: "Workspace",
      owner: "Owner",
      lastCheckin: "Last check-in",
      checkinNow: "Check in now",
      programsSent: "Programs sent",
      softwareSent: (count, at) => `${count} · ${at}`,
      notSent: "Not sent yet",
      setUpBy: "Set up by",
      methods: {
        code: "Enrollment code",
        invite: "Invite link",
        deployment_token: "Device management",
        sign_in: "Sign-in",
      },
      removedAt: "Removed",
      removedNote:
        "Your workspace removed this computer, so Sanka Monitor deleted its usage history and credential here. Scans keep working.",
      revokePending: (error) =>
        `Disconnected, but your workspace hasn't confirmed yet. Sanka Monitor keeps only the credential and tries again every hour and at login.${error ? ` Last attempt: ${error}` : ""}`,
      connectTitle: "Connect to your company",
      connectHint:
        "Only if your company uses Sanka Monitor. Connected computers send their security checks and installed programs. Invite links open here by themselves.",
      signIn: "Sign in with your company",
      signInHint: "Opens your browser to choose your workspace",
      signInButton: "Sign in…",
      signingIn: "Waiting for the browser…",
      code: "Enrollment code",
      codeHint: "From your IT admin",
      codePlaceholder: "Paste enrollment code",
      enterCode: "Enter code…",
      connect: "Connect",
      connecting: "Connecting…",
      shareUsage: "Share which apps I use each day",
      shareUsageHint: "Off by default. Days only, never durations. Turn it off any time.",
      usage: "App usage",
      usageToday: "Used today",
      noneToday: "None recorded yet today",
      pausedShort: "Paused by your workspace",
      sharedFrom: (at, pending) => `Shared since ${at} · ${pending} waiting to upload`,
      disconnect: "Disconnect…",
      disconnectHint: "Removes this computer from your workspace and deletes its usage history here.",
      managedNote: "Your company manages this connection, so it can't be disconnected here.",
      lastError: (error) => `Last check-in failed: ${error}`,
      paused:
        "Paused by your workspace. Check-ins still send this computer's name, model, serial number and OS version, to learn when it's turned back on.",
      thisWorkspace: "your workspace",
    },
    join: {
      title: (workspace) => `Connect to ${workspace}?`,
      invitedBy: (who) => `Invited by ${who}`,
      forEmail: (email) => `Invite for ${email}`,
      shared: "Shared",
      sharedValue: "Security checks and installed programs",
      notShared: "Not shared",
      notSharedValue: "Files, browsing, screen, keystrokes",
      usage: "App usage",
      usageValue: "Only if you turn it on",
      shareUsage: "Also share which apps I use each day (days only, never durations)",
      connect: "Connect",
      connecting: "Connecting…",
      loading: "Checking the invite…",
      invalid: "This invite link doesn't work any more. Ask your IT admin for a new one.",
    },
    privacy: {
      leaves: "Leaves this computer",
      leavesNothing: "Nothing",
      leavesConnected: (workspace) => `Security checks and installed programs, to ${workspace}`,
      appUsage: "App usage",
      shared: "Shared",
      notShared: "Not shared",
      saas: "SaaS",
      updateChecks: "Check for updates every hour",
      updateHint: (at) => `Sends nothing about this computer · last checked ${at}`,
      checksOff: "Off: install new versions yourself",
      managedOff: "Turned off by your company's device management",
      lastFailed: (error) => `Last check failed: ${error}`,
      sent: "Sent from this computer",
      nothingSent: "Nothing sent yet.",
      unconfirmed: "delivery unconfirmed",
      details: "Details",
      leavesLocalLong:
        "Nothing, unless you sign in to send reports to Sanka. Scans and reports stay on this computer. To finish signing in, the app sends Sanka only a one-time sign-in code with its verifier, its version and whether this is a Mac or a Windows PC. Sending a report and signing out use a one-hour token, which the app gets in exchange for its credential (a device ID and secret) and its version. Signing out revokes the credential in Sanka, and a cancelled or replaced sign-in is revoked the same way. These requests and the reports you send are listed here, exactly as sent, with the code, verifier and secret hidden. The only automatic network request is the update check, which downloads the latest version number from release.sanka.com and sends nothing about this computer.",
      leavesConnectedLong:
        "This computer is connected to a workspace: check-ins send its security checks and installed programs, and app usage and the SaaS it used only if you turned that sharing on. Check-ins and uploads are listed here, exactly as sent, and so are your Sanka account's sign-in, token requests, sent reports and sign-out, with the sign-in code, its verifier and the device secret hidden. The token request that comes first each time the app contacts your workspace is not listed.",
      readsTitle: "What Sanka Monitor reads",
      readsShort: "Security settings and installed apps. Browser history only when the browser check is on.",
      reads: [
        "Security settings: disk encryption, firewall, Gatekeeper or SmartScreen, screen lock, OS updates, automatic updates, antivirus, admin rights, device management, Secure Boot or System Integrity Protection",
        "Installed apps and programs: name, version and publisher",
        "Device identity: model, OS version and edition, computer name, serial number",
        "Only if you turn on the browser check: your browsers' history on this computer, to find the business SaaS you use",
      ],
      neverTitle: "What it never collects",
      neverShort: "Files, keystrokes, the screen, window titles and web addresses.",
      never: [
        "Window titles or document names",
        "Web addresses, page titles or browsing history: even with the browser check on, they are never stored, written to a report or sent",
        "Screen contents, screenshots, or recordings",
        "Keystrokes or clipboard contents",
        "Where programs are installed, or the files on this computer",
        "How long any app was used",
      ],
      viewsTitle: "Who viewed your app usage",
      viewsIntro:
        "When an IT admin opens your app usage (which apps, on which days), Sanka records when, who, the days and the reason they gave. Loaded from your workspace when you open this page.",
      viewsEmpty: "No one has viewed it.",
      viewerUnnamed: "An IT admin",
      viewedDay: (day) => `Day viewed: ${day}`,
      viewedDays: (from, to) => `Days viewed: ${from} – ${to}`,
      viewReason: (reason) => `Reason: ${reason}`,
      viewsShown: (shown, total) => `Showing the latest ${shown} of ${total}.`,
      viewsOffline: "Couldn't reach your workspace. Check your network and try again.",
      viewsError: (error) => `Couldn't load this list: ${error}`,
      viewsRetry: "Try again",
    },
    settings: {
      appearance: "Appearance",
      themes: { system: "System", light: "Light", dark: "Dark" },
      regular: "Regular checks",
      regularHint: "Scan again and tell me when something new appears. Nothing is sent.",
      off: "Off",
      daily: "Daily",
      weekly: "Weekly",
      lastRegular: (at) => `Last regular check: ${at}`,
      launchAtLogin: "Open at login",
      launchHint: "Keeps check-ins running in the menu bar",
      reportLanguage: "Report language",
      version: (version) => `Sanka Monitor ${version}`,
      upToDate: (at) => `Up to date · checked ${at}`,
      downloaded: (version) => `Version ${version} is downloaded`,
      checkNow: "Check for updates",
      checking: "Checking…",
      restartToUpdate: "Restart to update",
    },
    first: {
      title: "Check this computer for IT risks",
      body: "Sanka Monitor reads this computer's security settings and programs and checks them against vendor support dates.",
      nothingLeaves: "Nothing leaves this computer unless you send a report or your company connects it.",
      steps: ["Security settings", "Installed programs", "Vendor support dates", "SaaS from desktop apps"],
      seeResults: "See results",
      connect: "My company uses Sanka Monitor",
    },
  },
  ja: {
    appName: "Sanka Monitor",
    appWord: "Monitor",
    loading: "読み込み中…",
    never: "なし",
    cancel: "キャンセル",
    open: "開く",
    dismiss: "閉じる",
    nav: {
      overview: "概要",
      risks: "リスク",
      checks: "セキュリティチェック",
      inventory: "インベントリ",
      monitor: "Sanka Monitor",
      software: "ソフトウェア",
      saas: "SaaS",
      reports: "レポート",
      workspace: "ワークスペース",
      privacy: "プライバシー",
      settings: "設定",
    },
    foot: {
      background: "バックグラウンドで実行中",
      updateReady: (version) => `アップデートの準備完了（${version}）`,
      updateRequired: "アップデートが必要です",
      restart: "再起動",
      update: "更新",
    },
    banners: {
      updateReady: (version) => `Sanka Monitor ${version} の準備ができました。`,
      updateReadyHint: "アプリを再起動するとインストールされます。",
      restartNow: "今すぐ再起動",
      updateRequired: (version) =>
        `アップデートが必要です。ワークスペースはバージョン ${version ?? "の新しいバージョン"}以降を求めています。それまで共有を停止します。`,
      updateNow: "今すぐ更新",
      removed: (at) => `ワークスペースが ${at} にこのPCを削除しました。スキャンは引き続き使えます。`,
      details: "詳細",
      paused: "ワークスペースがSanka Monitorを一時停止しています。再開されるまで、何も収集・共有しません。",
      pending: (workspace) => `${workspace}に接続しました。管理者の承認を待っています。`,
      justUpdated: (version) => `Sanka Monitor ${version} にアップデートしました。`,
      newSince: "前回のスキャン以降の新しいリスク",
    },
    overview: {
      scanning: "このPCをスキャンしています…",
      scanningHint: "数秒かかります。",
      notScanned: "まだスキャンしていません",
      verdict: (risks) => (risks === 0 ? "リスクは見つかりませんでした" : `確認が必要なリスク：${risks}件`),
      summary: (passed, checks, past, programs) =>
        `チェック ${checks}件中 ${passed}件が合格 · ソフトウェア ${programs}件中 ${past}件がサポート終了`,
      scanAgain: "もう一度スキャン",
      scanningButton: "スキャン中…",
      lastScan: "前回のスキャン",
      risks: "リスク",
      riskCounts: (high, medium, low) => `高 ${high}・中 ${medium}・低 ${low}`,
      programs: "ソフトウェア",
      programCounts: (programs, past) => `${programs}件 · サポート終了 ${past}件`,
      device: "デバイス",
      os: "OSのバージョン",
      osSupport: "サポート状況は未判定",
      notAssessed: "未判定",
      workspace: "ワークスペース",
      notConnected: "未接続",
      connect: "接続…",
      managed: "管理対象",
      removed: "ワークスペースにより削除",
      version: "バージョン",
      needsAttention: "対応が必要",
      noRisks:
        "リスクは見つかりませんでした。リスクは提供元のサポート期限とこのPCの設定から判定しています。リスクが表示されなくても、PCが安全とは限りません。",
      fromWorkspace: "ワークスペースからのお知らせ",
      today: "今日",
    },
    risks: {
      none: "前回のスキャンでリスクは見つかりませんでした。",
      fix: (fix) => `対応方法：${fix}`,
    },
    check: {
      lastCheck: "前回のチェック",
      status: "状態",
      passing: "合格",
      failing: "不合格",
      unknown: "判定できません",
      attention: "要対応",
      rerun: "もう一度チェック",
      checking: "チェック中…",
      found: "検出内容",
      why: "このチェックが必要な理由",
      howToFix: "対応方法",
      ack: {
        title: "あなたの判断",
        button: "リスクを承認する",
        adminDue: (due) =>
          `管理者アカウントで作業するリスクは、1週間ずつ承認できます。${due}までに承認しないと、このチェックは不合格になります。`,
        adminFailing:
          "管理者アカウントでの作業を続ける場合は、リスクを承認してください（1週間有効）。または標準アカウントに切り替えてください。",
        adminAccepted: (at, until) => `${at}にこのリスクを承認しました。${until}に再度の承認が必要です。`,
        mdmOptional:
          "デバイス管理（MDM）は任意です。このコンピューターに必要ない場合はリスクを承認すると、このチェックは不合格ではなく要対応と表示されます。",
        mdmAccepted: (at) => `${at}に、このコンピューターがデバイス管理されていないことを承認しました。`,
      },
    },
    software: {
      search: (count) => `${count}件のソフトウェアを検索`,
      all: (count) => `すべて ${count}`,
      flagged: (count) => `サポート終了・終了間近 · ${count}`,
      nothing: "一致するソフトウェアはありません。",
      nothingFlagged: "サポート終了・終了間近のソフトウェアはありません。",
      supportEnded: (date) => `${date}にサポート終了`,
      supportEnds: (date) => `${date}にサポート終了予定`,
      replaceWith: (successor) => `移行先：${successor}`,
      footer: (date) =>
        `サポート期限はアプリに同梱したカタログによるもので、確認日は${date}です。カタログにないソフトウェアは未判定と表示されます。`,
      empty: "このPCをスキャンすると、ソフトウェアが一覧表示されます。",
    },
    saas: {
      browserCheck: "ブラウザーの確認",
      browserHint: "Chrome、Edge、Firefoxの閲覧履歴からWebアプリを見つけます。残すのは最終利用日だけです。",
      browserDetails:
        "オンにしない限り行いません。オンにすると、スキャンのたびにこのPCのChrome、Edge、Firefoxの閲覧履歴を読み取ります。Safariは、このMacでSanka Monitorにフルディスクアクセスがすでに許可されている場合だけ読み取ります（アプリから許可を求めることはありません）。サイト名をSanka MonitorのビジネスSaaSの一覧と照合し、見つかったSaaSごとに、最後に使った日と、直近30日のうち使った日数だけを残します。URL、ページのタイトル、その他の閲覧履歴は保存せず、レポートにも書き込まず、このPCの外に出しません。オフにすると、結果と共有した内容を削除します。",
      thisScan: (summary) => `このスキャン：${summary}`,
      safariHint:
        "macOSでは、フルディスクアクセスを許可したアプリだけがSafariの閲覧履歴を読み取れます。この許可はMac内のすべてのファイルを開けるようにするため、Sanka Monitorからは求めません。",
      allowlist: "許可リスト",
      allowlistNone: "なし。会社のファイルを選ぶと、SaaSが許可されているかを判定します。",
      machineInUse: "デバイス管理で配布された会社のファイル",
      workspaceInUse: "ワークスペースのリスト（チェックインのたびに更新）",
      chooseFile: "ファイルを選択…",
      stopUsing: "使用をやめる",
      allowlistError: (error) => `許可リストを読み取れないため、SaaSを判定していません：${error}`,
      machinePath: (path) => `デバイス管理で配布する場合の場所：${path}`,
      share: (workspace) => `${workspace}と共有`,
      shareHint: "直近30日に使ったSaaSと最終利用日。URLは送信しません。",
      shareHeld: "保留中です。ワークスペースがSanka Monitorを一時停止しているか、このアプリの更新が必要です。",
      all: (count) => `すべて ${count}`,
      notAllowed: (count) => `未許可 · ${count}`,
      noneNotAllowed: "許可リストにないSaaSはありません。",
      none: "このPCでは、Sanka MonitorのカタログにあるSaaSは見つかりませんでした。",
      migration: "Sankaの移行ページ",
      footer: (date) =>
        `SaaSカタログの確認日：${date}。カタログにあるビジネスSaaSだけを照合し、カタログにないサイトは記録しません。`,
      empty: "先にこのPCをスキャンしてください。",
      ai: "AI",
    },
    reports: {
      send: "Sankaに送信",
      sendThis: "このスキャンを送信",
      sendHint: "ブラウザーで一度サインインします。送信前に内容を確認できます。",
      signedInAs: (who, company) => `${who}${company ? `（${company}）` : ""}としてサインイン中`,
      sendButton: "Sankaに送信…",
      save: "レポートを保存",
      language: "レポートの言語",
      formats: {
        xlsx: { label: "IT資産台帳", hint: "Excel（.xlsx）：デバイス、ソフトウェア、SaaS、リスク、チェック結果" },
        html: { label: "1ページのリスクレポート", hint: "HTML：ブラウザーで開き、A4 1ページで印刷できます" },
        csv: { label: "CSV", hint: "1行1レコード。複数のPCをまとめるときに使います" },
      },
      saveButton: "保存…",
      saving: "保存中…",
      saved: "このPCに保存したレポート",
      showInFolder: "フォルダーで表示",
      emailInstead: "サインインしたくない場合は、Excelを保存してSankaの担当者にメールで送ってください。",
      noScan: "先にこのPCをスキャンしてください。",
    },
    send: {
      title: "このスキャンをSankaに送信",
      signInTitle: "先にサインインしてください",
      signInIntro:
        "送信にはSankaのアカウントを使います。Sankaは、あなたの名前、メールアドレス、会社名とあわせてレポートを受け取ります。ブラウザーが開くので、Google、Microsoft、メールアドレスのいずれかでサインインするか、無料のアカウントを作成してください。",
      signIn: "サインイン…",
      waiting:
        "ブラウザーでサインインを完了してください。アプリに自動で戻らない場合は、サインインコードを貼り付けてください。",
      codePlaceholder: "サインインコードを貼り付け",
      useCode: "コードでサインイン",
      signingIn: "サインイン中…",
      signOut: "サインアウト",
      whatIsSent: "Sankaが受け取るのは、次の内容と、アプリのバージョン、レポートのIDです。",
      device: "デバイス",
      risks: "リスク",
      riskCounts: (high, medium, low) => `高 ${high}・中 ${medium}・低 ${low}`,
      programs: "ソフトウェア",
      programCounts: (past, total) => `${total}件中 ${past}件がサポート終了`,
      saas: "SaaS",
      saasCounts: (found, notAllowed, browserCheck) =>
        `${found}件（未許可 ${notAllowed}件）` + (browserCheck ? "・最終利用日を含む（ブラウザーの確認）" : ""),
      scanned: "スキャン日時",
      language: "言語",
      file: "ファイル",
      size: "サイズ",
      fingerprint: "SHA-256",
      edited: "編集したブックです。保存した内容のまま送信し、件数は送りません。",
      explain:
        "このPCのそれ以外の情報は送信しません。レポートを開けるのはSankaの社員だけで、ご依頼いただいたアセスメントのためだけに使い、他社への営業には使いません。",
      note: "メモ（任意）",
      send: "送信",
      sending: "送信中…",
      tryAgain: "もう一度送信",
      sendEdited: "編集したブックを送る…",
      useScan: "このスキャンを送る",
      received: "Sankaが受け取りました",
      receivedDetail: (fileName, size, at) => `${fileName} · ${size} · ${at}`,
      matches: "SHA-256は、このPCが送信した内容と一致しています。",
      mismatch: (sha256) =>
        `Sankaが記録したSHA-256（${sha256}）が、表示したレポートと一致しません。もう一度送信するか、保存したExcelをSankaの担当者にメールで送ってください。`,
      receipt: (id) => `受付番号 ${id}`,
      done: "閉じる",
    },
    workspace: {
      status: "状態",
      notConnected: "未接続",
      connected: "接続中",
      pending: "承認待ち",
      managedBy: (workspace) => `${workspace}のIT部門が管理`,
      workspace: "ワークスペース",
      owner: "利用者",
      lastCheckin: "最終チェックイン",
      checkinNow: "今すぐチェックイン",
      programsSent: "送信したソフトウェア",
      softwareSent: (count, at) => `${count}件 · ${at}`,
      notSent: "まだ送信していません",
      setUpBy: "接続方法",
      methods: {
        code: "登録コード",
        invite: "招待リンク",
        deployment_token: "デバイス管理",
        sign_in: "サインイン",
      },
      removedAt: "削除日時",
      removedNote:
        "ワークスペースがこのPCを削除したため、Sanka Monitorはこのコンピューター内の利用履歴と認証情報を削除しました。スキャンは引き続き使えます。",
      revokePending: (error) =>
        `接続を解除しましたが、ワークスペースの確認がまだです。Sanka Monitorは認証情報だけを残し、1時間ごととログイン時に再試行します。${error ? `前回の試行：${error}` : ""}`,
      connectTitle: "会社に接続",
      connectHint:
        "会社がSanka Monitorを使っている場合だけ接続してください。接続したPCは、セキュリティチェックとインストール済みのソフトウェアを送信します。招待リンクはこの画面で自動的に開きます。",
      signIn: "会社のアカウントでサインイン",
      signInHint: "ブラウザーが開き、ワークスペースを選べます",
      signInButton: "サインイン…",
      signingIn: "ブラウザーを待っています…",
      code: "登録コード",
      codeHint: "IT管理者から受け取ったコード",
      codePlaceholder: "登録コードを貼り付け",
      enterCode: "コードを入力…",
      connect: "接続",
      connecting: "接続中…",
      shareUsage: "日ごとに使ったアプリを共有する",
      shareUsageHint: "初期設定はオフです。共有するのは日付だけで、利用時間は共有しません。いつでもオフにできます。",
      usage: "アプリの利用",
      usageToday: "今日使ったアプリ",
      noneToday: "今日はまだ記録がありません",
      pausedShort: "ワークスペースにより一時停止中",
      sharedFrom: (at, pending) => `${at}から共有中 · アップロード待ち ${pending}件`,
      disconnect: "接続を解除…",
      disconnectHint: "ワークスペースからこのPCを削除し、PC内の利用履歴を削除します。",
      managedNote: "この接続は会社が管理しているため、ここでは解除できません。",
      lastError: (error) => `前回のチェックインに失敗しました：${error}`,
      paused:
        "ワークスペースにより一時停止中です。再開を知るため、チェックインではこのPCの名前・機種・シリアル番号・OSのバージョンだけを送信します。",
      thisWorkspace: "ワークスペース",
    },
    join: {
      title: (workspace) => `${workspace}に接続しますか？`,
      invitedBy: (who) => `招待者：${who}`,
      forEmail: (email) => `${email}への招待`,
      shared: "共有する情報",
      sharedValue: "セキュリティチェック、インストール済みのソフトウェア",
      notShared: "共有しない情報",
      notSharedValue: "ファイル、閲覧履歴、画面、キー入力",
      usage: "アプリの利用",
      usageValue: "オンにした場合だけ",
      shareUsage: "日ごとに使ったアプリも共有する（日付だけで、利用時間は共有しません）",
      connect: "接続",
      connecting: "接続中…",
      loading: "招待を確認しています…",
      invalid: "この招待リンクは使えなくなっています。IT管理者に新しいリンクを依頼してください。",
    },
    privacy: {
      leaves: "PCの外に出る情報",
      leavesNothing: "なし",
      leavesConnected: (workspace) => `セキュリティチェックとインストール済みのソフトウェアを${workspace}へ`,
      appUsage: "アプリの利用",
      shared: "共有中",
      notShared: "共有していません",
      saas: "SaaS",
      updateChecks: "1時間ごとにアップデートを確認する",
      updateHint: (at) => `このPCの情報は送信しません · 前回の確認：${at}`,
      checksOff: "オフ：新しいバージョンはご自身でインストールしてください",
      managedOff: "会社のデバイス管理によりオフになっています",
      lastFailed: (error) => `前回の確認に失敗しました：${error}`,
      sent: "このPCから送信した内容",
      nothingSent: "まだ何も送信していません。",
      unconfirmed: "配信未確認",
      details: "詳細",
      leavesLocalLong:
        "Sankaにレポートを送るためにサインインしない限り、ありません。スキャン結果とレポートはこのPCに残ります。サインインを完了するためにアプリがSankaに送るのは、ワンタイムのサインインコードとその検証用の値、アプリのバージョン、MacかWindowsかの区別だけです。レポートの送信とサインアウトには1時間有効なトークンを使い、アプリはトークンを受け取るために、認証情報（デバイスIDとシークレット）とアプリのバージョンを送ります。サインアウトでは、Sankaでその認証情報を無効にします。途中でキャンセルしたり、やり直したりしたサインインの認証情報も、同じように無効にします。これらの通信と送信したレポートは、ここに送信したとおり表示します（サインインコード、検証用の値、シークレットは伏せて表示します）。自動で行う通信はアップデートの確認だけで、release.sanka.com から最新のバージョン番号を取得し、このPCの情報は送信しません。",
      leavesConnectedLong:
        "このPCはワークスペースに接続しています。チェックインでセキュリティチェックとインストール済みのソフトウェアを送信し、アプリの利用データと使ったSaaSは、それぞれ共有をオンにした場合だけ送信します。チェックインとアップロードに加え、Sankaのアカウントで行うサインイン、トークンの要求、レポートの送信、サインアウトも、ここに送信したとおり表示します（サインインコード、検証用の値、シークレットは伏せて表示します）。ワークスペースと通信するたびに最初に行うトークンの要求は表示しません。",
      readsTitle: "Sanka Monitorが読み取る情報",
      readsShort: "セキュリティ設定とインストール済みのアプリ。閲覧履歴はブラウザーの確認がオンの場合だけです。",
      reads: [
        "セキュリティ設定：ディスク暗号化、ファイアウォール、Gatekeeper または SmartScreen、画面ロック、OSアップデート、自動アップデート、ウイルス対策、管理者権限、デバイス管理、セキュアブートまたはシステム整合性保護",
        "インストール済みのアプリとソフトウェア：名前、バージョン、提供元",
        "デバイスの情報：機種、OSのバージョンとエディション、コンピューター名、シリアル番号",
        "ブラウザーの確認をオンにした場合のみ：使っているビジネスSaaSを見つけるための、このPCのブラウザーの閲覧履歴",
      ],
      neverTitle: "収集しない情報",
      neverShort: "ファイル、キー入力、画面、ウィンドウタイトル、URL。",
      never: [
        "ウィンドウタイトル、ドキュメント名",
        "URL、ページのタイトル、閲覧履歴（ブラウザーの確認をオンにしても、保存、レポートへの記載、送信はしません）",
        "画面の内容、スクリーンショット、録画",
        "キー入力、クリップボードの内容",
        "ソフトウェアのインストール先、このPC内のファイル",
        "アプリの利用時間",
      ],
      viewsTitle: "アプリの利用データを開いた人",
      viewsIntro:
        "IT管理者があなたのアプリの利用データ（どの日にどのアプリを使ったか）を開くと、Sankaは日時、開いた人、対象の期間、入力された理由を記録します。この画面を開いたときにワークスペースから読み込みます。",
      viewsEmpty: "まだ誰も開いていません。",
      viewerUnnamed: "IT管理者",
      viewedDay: (day) => `対象の日：${day}`,
      viewedDays: (from, to) => `対象の期間：${from}〜${to}`,
      viewReason: (reason) => `理由：${reason}`,
      viewsShown: (shown, total) => `最新の${shown}件を表示しています（全${total}件）。`,
      viewsOffline: "ワークスペースに接続できませんでした。ネットワークを確認して、もう一度お試しください。",
      viewsError: (error) => `この一覧を読み込めませんでした：${error}`,
      viewsRetry: "再読み込み",
    },
    settings: {
      appearance: "表示",
      themes: { system: "システム", light: "ライト", dark: "ダーク" },
      regular: "定期チェック",
      regularHint: "定期的にスキャンし、新しいリスクが見つかったら知らせます。何も送信しません。",
      off: "オフ",
      daily: "毎日",
      weekly: "毎週",
      lastRegular: (at) => `前回の定期チェック：${at}`,
      launchAtLogin: "ログイン時に開く",
      launchHint: "メニューバーに常駐してチェックインを続けます",
      reportLanguage: "レポートの言語",
      version: (version) => `Sanka Monitor ${version}`,
      upToDate: (at) => `最新です · 確認日時：${at}`,
      downloaded: (version) => `バージョン ${version} をダウンロード済み`,
      checkNow: "アップデートを確認",
      checking: "確認中…",
      restartToUpdate: "再起動してアップデート",
    },
    first: {
      title: "このPCのITリスクを確認します",
      body: "Sanka Monitorは、このPCのセキュリティ設定とソフトウェアを読み取り、提供元のサポート期限と照らし合わせます。",
      nothingLeaves: "レポートを送信するか、会社が接続しない限り、データはこのPCの外に出ません。",
      steps: ["セキュリティ設定", "インストール済みのソフトウェア", "提供元のサポート期限", "デスクトップアプリのSaaS"],
      seeResults: "結果を見る",
      connect: "会社でSanka Monitorを使っている",
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
    : date.toLocaleString(lang === "ja" ? "ja-JP" : "en-US", { dateStyle: "medium", timeStyle: "short" });
}

/** A time today, or a date and time otherwise: "Today, 8:39 AM". */
export function formatWhen(iso: string | null, lang: Lang): string {
  if (!iso) return STRINGS[lang].never;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return STRINGS[lang].never;
  const locale = lang === "ja" ? "ja-JP" : "en-US";
  const time = date.toLocaleTimeString(locale, { hour: "numeric", minute: "2-digit" });
  const today = new Date();
  if (date.toDateString() === today.toDateString()) {
    return lang === "ja" ? `今日 ${time}` : `Today, ${time}`;
  }
  return formatTime(iso, lang);
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
