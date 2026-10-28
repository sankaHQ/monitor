//! The language of the app's native parts: the menu bar menu and its tooltip,
//! and the macOS app menu. The window chooses it (Settings → Language: the
//! system's language, English or Japanese) and reports it with
//! `set_ui_language`; config keeps the last one, so the next launch starts in
//! it before the window has loaded. Japanese wording follows the window's
//! strings (src/i18n.ts) and macOS's own menu terms.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::agent::{save_config, AgentState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLang {
    #[default]
    En,
    Ja,
}

impl UiLang {
    pub fn parse(code: &str) -> Option<UiLang> {
        match code {
            "en" => Some(UiLang::En),
            "ja" => Some(UiLang::Ja),
            _ => None,
        }
    }
}

/// The native strings in one language.
pub struct Text {
    pub open: &'static str,
    pub scan_now: &'static str,
    pub check_in_now: &'static str,
    pub restart_to_update: fn(&str) -> String,
    pub quit: &'static str,
    pub paused: &'static str,
    pub removed: &'static str,
    pub pending: &'static str,
    pub not_scanned: &'static str,
    pub no_risks: &'static str,
    pub risks: fn(usize) -> String,
    // The macOS app menu.
    pub about: &'static str,
    pub check_for_updates: &'static str,
    pub services: &'static str,
    pub hide: &'static str,
    pub hide_others: &'static str,
    pub show_all: &'static str,
    pub edit: &'static str,
    pub undo: &'static str,
    pub redo: &'static str,
    pub cut: &'static str,
    pub copy: &'static str,
    pub paste: &'static str,
    pub select_all: &'static str,
    pub window: &'static str,
    pub minimize: &'static str,
    pub close_window: &'static str,
}

const EN: Text = Text {
    open: "Open Sanka Monitor",
    scan_now: "Scan Now",
    check_in_now: "Check In Now",
    restart_to_update: |version| format!("Restart to Update ({version})"),
    quit: "Quit Sanka Monitor",
    paused: "Paused by your workspace",
    removed: "Removed by your workspace",
    pending: "Waiting for your workspace to approve",
    not_scanned: "Not scanned yet",
    no_risks: "No risks found",
    risks: |n| {
        if n == 1 {
            "1 risk to review".to_string()
        } else {
            format!("{n} risks to review")
        }
    },
    about: "About Sanka Monitor",
    check_for_updates: "Check for Updates…",
    services: "Services",
    hide: "Hide Sanka Monitor",
    hide_others: "Hide Others",
    show_all: "Show All",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    window: "Window",
    minimize: "Minimize",
    close_window: "Close Window",
};

const JA: Text = Text {
    open: "Sanka Monitorを開く",
    scan_now: "今すぐスキャン",
    check_in_now: "今すぐチェックイン",
    restart_to_update: |version| format!("再起動してアップデート（{version}）"),
    quit: "Sanka Monitorを終了",
    paused: "ワークスペースにより一時停止中",
    removed: "ワークスペースにより削除",
    pending: "ワークスペースの承認待ち",
    not_scanned: "まだスキャンしていません",
    no_risks: "リスクは見つかりませんでした",
    risks: |n| format!("確認が必要なリスク：{n}件"),
    about: "Sanka Monitorについて",
    check_for_updates: "アップデートを確認…",
    services: "サービス",
    hide: "Sanka Monitorを非表示",
    hide_others: "ほかを非表示",
    show_all: "すべてを表示",
    edit: "編集",
    undo: "取り消す",
    redo: "やり直す",
    cut: "カット",
    copy: "コピー",
    paste: "ペースト",
    select_all: "すべてを選択",
    window: "ウインドウ",
    minimize: "しまう",
    close_window: "ウインドウを閉じる",
};

pub fn text(lang: UiLang) -> &'static Text {
    match lang {
        UiLang::En => &EN,
        UiLang::Ja => &JA,
    }
}

/// The language the native parts use now.
pub async fn current(app: &AppHandle) -> UiLang {
    let state: State<'_, AgentState> = app.state();
    let config = state.config.lock().await;
    config
        .ui_language
        .as_deref()
        .and_then(UiLang::parse)
        .unwrap_or_default()
}

/// The macOS app menu in `lang`: the app's items with Check for Updates…
/// after About, then Edit (so copy and paste work in the window) and Window.
#[cfg(target_os = "macos")]
pub fn app_menu(app: &AppHandle, lang: UiLang) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let t = text(lang);
    let separator = || PredefinedMenuItem::separator(app);
    let package = app.package_info();
    let about = AboutMetadata {
        name: Some(package.name.clone()),
        version: Some(package.version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        ..AboutMetadata::default()
    };
    let app_items = Submenu::with_items(
        app,
        "Sanka Monitor",
        true,
        &[
            &PredefinedMenuItem::about(app, Some(t.about), Some(about))?,
            &MenuItem::with_id(
                app,
                crate::CHECK_FOR_UPDATES_MENU_ID,
                t.check_for_updates,
                true,
                None::<&str>,
            )?,
            &separator()?,
            &PredefinedMenuItem::services(app, Some(t.services))?,
            &separator()?,
            &PredefinedMenuItem::hide(app, Some(t.hide))?,
            &PredefinedMenuItem::hide_others(app, Some(t.hide_others))?,
            &PredefinedMenuItem::show_all(app, Some(t.show_all))?,
            &separator()?,
            &PredefinedMenuItem::quit(app, Some(t.quit))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        t.edit,
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(t.undo))?,
            &PredefinedMenuItem::redo(app, Some(t.redo))?,
            &separator()?,
            &PredefinedMenuItem::cut(app, Some(t.cut))?,
            &PredefinedMenuItem::copy(app, Some(t.copy))?,
            &PredefinedMenuItem::paste(app, Some(t.paste))?,
            &PredefinedMenuItem::select_all(app, Some(t.select_all))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        t.window,
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(t.minimize))?,
            &PredefinedMenuItem::close_window(app, Some(t.close_window))?,
        ],
    )?;
    Menu::with_items(app, &[&app_items, &edit, &window])
}

/// The window's language changed (or it loaded): remember it and redraw the
/// native menus in it.
#[tauri::command]
pub async fn set_ui_language(app: AppHandle, lang: String) -> Result<(), String> {
    let lang = UiLang::parse(&lang).ok_or_else(|| format!("unknown language {lang}"))?;
    {
        let state: State<'_, AgentState> = app.state();
        let mut config = state.config.lock().await;
        let code = serde_json::to_value(lang)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string));
        if config.ui_language != code {
            config.ui_language = code;
            save_config(&app, &config)?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        let menu = app_menu(&app, lang).map_err(|err| err.to_string())?;
        app.set_menu(menu).map_err(|err| err.to_string())?;
    }
    crate::tray::refresh(&app).await;
    Ok(())
}
