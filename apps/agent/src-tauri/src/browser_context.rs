//! Chromium native-message bridge. No capture or UI initialization in host mode.
use crate::context_history::{self, ContextState};
use base64::{engine::general_purpose::STANDARD, Engine};
use monitor_context::Action;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
#[cfg(target_os = "windows")]
#[path = "browser_context_windows.rs"]
mod windows;
#[cfg(target_os = "windows")]
use windows::register_windows;
const MAX_FRAME: usize = 4096;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    native_host: String,
    extension_id: String,
}
fn identity() -> Identity {
    serde_json::from_str(include_str!("../browser-extension-identity.json"))
        .expect("committed browser identity")
}
fn valid_origin(origin: &str) -> bool {
    origin == format!("chrome-extension://{}/", identity().extension_id)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Frame {
    Status {
        #[serde(rename = "schemaVersion")]
        schema_version: u8,
    },
    Pause {
        #[serde(rename = "schemaVersion")]
        schema_version: u8,
        site: String,
    },
    Observation {
        #[serde(rename = "schemaVersion")]
        schema_version: u8,
        site: String,
        action: Action,
        #[serde(rename = "objectCategory")]
        object_category: String,
        provenance: String,
    },
}
fn parse(bytes: &[u8]) -> Result<Frame, ()> {
    if bytes.len() > MAX_FRAME {
        return Err(());
    }
    let frame: Frame = serde_json::from_slice(bytes).map_err(|_| ())?;
    let valid = match &frame {
        Frame::Status { schema_version } => *schema_version == 1,
        Frame::Pause {
            schema_version,
            site,
        } => *schema_version == 1 && site == "google-sheets",
        Frame::Observation {
            schema_version,
            site,
            action,
            object_category,
            provenance,
        } => {
            *schema_version == 1
                && site == "google-sheets"
                && *action != Action::Unknown
                && object_category == "spreadsheet"
                && provenance == "inferred"
        }
    };
    if valid {
        Ok(frame)
    } else {
        Err(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    schema_version: u8,
    accepted: bool,
    status: String,
}
impl Response {
    fn new(accepted: bool, status: &str) -> Self {
        Self {
            schema_version: 1,
            accepted,
            status: status.into(),
        }
    }
}
fn read_frame(reader: &mut impl Read, limit: usize) -> Result<Vec<u8>, ()> {
    let mut size = [0; 4];
    reader.read_exact(&mut size).map_err(|_| ())?;
    let size = u32::from_ne_bytes(size) as usize;
    if size == 0 || size > limit {
        return Err(());
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).map_err(|_| ())?;
    Ok(bytes)
}
fn write_frame(writer: &mut impl Write, bytes: &[u8], limit: usize) -> Result<(), ()> {
    if bytes.len() > limit {
        return Err(());
    }
    writer
        .write_all(&(bytes.len() as u32).to_ne_bytes())
        .and_then(|_| writer.write_all(bytes))
        .and_then(|_| writer.flush())
        .map_err(|_| ())
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Endpoint {
    instance: Uuid,
    port: u16,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Secret {
    instance: Uuid,
    secret: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Relay {
    instance: Uuid,
    secret: String,
    frame: serde_json::Value,
}
fn bridge_key() -> Result<keyring::Entry, ()> {
    keyring::Entry::new("com.sanka.monitor.context", "browser-bridge").map_err(|_| ())
}
fn data_dir() -> Result<PathBuf, ()> {
    #[cfg(target_os = "macos")]
    {
        Ok(PathBuf::from(std::env::var_os("HOME").ok_or(())?)
            .join("Library/Application Support/com.sanka.monitor"))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(PathBuf::from(std::env::var_os("APPDATA").ok_or(())?).join("com.sanka.monitor"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(())
    }
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Refusing a linked context file".into());
    }
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })()
    .map_err(|_| "Could not save browser bridge configuration".into());
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
fn relay(frame: &Frame) -> Response {
    let result = (|| -> Result<Response, &'static str> {
        let secret_value = crate::secure_storage::access(false, || {
            bridge_key()
                .map_err(|_| keyring::Error::NoEntry)?
                .get_password()
        })
        .map_err(|error| {
            if matches!(error, keyring::Error::NoEntry) {
                "not_running"
            } else {
                "access_required"
            }
        })?;
        let secret: Secret = serde_json::from_str(&secret_value).map_err(|_| "access_required")?;
        let path = data_dir()
            .map_err(|_| "not_running")?
            .join("context-browser-endpoint.json");
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| "not_running")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 {
            return Err("not_running");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("not_running");
            }
        }
        let endpoint: Endpoint =
            serde_json::from_slice(&std::fs::read(path).map_err(|_| "not_running")?)
                .map_err(|_| "not_running")?;
        if endpoint.instance != secret.instance {
            return Err("not_running");
        }
        let mut socket = TcpStream::connect_timeout(
            &SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), endpoint.port),
            Duration::from_millis(250),
        )
        .map_err(|_| "not_running")?;
        socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .map_err(|_| "not_running")?;
        socket
            .set_write_timeout(Some(Duration::from_millis(500)))
            .map_err(|_| "not_running")?;
        let bytes = serde_json::to_vec(&Relay {
            instance: secret.instance,
            secret: secret.secret,
            frame: serde_json::to_value(frame).map_err(|_| "not_running")?,
        })
        .map_err(|_| "not_running")?;
        write_frame(&mut socket, &bytes, MAX_FRAME).map_err(|_| "not_running")?;
        serde_json::from_slice(&read_frame(&mut socket, MAX_FRAME).map_err(|_| "not_running")?)
            .map_err(|_| "not_running")
    })();
    result.unwrap_or_else(|status| Response::new(false, status))
}
/// True for native-host invocations, including invalid origins (never fall through to UI).
pub fn run_if_host(args: &[String]) -> Option<i32> {
    let explicit = args.get(1).is_some_and(|s| s == "--context-browser-host");
    let origin = args.get(if explicit { 2 } else { 1 });
    if !explicit && !origin.is_some_and(|s| s.starts_with("chrome-extension://")) {
        return None;
    }
    if !origin.is_some_and(|s| valid_origin(s)) {
        return Some(1);
    }
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    while let Ok(bytes) = read_frame(&mut input, MAX_FRAME) {
        let response = match parse(&bytes) {
            Ok(frame) => relay(&frame),
            Err(_) => Response::new(false, "invalid_message"),
        };
        let Ok(bytes) = serde_json::to_vec(&response) else {
            return Some(1);
        };
        if write_frame(&mut output, &bytes, MAX_FRAME).is_err() {
            return Some(1);
        }
    }
    Some(0)
}
fn authorized(relay: &Relay, secret: &Secret) -> bool {
    // Compare all secret bytes; do not log either value or rejected frames.
    relay.instance == secret.instance
        && relay.secret.len() == secret.secret.len()
        && relay
            .secret
            .bytes()
            .zip(secret.secret.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}
fn process(bytes: &[u8], secret: &Secret, accept: impl FnOnce(Frame) -> Response) -> Response {
    let Ok(relay) = serde_json::from_slice::<Relay>(bytes) else {
        return Response::new(false, "invalid_message");
    };
    if !authorized(&relay, secret) {
        return Response::new(false, "invalid_message");
    }
    let Ok(frame) = serde_json::to_vec(&relay.frame)
        .map_err(|_| ())
        .and_then(|b| parse(&b))
    else {
        return Response::new(false, "invalid_message");
    };
    accept(frame)
}
static BRIDGE_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn start(app: AppHandle) {
    use std::sync::atomic::Ordering;
    if BRIDGE_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("context-browser-relay".into())
        .spawn(move || {
            struct Running;
            impl Drop for Running {
                fn drop(&mut self) {
                    BRIDGE_STARTED.store(false, Ordering::SeqCst);
                }
            }
            let _running = Running;
            let Ok(dir) = app.path().app_data_dir() else {
                return;
            };
            let path = dir.join("context-browser-endpoint.json");
            // A stale endpoint never survives failure to obtain this launch's secret.
            if path.is_file() {
                let _ = std::fs::remove_file(&path);
            }
            let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else {
                return;
            };
            let mut random = [0; 32];
            if getrandom::fill(&mut random).is_err() {
                return;
            }
            let secret = Secret {
                instance: Uuid::new_v4(),
                secret: STANDARD.encode(random),
            };
            let Ok(entry) = bridge_key() else { return };
            let Ok(serialized) = serde_json::to_string(&secret) else {
                return;
            };
            if crate::secure_storage::access(false, || entry.set_password(&serialized)).is_err() {
                return;
            }
            let Ok(address) = listener.local_addr() else {
                return;
            };
            let endpoint = Endpoint {
                instance: secret.instance,
                port: address.port(),
            };
            if private_write(&path, &serde_json::to_vec(&endpoint).unwrap()).is_err() {
                return;
            }
            // Serial accept/read with a deadline: one bounded client, no event queue.
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                if stream
                    .set_read_timeout(Some(Duration::from_millis(250)))
                    .is_err()
                    || stream
                        .set_write_timeout(Some(Duration::from_millis(250)))
                        .is_err()
                {
                    continue;
                }
                let Ok(bytes) = read_frame(&mut stream, MAX_FRAME) else {
                    continue;
                };
                let state = app.state::<ContextState>();
                let response = process(&bytes, &secret, |frame| match frame {
                    Frame::Status { .. } => Response::new(
                        false,
                        if context_history::browser_status(&state, false) {
                            "active"
                        } else {
                            "paused"
                        },
                    ),
                    Frame::Pause { .. } => {
                        context_history::browser_status(&state, true);
                        Response::new(true, "paused")
                    }
                    Frame::Observation { action, .. } => {
                        let accepted = context_history::ingest_browser_observation(&state, action);
                        Response::new(accepted, if accepted { "active" } else { "paused" })
                    }
                });
                let _ = write_frame(
                    &mut stream,
                    &serde_json::to_vec(&response).unwrap(),
                    MAX_FRAME,
                );
            }
            let _ = std::fs::remove_file(path);
            let _ = crate::secure_storage::access(false, || entry.delete_credential());
        });
    if spawned.is_err() {
        BRIDGE_STARTED.store(false, Ordering::SeqCst);
    }
}
pub fn shutdown(app: &AppHandle) {
    app.state::<ContextState>().pause();
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::remove_file(dir.join("context-browser-endpoint.json"));
    }
    if let Ok(entry) = bridge_key() {
        let _ = crate::secure_storage::access(false, || entry.delete_credential());
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Edge,
}
#[tauri::command]
pub fn open_context_browser_extension(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://chromewebstore.google.com/detail/sanka-monitor/pnmoihnlnknipihnmjipgcanlifjenhh", None::<&str>)
        .map_err(|_| "Could not open the extension store".into())
}

#[tauri::command]
pub async fn register_context_browser_host(
    app: AppHandle,
    browser: Browser,
) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "App directory unavailable")?;
    tauri::async_runtime::spawn_blocking(move || register(&dir, browser))
        .await
        .map_err(|_| "Registration unavailable")?
}
fn register(dir: &Path, browser: Browser) -> Result<String, String> {
    let id = identity();
    let exe = std::env::current_exe().map_err(|_| "Executable unavailable")?;
    let own_dir = dir.join("context-browser-host");
    std::fs::create_dir_all(&own_dir).map_err(|_| "Cannot create host directory")?;
    if std::fs::symlink_metadata(&own_dir)
        .map_err(|_| "Host directory unavailable")?
        .file_type()
        .is_symlink()
    {
        return Err("Refusing linked host directory".into());
    }
    #[cfg(target_os = "macos")]
    let (host, manifest) = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&own_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect host directory")?;
        let host = own_dir.join("native-host");
        let base = PathBuf::from(std::env::var_os("HOME").ok_or("User home unavailable")?).join(
            match browser {
                Browser::Chrome => "Library/Application Support/Google/Chrome/NativeMessagingHosts",
                Browser::Edge => "Library/Application Support/Microsoft Edge/NativeMessagingHosts",
            },
        );
        std::fs::create_dir_all(&base).map_err(|_| "Cannot create browser manifest directory")?;
        if std::fs::symlink_metadata(&base)
            .map_err(|_| "Browser directory unavailable")?
            .file_type()
            .is_symlink()
        {
            return Err("Refusing linked browser manifest directory".into());
        }
        (host, base.join(format!("{}.json", id.native_host)))
    };
    #[cfg(target_os = "windows")]
    let (host, manifest) = (exe, own_dir.join(format!("{}.json", id.native_host)));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (browser, exe);
        return Err("Browser context is available on macOS and Windows".into());
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let value = serde_json::json!({"name":id.native_host,"description":"Sanka Monitor work context","path":host,"type":"stdio","allowed_origins":[format!("chrome-extension://{}/",id.extension_id)]});
        if manifest.exists() {
            let old: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&manifest).map_err(|_| "Existing registration unreadable")?,
            )
            .map_err(|_| "Existing registration is not owned by this app")?;
            if old["name"] != value["name"]
                || old["description"] != value["description"]
                || old["allowed_origins"] != value["allowed_origins"]
                || old["path"] != value["path"]
            {
                return Err("Preserving an existing browser host registration".into());
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            let marker = "#!/bin/sh\n# Sanka Monitor context native host v1\n";
            if host.exists() && !std::fs::read_to_string(&host).is_ok_and(|v| v.starts_with(marker))
            {
                return Err("Preserving an existing native host launcher".into());
            }
            let quoted = exe.to_string_lossy().replace('\'', "'\\''");
            private_write(
                &host,
                format!("{marker}exec '{quoted}' --context-browser-host \"$@\"\n").as_bytes(),
            )?;
            std::fs::set_permissions(&host, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Cannot protect native host")?;
        }
        private_write(
            &manifest,
            &serde_json::to_vec_pretty(&value).map_err(|_| "Invalid host manifest")?,
        )?;
        #[cfg(target_os = "windows")]
        register_windows(browser, &id.native_host, &manifest)?;
        Ok(manifest.to_string_lossy().into_owned())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_frames_reject_untrusted_content_and_origins_before_relay() {
        let valid=br#"{"schemaVersion":1,"kind":"observation","site":"google-sheets","action":"export","objectCategory":"spreadsheet","provenance":"inferred"}"#;
        assert!(matches!(
            parse(valid),
            Ok(Frame::Observation {
                action: Action::Export,
                ..
            })
        ));
        for bytes in [
            br#"{"schemaVersion":1,"kind":"status","url":"secret"}"#.as_slice(),
            br#"{"schemaVersion":2,"kind":"status"}"#,
            b"not json",
            &vec![b'x'; 4097],
        ] {
            assert!(parse(bytes).is_err());
        }
        assert!(!valid_origin("chrome-extension://other/"));
        assert!(!valid_origin(&format!(
            "https://{}/",
            identity().extension_id
        )));
        let mut framed = Vec::new();
        write_frame(&mut framed, valid, MAX_FRAME).unwrap();
        assert_eq!(
            read_frame(&mut framed.as_slice(), MAX_FRAME).unwrap(),
            valid
        );
        assert!(read_frame(&mut (4097u32.to_ne_bytes()).as_slice(), MAX_FRAME).is_err());
    }
    #[test]
    fn stale_or_unauthenticated_local_clients_cannot_reach_runtime() {
        let secret = Secret {
            instance: Uuid::new_v4(),
            secret: "correct-secret".into(),
        };
        for (instance, credential) in [
            (secret.instance, "wrong-secret"),
            (Uuid::new_v4(), "correct-secret"),
        ] {
            let relay = Relay {
                instance,
                secret: credential.into(),
                frame: serde_json::json!({"schemaVersion":1,"kind":"status"}),
            };
            assert!(
                !process(&serde_json::to_vec(&relay).unwrap(), &secret, |_| panic!(
                    "unauthorized callback"
                ))
                .accepted
            );
        }
        let relay = Relay {
            instance: secret.instance,
            secret: secret.secret.clone(),
            frame: serde_json::json!({"schemaVersion":1,"kind":"pause","site":"google-sheets"}),
        };
        assert!(
            process(&serde_json::to_vec(&relay).unwrap(), &secret, |f| {
                Response::new(matches!(f, Frame::Pause { .. }), "paused")
            })
            .accepted
        );
        let state = ContextState::default();
        assert!(!context_history::browser_status(&state, false));
        assert!(!context_history::ingest_browser_observation(
            &state,
            Action::Export
        ));
    }
}
