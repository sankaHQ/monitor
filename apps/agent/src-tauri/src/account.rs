//! Signing in with a Sanka account to send reports.
//!
//! Scanning and saving reports need no account; only "Send to Sanka" does.
//! The app opens `<app base>/desktop/monitor/connect?challenge&state&platform`
//! in the system browser (RFC 8252: browser + PKCE, as Sanka Recorder). The
//! person signs in, or creates a free account, picks the workspace to send as
//! and approves; the page hands a one-time code back through
//! `sanka-monitor://auth/callback?code&state`, or the person pastes it. The
//! app exchanges the code and its verifier for a device credential, kept in
//! the OS keychain, and the credential for a one-hour token just before it
//! sends.
//!
//! Signing in sends Sanka the code, the verifier, the app version and the
//! platform: no computer name and nothing from the scan. The credential only
//! sends reports; it is not the connected mode's enrollment. Sign out revokes
//! it on the server when it can, and always forgets it here.
//!
//! Every request the account makes is listed in the Privacy pane before it
//! goes out: the sign-in, each token request, each report and the revoke,
//! with the code, the verifier and the device secret left out.

use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use monitor_core::DEEP_LINK_SCHEME;
use monitor_report::Lang;
use monitor_sync::{
    AgentClient, DesktopEndpoint, DesktopSignInData, DesktopSignInRequest, DesktopTokenData,
    DesktopTokenRequest, SyncError, DESKTOP_INVALID_GRANT,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter as _, Manager, State};
use tauri_plugin_opener::OpenerExt as _;

use crate::agent::{save_config, AccountConfig, AgentState, AGENT_VERSION, KEYRING_SERVICE};

/// Where the connect page lives: the Monitor host, which is also the Monitor
/// sign-up door. Overridable via `appBase` in config.json.
pub const DEFAULT_APP_BASE: &str = "https://monitor.sanka.com";
/// The window hears about a sign-in that finished in the browser.
pub const ACCOUNT_EVENT: &str = "account-changed";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// A token this close to expiring is replaced before a send.
const TOKEN_MARGIN: Duration = Duration::from_secs(60);
/// Asking Sanka for a token holds the token lock: it never waits longer.
const TOKEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Sign out waits this long for Sanka, then forgets the account anyway.
const SIGN_OUT_TIMEOUT: Duration = Duration::from_secs(15);

/// Why the app has no runtime token for a send.
#[derive(Debug)]
pub enum TokenError {
    /// Sanka refused (a signed-out credential among them) or was unreachable.
    Sync(SyncError),
    /// The keychain did not hand over the secret (locked, or the person chose
    /// Deny). The account is still signed in, here and in Sanka.
    Keychain(String),
    /// Sanka did not answer in time.
    TimedOut,
}

impl TokenError {
    /// Only Sanka's answer signs the app out: never a keychain prompt or a slow
    /// network, which would leave a credential live in Sanka with no app here
    /// to revoke it.
    pub fn signed_out(&self) -> bool {
        matches!(self, TokenError::Sync(err) if err.signed_out())
    }
}

impl From<SyncError> for TokenError {
    fn from(err: SyncError) -> Self {
        TokenError::Sync(err)
    }
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Sync(err) => err.fmt(f),
            TokenError::Keychain(message) => f.write_str(message),
            TokenError::TimedOut => f.write_str("Sanka did not answer in time"),
        }
    }
}

/// A sign-in the person started that the browser has not answered yet.
pub struct PendingSignIn {
    verifier: String,
    state: String,
    started: Instant,
}

/// The account as the window shows it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub signed_in: bool,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    pub workspace_name: Option<String>,
    /// A sign-in is waiting for the browser.
    pub signing_in: bool,
    /// Why the last sign-in from the browser failed.
    pub error: Option<String>,
}

fn random_token(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|err| err.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// RFC 7636 S256.
pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else {
        "macos"
    }
}

/// The connect page in the window's language.
pub fn connect_url(app_base: &str, lang: Lang, challenge: &str, state: &str) -> String {
    let prefix = match lang {
        Lang::Ja => "/ja",
        Lang::En => "",
    };
    let mut url = url::Url::parse(&format!(
        "{}{prefix}/desktop/monitor/connect",
        app_base.trim_end_matches('/')
    ))
    .expect("the app base is a URL");
    url.query_pairs_mut()
        .append_pair("challenge", challenge)
        .append_pair("state", state)
        .append_pair("platform", platform());
    url.into()
}

/// `sanka-monitor://auth/callback?code=…&state=…`, or nothing for any other link.
pub fn callback_params(raw: &str) -> Option<(String, String)> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != DEEP_LINK_SCHEME
        || url.host_str() != Some("auth")
        || url.path() != "/callback"
    {
        return None;
    }
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .filter(|value| !value.is_empty())
    };
    Some((param("code")?, param("state")?))
}

fn keyring_entry(device_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("account:{device_id}"))
        .map_err(|err| err.to_string())
}

async fn store_secret(device_id: String, secret: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        keyring_entry(&device_id)?
            .set_password(&secret)
            .map_err(|err| format!("could not save the sign-in in the keychain: {err}"))
    })
    .await
    .map_err(|err| err.to_string())?
}

/// The secret, or `None` when this computer's keychain no longer holds it.
async fn read_secret(device_id: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || match keyring_entry(&device_id)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format!(
            "Sanka Monitor could not read its sign-in from the keychain ({err}). Allow access and try again."
        )),
    })
    .await
    .map_err(|err| err.to_string())?
}

async fn delete_secret(device_id: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        keyring_entry(&device_id)
            .and_then(|entry| entry.delete_credential().map_err(|err| err.to_string()))
    })
    .await;
}

pub async fn account_view(app: &AppHandle) -> AccountView {
    let state: State<'_, AgentState> = app.state();
    let account = state.config.lock().await.account.clone();
    let signing_in = state
        .pending_sign_in
        .lock()
        .await
        .as_ref()
        .is_some_and(|pending| pending.started.elapsed() < SIGN_IN_TIMEOUT);
    let error = state.account_error.lock().await.clone();
    match account {
        Some(account) => AccountView {
            signed_in: true,
            user_name: account.user_name,
            user_email: account.user_email,
            workspace_name: account.workspace_name,
            signing_in: false,
            error,
        },
        None => AccountView {
            signing_in,
            error,
            ..AccountView::default()
        },
    }
}

#[tauri::command]
pub async fn get_account(app: AppHandle) -> AccountView {
    account_view(&app).await
}

/// Opens the connect page in the system browser.
#[tauri::command]
pub async fn begin_sign_in(app: AppHandle, lang: Lang) -> Result<AccountView, String> {
    let state: State<'_, AgentState> = app.state();
    let verifier = random_token(64)?;
    let nonce = random_token(16)?;
    let app_base = state.config.lock().await.app_base();
    let url = connect_url(&app_base, lang, &challenge_for(&verifier), &nonce);
    *state.pending_sign_in.lock().await = Some(PendingSignIn {
        verifier,
        state: nonce,
        started: Instant::now(),
    });
    *state.account_error.lock().await = None;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| format!("could not open the browser: {err}"))?;
    Ok(account_view(&app).await)
}

/// The code the person pasted, when the browser did not hand it back.
#[tauri::command]
pub async fn finish_sign_in(app: AppHandle, code: String) -> Result<AccountView, String> {
    complete(&app, code.trim(), None).await?;
    Ok(account_view(&app).await)
}

#[tauri::command]
pub async fn cancel_sign_in(app: AppHandle) -> AccountView {
    *app.state::<AgentState>().pending_sign_in.lock().await = None;
    account_view(&app).await
}

/// The code from `sanka-monitor://auth/callback` (with its state) or pasted.
async fn complete(app: &AppHandle, code: &str, returned_state: Option<&str>) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let (verifier, attempt) = {
        let pending = state.pending_sign_in.lock().await;
        let pending = pending
            .as_ref()
            .filter(|pending| pending.started.elapsed() < SIGN_IN_TIMEOUT)
            .ok_or("Start the sign-in from Sanka Monitor again.")?;
        if returned_state.is_some_and(|returned| returned != pending.state) {
            return Err("This sign-in link belongs to another attempt. Start again.".into());
        }
        (pending.verifier.clone(), pending.state.clone())
    };
    if code.is_empty() {
        return Err("Paste the sign-in code from the browser.".into());
    }
    let api_base = state.config.lock().await.api_base();
    let client = AgentClient::new(api_base, AGENT_VERSION).map_err(|err| err.to_string())?;
    let request = DesktopSignInRequest {
        code: code.to_string(),
        code_verifier: verifier,
        platform: platform().to_string(),
        app_version: AGENT_VERSION.to_string(),
    };
    // Listed in the Privacy pane like every other request, without the secrets.
    let request_id = state
        .record_payload(
            &DesktopEndpoint::Devices.path(),
            serde_json::json!({
                "code": "(one-time sign-in code)",
                "codeVerifier": "(this app's PKCE verifier)",
                "platform": request.platform,
                "appVersion": request.app_version,
            }),
        )
        .await;
    let signed_in = client
        .desktop_sign_in(&request)
        .await
        .map_err(|err| match err.code() {
            Some(DESKTOP_INVALID_GRANT) => {
                "The sign-in code is invalid or expired. Sign in again.".to_string()
            }
            _ => format!("Sign-in failed: {err}"),
        })?;
    state.mark_payload_delivered(request_id).await;
    // The person may have cancelled this sign-in, or started another one, while
    // the code was on its way: only the attempt still waiting saves an account.
    let still_waiting = take_if_waiting(&mut *state.pending_sign_in.lock().await, &attempt);
    if !still_waiting {
        discard(&state, &client, &signed_in).await;
        return Ok(());
    }
    if let Err(err) = save_account(app, &signed_in).await {
        discard(&state, &client, &signed_in).await;
        return Err(err);
    }
    Ok(())
}

/// Takes the waiting sign-in when it is still `attempt`. A cancelled or
/// replaced attempt finds nothing to take, so its late answer can neither save
/// an account nor clear the newer attempt.
fn take_if_waiting(pending: &mut Option<PendingSignIn>, attempt: &str) -> bool {
    if pending
        .as_ref()
        .is_some_and(|pending| pending.state == attempt)
    {
        *pending = None;
        return true;
    }
    false
}

/// Keeps a finished sign-in: the secret in the keychain, the names in the
/// config, and the previous account's secret deleted.
async fn save_account(app: &AppHandle, signed_in: &DesktopSignInData) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    store_secret(signed_in.device_id.clone(), signed_in.device_secret.clone()).await?;
    let saved = {
        let mut config = state.config.lock().await;
        let mut next = config.clone();
        let previous = next.account.replace(AccountConfig {
            device_id: signed_in.device_id.clone(),
            user_name: signed_in.user.name.clone(),
            user_email: signed_in.user.email.clone(),
            workspace_name: signed_in.workspace.name.clone(),
            workspace_code: signed_in.workspace.code.clone(),
        });
        save_config(app, &next).map(|()| {
            *config = next;
            previous
        })
    };
    let previous = match saved {
        Ok(previous) => previous,
        Err(err) => {
            delete_secret(signed_in.device_id.clone()).await;
            return Err(err);
        }
    };
    if let Some(previous) = previous.filter(|p| p.device_id != signed_in.device_id) {
        delete_secret(previous.device_id).await;
    }
    *state.account_token.lock().await = None;
    *state.account_error.lock().await = None;
    Ok(())
}

/// Signs out, in Sanka, a credential this app will not keep (a cancelled or
/// replaced sign-in), so none is left live. Best effort, and never longer
/// than Sign out waits.
async fn discard(state: &AgentState, client: &AgentClient, signed_in: &DesktopSignInData) {
    let revoke = async {
        let request = DesktopTokenRequest {
            device_id: signed_in.device_id.clone(),
            device_secret: signed_in.device_secret.clone(),
            app_version: AGENT_VERSION.to_string(),
        };
        let token = request_token(state, client, &request).await?;
        request_revoke(state, client, &token.access_token).await
    };
    let _ = tokio::time::timeout(SIGN_OUT_TIMEOUT, revoke).await;
}

/// The token request as the Privacy pane lists it: every field the app
/// sends, with the device secret left out.
fn listed_token_request(request: &DesktopTokenRequest) -> serde_json::Value {
    serde_json::json!({
        "deviceId": request.device_id,
        "deviceSecret": "(this app's device secret)",
        "appVersion": request.app_version,
    })
}

/// Asks Sanka for a one-hour token, listed in the Privacy pane first.
async fn request_token(
    state: &AgentState,
    client: &AgentClient,
    request: &DesktopTokenRequest,
) -> Result<DesktopTokenData, SyncError> {
    let request_id = state
        .record_payload(
            &DesktopEndpoint::Token.path(),
            listed_token_request(request),
        )
        .await;
    let data = client.desktop_token(request).await?;
    state.mark_payload_delivered(request_id).await;
    Ok(data)
}

/// Signs a credential out in Sanka, listed in the Privacy pane first. The
/// request has no body; its token goes in the header, as for a report.
async fn request_revoke(
    state: &AgentState,
    client: &AgentClient,
    token: &str,
) -> Result<(), SyncError> {
    let request_id = state
        .record_payload(&DesktopEndpoint::Revoke.path(), serde_json::json!({}))
        .await;
    client.desktop_revoke(token).await?;
    state.mark_payload_delivered(request_id).await;
    Ok(())
}

/// A link to `sanka-monitor://…` opened the app, or reached the running one.
pub fn handle_deep_link(app: &AppHandle, raw: &str) {
    let Some((code, returned_state)) = callback_params(raw) else {
        return;
    };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = complete(&handle, &code, Some(&returned_state)).await;
        let state: State<'_, AgentState> = handle.state();
        *state.account_error.lock().await = result.err();
        let view = account_view(&handle).await;
        let _ = handle.emit(ACCOUNT_EVENT, view);
        crate::show_main_window(&handle);
    });
}

/// A runtime token for sending, refreshed under one lock so that parallel
/// sends never race for it.
pub async fn access_token(app: &AppHandle) -> Result<String, TokenError> {
    let state: State<'_, AgentState> = app.state();
    let mut cached = state.account_token.lock().await;
    if let Some((token, expires)) = cached.as_ref() {
        if *expires > Instant::now() + TOKEN_MARGIN {
            return Ok(token.clone());
        }
    }
    let (api_base, account) = {
        let config = state.config.lock().await;
        (config.api_base(), config.account.clone())
    };
    let account = account.ok_or_else(not_signed_in)?;
    let secret = read_secret(account.device_id.clone())
        .await
        .map_err(TokenError::Keychain)?
        // Gone from the keychain: this app can no longer send as the account.
        .ok_or_else(not_signed_in)?;
    let client = AgentClient::new(api_base, AGENT_VERSION)?;
    let request = DesktopTokenRequest {
        device_id: account.device_id,
        device_secret: secret,
        app_version: AGENT_VERSION.to_string(),
    };
    let data = tokio::time::timeout(
        TOKEN_REQUEST_TIMEOUT,
        request_token(&state, &client, &request),
    )
    .await
    .map_err(|_| TokenError::TimedOut)??;
    let token = data.access_token.clone();
    *cached = Some((
        data.access_token,
        Instant::now() + Duration::from_secs(data.expires_in),
    ));
    Ok(token)
}

fn not_signed_in() -> SyncError {
    SyncError::Api {
        status: 401,
        code: Some(monitor_sync::DESKTOP_DEVICE_INVALID.to_string()),
        message: "Sign in to Sanka to send reports.".into(),
    }
}

pub async fn forget_token(app: &AppHandle) {
    *app.state::<AgentState>().account_token.lock().await = None;
}

/// Forgets the account here: the keychain secret, the names and any token.
pub async fn forget_account(app: &AppHandle) -> Result<(), String> {
    let state: State<'_, AgentState> = app.state();
    let previous = {
        let mut config = state.config.lock().await;
        let mut next = config.clone();
        let previous = next.account.take();
        save_config(app, &next)?;
        *config = next;
        previous
    };
    *state.account_token.lock().await = None;
    if let Some(previous) = previous {
        delete_secret(previous.device_id).await;
    }
    Ok(())
}

/// Sign out: revokes the credential in Sanka when it can, then forgets it.
#[tauri::command]
pub async fn sign_out(app: AppHandle) -> Result<AccountView, String> {
    // Without the network the secret is still deleted here, so it cannot send.
    let _ = tokio::time::timeout(SIGN_OUT_TIMEOUT, revoke(&app)).await;
    forget_account(&app).await?;
    *app.state::<AgentState>().account_error.lock().await = None;
    Ok(account_view(&app).await)
}

/// Revokes this app's credential in Sanka: its token, then the revoke call.
async fn revoke(app: &AppHandle) -> Result<(), TokenError> {
    let token = access_token(app).await?;
    let state: State<'_, AgentState> = app.state();
    let api_base = state.config.lock().await.api_base();
    let client = AgentClient::new(api_base, AGENT_VERSION)?;
    request_revoke(&state, &client, &token).await?;
    Ok(())
}

pub fn register_deep_links(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt as _;
    #[cfg(any(windows, target_os = "linux"))]
    {
        // Installers register the scheme; a development run registers it here.
        let _ = app.deep_link().register_all();
    }
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            handle_deep_link(&handle, url.as_str());
        }
    });
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            handle_deep_link(app, url.as_str());
        }
    }
    for arg in std::env::args().filter(|arg| is_deep_link(arg)) {
        handle_deep_link(app, &arg);
    }
}

pub fn is_deep_link(arg: &str) -> bool {
    arg.strip_prefix(DEEP_LINK_SCHEME)
        .is_some_and(|rest| rest.starts_with("://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_rfc_7636_s256_of_the_verifier() {
        // RFC 7636 Appendix B.
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn only_the_sign_in_callback_completes_a_sign_in() {
        // (link, code and state handed to the sign-in)
        for (link, expected) in [
            (
                "sanka-monitor://auth/callback?code=abc&state=xyz",
                Some(("abc", "xyz")),
            ),
            ("sanka-monitor://auth/callback?code=abc", None),
            ("sanka-monitor://auth/callback?code=&state=xyz", None),
            (
                "sanka-monitor://elsewhere/callback?code=abc&state=xyz",
                None,
            ),
            ("https://auth/callback?code=abc&state=xyz", None),
            ("not a link", None),
        ] {
            assert_eq!(
                callback_params(link),
                expected.map(|(code, state)| (code.to_string(), state.to_string())),
                "{link}"
            );
        }
        assert!(is_deep_link("sanka-monitor://auth/callback?code=a&state=b"));
        assert!(!is_deep_link("scan"));
        assert!(!is_deep_link("sanka-monitorx://auth"));
    }

    /// A late answer to a cancelled or replaced sign-in saves nothing.
    #[test]
    fn only_the_waiting_sign_in_keeps_its_answer() {
        let attempt = |state: &str| PendingSignIn {
            verifier: "verifier".into(),
            state: state.into(),
            started: Instant::now(),
        };
        // Still waiting: taken, so no second answer can claim it.
        let mut pending = Some(attempt("first"));
        assert!(take_if_waiting(&mut pending, "first"));
        assert!(pending.is_none());
        // Cancelled while the code was on its way.
        assert!(!take_if_waiting(&mut pending, "first"));
        // Replaced by a newer sign-in, which keeps waiting.
        let mut pending = Some(attempt("second"));
        assert!(!take_if_waiting(&mut pending, "first"));
        assert_eq!(pending.map(|p| p.state).as_deref(), Some("second"));
    }

    /// The connect page gets the challenge, the state and the platform, and
    /// nothing about the computer.
    #[test]
    fn the_connect_page_link_carries_no_computer_details() {
        let link = connect_url("https://monitor.sanka.com/", Lang::Ja, "c", "s");
        let url = url::Url::parse(&link).unwrap();
        assert_eq!(url.path(), "/ja/desktop/monitor/connect");
        let keys: Vec<String> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
        assert_eq!(keys, ["challenge", "state", "platform"]);
        assert_eq!(
            connect_url("https://monitor.sanka.com", Lang::En, "c", "s")
                .split('?')
                .next(),
            Some("https://monitor.sanka.com/desktop/monitor/connect")
        );
    }

    /// Token requests and the revoke are listed in the Privacy pane before
    /// they go out, with the device secret left out, and show as delivered
    /// only once Sanka answers.
    #[test]
    fn token_requests_and_sign_out_are_listed_without_the_secret() {
        use std::io::{Read, Write};

        // Answers the token request, then the revoke, and hands both back.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            [
                r#"{"success":true,"data":{"accessToken":"runtime-token","expiresIn":3600},"error":null}"#,
                r#"{"success":true,"data":{"revokedAt":"2026-10-27T01:00:00Z"},"error":null}"#,
            ]
            .map(|answer| {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                let head_end = loop {
                    let n = stream.read(&mut buf).unwrap();
                    request.extend_from_slice(&buf[..n]);
                    if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let head = String::from_utf8_lossy(&request[..head_end]).to_ascii_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .map_or(0, |value| value.trim().parse().unwrap());
                while request.len() < head_end + length {
                    let n = stream.read(&mut buf).unwrap();
                    request.extend_from_slice(&buf[..n]);
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                    answer.len()
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            })
        });

        let state = AgentState::default();
        let request = DesktopTokenRequest {
            device_id: "device-1".into(),
            device_secret: "device-secret-1".into(),
            app_version: "0.3.5".into(),
        };
        tauri::async_runtime::block_on(async {
            let client = AgentClient::new(base, "0.3.5").unwrap();
            let token = request_token(&state, &client, &request).await.unwrap();
            request_revoke(&state, &client, &token.access_token)
                .await
                .unwrap();
        });
        // On the wire, the token request carries the secret and the revoke
        // the token.
        let [token_request, revoke_request] = server.join().unwrap();
        assert!(token_request.starts_with("POST /api/v2/monitor/desktop/token "));
        assert!(token_request.contains("device-secret-1"));
        assert!(revoke_request.starts_with("POST /api/v2/monitor/desktop/revoke "));
        assert!(revoke_request
            .to_ascii_lowercase()
            .contains("authorization: bearer runtime-token"));

        // In the Privacy pane, newest first: the revoke, then the token
        // request with every field the app sends and the secret left out.
        let sent =
            tauri::async_runtime::block_on(async { state.sent_payloads.lock().await.clone() });
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].endpoint, "/api/v2/monitor/desktop/revoke");
        assert_eq!(sent[0].body, serde_json::json!({}));
        assert_eq!(sent[1].endpoint, "/api/v2/monitor/desktop/token");
        assert_eq!(
            sent[1].body,
            serde_json::json!({
                "deviceId": "device-1",
                "deviceSecret": "(this app's device secret)",
                "appVersion": "0.3.5",
            })
        );
        let keys = |value: &serde_json::Value| {
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            keys(&sent[1].body),
            keys(&serde_json::to_value(&request).unwrap())
        );
        assert!(sent.iter().all(|payload| payload.delivered));

        // Sanka never answered: listed all the same, delivery unconfirmed.
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let failed = tauri::async_runtime::block_on(async {
            let client = AgentClient::new(format!("http://{closed}"), "0.3.5").unwrap();
            request_token(&state, &client, &request).await
        });
        assert!(failed.is_err());
        let sent =
            tauri::async_runtime::block_on(async { state.sent_payloads.lock().await.clone() });
        assert_eq!(sent[0].endpoint, "/api/v2/monitor/desktop/token");
        assert!(!sent[0].delivered);
        assert!(!serde_json::to_string(&sent)
            .unwrap()
            .contains("device-secret-1"));
    }
}
