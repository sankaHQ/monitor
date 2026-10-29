//! Explicit local context consent, immutable previews and account-bound cloud delivery.
use crate::{
    account,
    agent::{AgentConfig, AgentState, AGENT_VERSION},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use monitor_context::{
    Action, Audience, Collector, ContextApp, ContextCandidate, ContextCoverage, ContextEpisode,
    ContextEvent, ContextPackage, ContextSettings, ContextStore, ContextWorkflow, Gap,
    ObjectCategory, Provenance, ReportEvidence, SessionGate, MAX_PACKAGE_BYTES,
};
use monitor_sync::{AgentClient, ContextReceipt, ContextRoute};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt as _;
use uuid::Uuid;
#[path = "context_session.rs"]
mod session;
use session::{install_session_notifications, session_available};

pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Default)]
struct Authority {
    generation: u64,
}
impl Authority {
    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
    fn freeze(&self, identity: &str, json: &str) -> Preview {
        Preview {
            token: Uuid::new_v4().to_string(),
            package_json: json.into(),
            sha256: hash(json.as_bytes()),
            identity: identity.into(),
            generation: self.generation,
        }
    }
    fn permits(&self, preview: &Preview, identity: &str) -> bool {
        preview.identity == identity
            && preview.generation == self.generation
            && preview.sha256 == hash(preview.package_json.as_bytes())
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub token: String,
    pub package_json: String,
    pub sha256: String,
    #[serde(skip)]
    identity: String,
    #[serde(skip)]
    generation: u64,
}
impl Preview {
    fn receipt_matches(&self, sha: &str) -> bool {
        self.sha256 == sha
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Identity {
    scope: String,
    workspace: Option<Uuid>,
    device: Option<Uuid>,
    api: String,
    origin: String,
    code: Option<String>,
}
impl Identity {
    fn from_config(config: &AgentConfig) -> Self {
        let account = config.account.as_ref();
        let api = config.api_base();
        let origin = config.app_base();
        let scope = hash(
            format!(
                "{}|{}|{}|{}",
                api,
                origin,
                account.map(|a| a.device_id.as_str()).unwrap_or("local"),
                account
                    .and_then(|a| a.workspace_id.as_deref())
                    .unwrap_or("local")
            )
            .as_bytes(),
        );
        Self {
            scope,
            workspace: account
                .and_then(|a| a.workspace_id.as_deref())
                .and_then(|v| Uuid::parse_str(v).ok()),
            device: account.and_then(|a| Uuid::parse_str(&a.device_id).ok()),
            api,
            origin,
            code: account.and_then(|a| a.workspace_code.clone()),
        }
    }
}
pub(super) fn account_scope(config: &AgentConfig) -> String {
    Identity::from_config(config).scope
}
#[derive(Clone)]
struct Recurring {
    selection_generation: u64,
    grant_id: Uuid,
    selection_sha256: String,
    day: Option<chrono::NaiveDate>,
    identity: Identity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RevocationScope {
    Summaries,
    All,
}
struct Runtime {
    identity: Identity,
    store: ContextStore,
    settings: ContextSettings,
    collector: Collector,
    authority: Authority,
    preview: Option<Preview>,
    recurring: Option<Recurring>,
    last_gap: Option<Gap>,
    last_browser: Option<Instant>,
    error: Option<String>,
    personal_os_login: bool,
    key_fingerprint: String,
    pending_revocation: Option<RevocationScope>,
}
impl Runtime {
    fn revocation_pending(&self) -> bool {
        self.pending_revocation.is_some()
    }
    fn require_revocation(&mut self, scope: RevocationScope) {
        self.pending_revocation = Some(match (self.pending_revocation, scope) {
            (Some(RevocationScope::All), _) | (_, RevocationScope::All) => RevocationScope::All,
            _ => RevocationScope::Summaries,
        });
        let message = match self.pending_revocation {
            Some(RevocationScope::All) => "All context revocation remains pending; previously shared content may still be on the server",
            _ => "Daily sharing stopped locally; remote revocation pending",
        };
        self.error = Some(message.into());
    }
    fn acknowledge_revocation(&mut self, scope: RevocationScope) {
        let covered = match self.pending_revocation {
            None => false,
            Some(RevocationScope::Summaries) => true,
            Some(RevocationScope::All) => scope == RevocationScope::All,
        };
        if covered {
            self.pending_revocation = None;
            self.error = None;
        }
    }
    fn invalidate(&mut self) {
        self.authority.invalidate();
        self.preview = None;
    }
    fn check_key(&self) -> Result<(), String> {
        if hash(&key(&self.identity.scope, false)?) != self.key_fingerprint {
            return Err("Context key changed; collection disabled".into());
        }
        Ok(())
    }
    fn prune(&mut self) -> Result<(), String> {
        if self.store.prune(Utc::now()).map_err(|e| e.to_string())? > 0 {
            self.invalidate();
        }
        Ok(())
    }
}
#[derive(Clone, Default)]
pub struct RuntimeGate(Arc<AtomicU64>);
impl RuntimeGate {
    fn pause(&self) {
        let mut previous = self.generation();
        loop {
            match self.0.compare_exchange(
                previous,
                previous.wrapping_add(2) & !1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return,
                Err(value) => previous = value,
            }
        }
    }
    pub fn is_active(&self) -> bool {
        self.0.load(Ordering::SeqCst) & 1 == 1
    }
    fn generation(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn activate_explicitly(&self, generation: u64) -> bool {
        self.0
            .compare_exchange(
                generation,
                generation | 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    }
}

#[derive(Clone, Default)]
pub struct ContextState {
    runtime: Arc<Mutex<Option<Runtime>>>,
    // Session-only, content-free intent survives deleting/reopening an account's local files.
    retained_revocations: Arc<Mutex<BTreeMap<String, RevocationScope>>>,
    pub gate: RuntimeGate,
    session_ready: Arc<std::sync::atomic::AtomicBool>,
    native_gate: Arc<Mutex<Option<SessionGate>>>,
    epoch: Arc<AtomicU64>,
    action_generation: Arc<AtomicU64>,
    sharing_generation: Arc<AtomicU64>,
    // Shared between UI operations and sends; mutation never waits for network I/O.
    send_lock: Arc<tokio::sync::Mutex<()>>,
}
impl ContextState {
    fn retire_runtime(&self, slot: &mut Option<Runtime>) -> Result<(), String> {
        let mut retained = self
            .retained_revocations
            .lock()
            .map_err(|_| "Pending revocation state unavailable")?;
        if let Some(runtime) = slot.take() {
            if let Some(scope) = runtime.pending_revocation {
                retained.insert(runtime.identity.scope.clone(), scope);
            } else {
                retained.remove(&runtime.identity.scope);
            }
        }
        Ok(())
    }
    fn restore_runtime(&self, mut runtime: Runtime) -> Result<Runtime, String> {
        let pending = self
            .retained_revocations
            .lock()
            .map_err(|_| "Pending revocation state unavailable")?
            .remove(&runtime.identity.scope);
        if let Some(pending) = pending {
            runtime.require_revocation(pending);
        }
        Ok(runtime)
    }
    pub fn prepare_sign_out(&self) {
        // Fence old scheduler snapshots immediately, even if a store worker owns the mutex.
        self.sharing_generation.fetch_add(1, Ordering::SeqCst);
        self.pause();
        if let Ok(mut slot) = self.runtime.try_lock() {
            if let Some(runtime) = slot.as_mut() {
                runtime.recurring = None;
                runtime.invalidate();
            }
        }
    }
    pub fn pause(&self) {
        self.begin_action();
    }
    fn begin_action(&self) -> u64 {
        let generation = self
            .action_generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        self.pause_for_preparation();
        generation
    }
    fn pause_for_preparation(&self) {
        self.gate.pause();
        if let Ok(gate) = self.native_gate.lock() {
            if let Some(gate) = gate.as_ref() {
                gate.pause();
            }
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
}
fn key(scope: &str, create: bool) -> Result<[u8; 32], String> {
    let entry = keyring::Entry::new("com.sanka.monitor.context", scope)
        .map_err(|_| "Secure storage unavailable")?;
    match entry.get_password() {
        Ok(value) => STANDARD
            .decode(value)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or("Invalid context key".into()),
        Err(keyring::Error::NoEntry) if create => {
            let mut bytes = [0; 32];
            getrandom::fill(&mut bytes).map_err(|_| "Secure random unavailable")?;
            entry
                .set_password(&STANDARD.encode(bytes))
                .map_err(|_| "Secure storage unavailable")?;
            Ok(bytes)
        }
        _ => Err("Secure storage unavailable; work history remains paused".into()),
    }
}
async fn with_runtime<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&mut Runtime) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let state = app.state::<ContextState>().inner().clone();
    let (config, mut expected_epoch) = {
        let agent = app.state::<AgentState>();
        let config = agent.config.lock().await;
        (config.clone(), state.epoch.load(Ordering::SeqCst))
    };
    let identity = Identity::from_config(&config);
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "App directory unavailable")?
        .join("context");
    tauri::async_runtime::spawn_blocking(move || {
        let mut slot = state.runtime.lock().map_err(|_| "Context unavailable")?;
        if state.epoch.load(Ordering::SeqCst) != expected_epoch {
            return Err("Context state changed; try again".into());
        }
        if slot.as_ref().is_none_or(|r| r.identity != identity) {
            state.pause_for_preparation();
            expected_epoch = expected_epoch.wrapping_add(1);
            state.retire_runtime(&mut slot)?;
            std::fs::create_dir_all(&dir).map_err(|_| "Context directory unavailable")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                    .map_err(|_| "Context directory unavailable")?;
            }
            let path = dir.join(format!("{}.bin", identity.scope));
            let secret = key(&identity.scope, !path.exists())?;
            let store = ContextStore::open(&path, secret).map_err(|e| e.to_string())?;
            if state.epoch.load(Ordering::SeqCst) != expected_epoch {
                return Err("Account or consent changed".into());
            }
            let collector = Collector::default();
            *state
                .native_gate
                .lock()
                .map_err(|_| "Context gate unavailable")? = Some(collector.session_gate());
            *slot = Some(state.restore_runtime(Runtime {
                identity,
                store,
                settings: ContextSettings::default(),
                collector,
                authority: Authority::default(),
                preview: None,
                recurring: None,
                last_gap: None,
                last_browser: None,
                error: None,
                personal_os_login: false,
                key_fingerprint: hash(&secret),
                pending_revocation: None,
            })?);
        }
        let r = slot.as_mut().ok_or("Context unavailable")?;
        if config.kill_switch || config.update_required() {
            if state.gate.is_active() {
                state.pause();
            }
            r.settings.paused = true;
            r.invalidate();
        }
        let generation = r.authority.generation;
        r.prune()?;
        if r.authority.generation != generation {
            state.epoch.fetch_add(1, Ordering::SeqCst);
        }
        f(r)
    })
    .await
    .map_err(|_| "Context worker unavailable")?
}
async fn with_runtime_bound<T: Send + 'static>(
    app: &AppHandle,
    scope: &str,
    f: impl FnOnce(&mut Runtime) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let scope = scope.to_owned();
    with_runtime(app, move |r| {
        if r.identity.scope != scope {
            return Err("Account changed; reopen Work History".into());
        }
        f(r)
    })
    .await
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    own_os_login: bool,
    settings: ContextSettings,
    active: bool,
    events: Vec<ContextEvent>,
    candidates: Vec<ContextCandidate>,
    episodes: Vec<ContextEpisode>,
    workflows: Vec<ContextWorkflow>,
    recurring: bool,
    revocation_pending: bool,
    error: Option<String>,
    gap: Option<Gap>,
}
#[tauri::command]
pub async fn get_context_history(app: AppHandle, account_scope: String) -> Result<History, String> {
    let state = app.state::<ContextState>().inner().clone();
    with_runtime_bound(&app, &account_scope, move |r| {
        let candidates = r.store.pending_candidates(Utc::now());
        let workflows = r.store.workflows();
        let episodes = r
            .store
            .candidates()
            .into_iter()
            .filter(|candidate| {
                candidates.iter().any(|pending| pending.id == candidate.id)
                    || workflows.iter().any(|workflow| workflow.id == candidate.id)
            })
            .flat_map(|candidate| candidate.episodes)
            .collect();
        Ok(History {
            own_os_login: r.personal_os_login,
            settings: r.settings.clone(),
            active: state.gate.is_active() && r.settings.enabled && !r.settings.paused,
            events: r.store.events(),
            candidates,
            episodes,
            workflows,
            recurring: r.recurring.as_ref().is_some_and(|grant| {
                grant.selection_generation == state.sharing_generation.load(Ordering::SeqCst)
            }),
            revocation_pending: r.revocation_pending(),
            error: r.error.clone(),
            gap: r.last_gap,
        })
    })
    .await
}
#[tauri::command]
pub async fn dismiss_context_candidate(
    app: AppHandle,
    id: Uuid,
    account_scope: String,
) -> Result<(), String> {
    app.state::<ContextState>()
        .epoch
        .fetch_add(1, Ordering::SeqCst);
    with_runtime_bound(&app, &account_scope, move |r| {
        r.invalidate();
        r.store
            .dismiss_candidate(id, Utc::now())
            .map_err(|e| e.to_string())
    })
    .await
}

/// An explicit local copy, with no account tokens, settings or upload path.
#[tauri::command]
pub async fn export_context_history(app: AppHandle, account_scope: String) -> Result<bool, String> {
    let state = app.state::<ContextState>().inner().clone();
    let epoch = state.epoch.load(Ordering::SeqCst);
    let bytes = with_runtime_bound(&app, &account_scope, |r| {
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": Utc::now(),
            "activity": r.store.events(),
            "repeatedTasks": r.store.pending_candidates(Utc::now()),
            "reviewedTasks": r.store.workflows(),
        }))
        .map_err(|_| "Could not prepare local history".into())
    })
    .await?;
    let dialog = app
        .dialog()
        .file()
        .set_file_name("sanka-monitor-history.json")
        .add_filter("JSON", &["json"]);
    let Some(chosen) = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|_| "Save dialog unavailable")?
    else {
        return Ok(false);
    };
    let path = chosen.into_path().map_err(|_| "Choose a local file")?;
    with_runtime_bound(&app, &account_scope, move |_| {
        write_history_export(&path, &bytes, &state.epoch, epoch)?;
        Ok(true)
    })
    .await
}

fn write_history_export(
    path: &std::path::Path,
    bytes: &[u8],
    epoch: &AtomicU64,
    expected_epoch: u64,
) -> Result<(), String> {
    use std::io::Write;
    if epoch.load(Ordering::SeqCst) != expected_epoch {
        return Err("History changed; export again".into());
    }
    let temporary = path.with_file_name(format!(".sanka-monitor-history-{}.tmp", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Could not create local history file")?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if epoch.load(Ordering::SeqCst) != expected_epoch {
            return Err(std::io::Error::other("History changed"));
        }
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|_| "Could not save history; review and export again".to_string())
}

async fn settings_operation<P, A, F>(
    state: &ContextState,
    prepare: P,
    apply: A,
) -> Result<(), String>
where
    P: std::future::Future<Output = Result<(), String>>,
    A: FnOnce(u64) -> F,
    F: std::future::Future<Output = Result<(), String>>,
{
    let generation = state.begin_action();
    prepare.await?;
    if state.action_generation.load(Ordering::SeqCst) != generation {
        return Err("Session changed; explicitly resume again".into());
    }
    apply(generation).await
}
fn activate_settings(
    state: &ContextState,
    collector: &mut Collector,
    action: u64,
    session: u64,
    active: bool,
) -> Result<(), String> {
    if state.action_generation.load(Ordering::SeqCst) != action {
        return Err("Session changed; explicitly resume again".into());
    }
    collector.set_session_active(active);
    if active
        && (!state.gate.activate_explicitly(session)
            || state.action_generation.load(Ordering::SeqCst) != action)
    {
        collector.set_session_active(false);
        state.pause_for_preparation();
        return Err("Session changed; explicitly resume again".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn set_context_settings(
    app: AppHandle,
    settings: ContextSettings,
    own_os_login: bool,
    account_scope: String,
) -> Result<(), String> {
    let state = app.state::<ContextState>().inner().clone();
    let selection = settings.clone();
    let prepare = async {
        let settings = &selection;
        if !own_os_login && settings.enabled {
            return Err("A shared OS login supports inventory only".into());
        }
        if settings.allowed_app_ids.len() > 30
            || settings.allowed_app_ids.iter().any(|id| {
                !matches!(
                    id.as_str(),
                    "com.apple.TextEdit" | "notepad.exe" | "web:google-sheets"
                )
            })
            || settings
                .allowed_domains
                .iter()
                .any(|d| d != "docs.google.com")
        {
            return Err("Unsupported context selection".into());
        }
        revoke_recurring(&app, Some(&account_scope)).await
    };
    settings_operation(&state, prepare, |action| {
        let state = state.clone();
        let app = app.clone();
        let account_scope = account_scope.clone();
        async move {
            let permitted = {
                let c = app.state::<AgentState>().config.lock().await.clone();
                !c.kill_switch && !c.update_required()
            };
            let state = state.clone();
            with_runtime_bound(&app, &account_scope, move |r| {
                let session = state.gate.generation();
                if state.action_generation.load(Ordering::SeqCst) != action {
                    return Err("Session changed; explicitly resume again".into());
                }
                r.invalidate();
                r.settings = settings;
                r.personal_os_login = own_os_login;
                let active = r.settings.enabled
                    && !r.settings.paused
                    && own_os_login
                    && state.session_ready.load(Ordering::SeqCst)
                    && permitted
                    && session_available();
                if r.settings.enabled && !active {
                    r.settings.paused = true;
                }
                r.check_key()?;
                if let Err(error) =
                    activate_settings(&state, &mut r.collector, action, session, active)
                {
                    r.settings.paused = true;
                    return Err(error);
                }
                Ok(())
            })
            .await
        }
    })
    .await
}
#[tauri::command]
pub async fn delete_context_history(
    app: AppHandle,
    ids: Option<Vec<Uuid>>,
    workflow_id: Option<Uuid>,
    account_scope: String,
) -> Result<(), String> {
    let state = app.state::<ContextState>().inner().clone();
    let operation = state.begin_action();
    let remote = revoke_recurring(&app, Some(&account_scope)).await;
    if identity(&app).await.scope != account_scope
        || state.action_generation.load(Ordering::SeqCst) != operation
    {
        return Err("Account or session changed; retry local deletion".into());
    }
    if ids.is_none() && workflow_id.is_none() {
        let state = app.state::<ContextState>().inner().clone();
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "App directory unavailable")?
            .join("context");
        tauri::async_runtime::spawn_blocking(move || {
            erase_local_runtime(&state, &dir, &account_scope, operation)
        })
        .await
        .map_err(|_| "Context worker unavailable")??;
    } else {
        with_runtime_bound(&app, &account_scope, move |r| {
            r.invalidate();
            r.settings.paused = true;
            if let Some(id) = workflow_id {
                r.store.delete_workflow(id)
            } else {
                r.store.delete(&ids.unwrap_or_default()).map(|_| ())
            }
            .map_err(|e| e.to_string())
        })
        .await?;
    }
    remote.map_err(|_| "Local context deleted; remote revocation remains unconfirmed".into())
}
fn save_workflow_scoped(
    runtime: &mut Runtime,
    scope: &str,
    workflow: ContextWorkflow,
) -> Result<(), String> {
    if runtime.identity.scope != scope {
        return Err("Account changed; reopen the workflow in the current account".into());
    }
    runtime.invalidate();
    runtime
        .store
        .save_workflow(workflow)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn save_context_workflow(
    app: AppHandle,
    workflow: ContextWorkflow,
    account_scope: String,
) -> Result<(), String> {
    app.state::<ContextState>()
        .epoch
        .fetch_add(1, Ordering::SeqCst);
    let remote = revoke_recurring(&app, Some(&account_scope)).await;
    with_runtime(&app, move |r| {
        save_workflow_scoped(r, &account_scope, workflow)
    })
    .await?;
    remote.map_err(|_| "Description saved locally; remote revocation remains unconfirmed".into())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    app_ids: Vec<String>,
    episode_ids: Vec<Uuid>,
    workflow_ids: Vec<Uuid>,
    audience: Audience,
    include_scan: bool,
}
fn package(
    r: &Runtime,
    selection: &Selection,
    scan: Option<ReportEvidence>,
) -> Result<ContextPackage, String> {
    let now = Utc::now();
    let mut episodes: Vec<_> = r
        .store
        .candidates()
        .into_iter()
        .flat_map(|c| c.episodes)
        .filter(|e| selection.episode_ids.contains(&e.id))
        .collect();
    episodes.sort_by_key(|e| e.id);
    episodes.dedup_by_key(|e| e.id);
    if episodes.len() != selection.episode_ids.len() {
        return Err("Episode evidence changed; review again".into());
    }
    let mut workflows: Vec<_> = r
        .store
        .workflows()
        .into_iter()
        .filter(|w| selection.workflow_ids.contains(&w.id))
        .collect();
    workflows.sort_by_key(|w| w.id);
    if workflows.len() != selection.workflow_ids.len() {
        return Err("Workflow selection changed".into());
    }
    for w in &mut workflows {
        w.episode_ids
            .retain(|id| selection.episode_ids.contains(id));
    }
    let mut apps = BTreeMap::<String, ContextApp>::new();
    let events = r.store.events();
    for id in &selection.app_ids {
        let known_name = match id.as_str() {
            "web:google-sheets" => "Google Sheets",
            "com.apple.TextEdit" => "TextEdit",
            "notepad.exe" => "Notepad",
            _ => return Err("Unknown app selection".into()),
        };
        let days = events
            .iter()
            .filter(|e| e.app_id == *id)
            .map(|e| e.at.date_naive())
            .collect::<BTreeSet<_>>()
            .len() as u8;
        apps.insert(
            id.clone(),
            ContextApp {
                app_id: id.clone(),
                name: known_name.into(),
                service_key: (id == "web:google-sheets").then(|| "google-workspace".into()),
                observed_days: days,
            },
        );
    }
    let days = episodes.iter().map(|e| e.day).collect::<BTreeSet<_>>();
    let from = days
        .first()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc())
        .unwrap_or(now);
    let mut gaps: Vec<_> = events.iter().filter_map(|e| e.gap).collect();
    gaps.sort_by_key(|g| format!("{g:?}"));
    gaps.dedup();
    if episodes.is_empty() && !gaps.contains(&Gap::InsufficientEvidence) {
        gaps.push(Gap::InsufficientEvidence);
    }
    let package = ContextPackage {
        schema_version: 1,
        package_id: Uuid::new_v4(),
        workspace_id: r
            .identity
            .workspace
            .ok_or("Sign in again to bind a workspace")?,
        account_device_id: r.identity.device.ok_or("Sign in first")?,
        audience: selection.audience,
        policy_version: 1,
        captured_from: from,
        captured_to: now,
        consented_at: now,
        coverage: ContextCoverage {
            observed_days: days.len() as u8,
            gaps,
        },
        apps: apps.into_values().collect(),
        episodes,
        workflows,
        scan,
    };
    package.validate(now).map_err(|e| e.to_string())?;
    Ok(package)
}
#[tauri::command]
pub async fn prepare_context_package(
    app: AppHandle,
    selection: Selection,
    own_os_login: bool,
    account_scope: String,
) -> Result<Preview, String> {
    app.state::<ContextState>()
        .epoch
        .fetch_add(1, Ordering::SeqCst);
    if !own_os_login {
        return Err("Shared OS logins support inventory only".into());
    }
    let scan = if selection.include_scan {
        let report = app
            .state::<AgentState>()
            .last_scan
            .lock()
            .await
            .clone()
            .ok_or("Run a scan first")?;
        Some(
            serde_json::from_value(
                serde_json::to_value(monitor_report::evidence::review_evidence(&report, &[]))
                    .map_err(|_| "Invalid scan")?,
            )
            .map_err(|_| "Unsupported scan evidence")?,
        )
    } else {
        None
    };
    with_runtime_bound(&app, &account_scope, move |r| {
        r.invalidate();
        r.personal_os_login = own_os_login;
        let p = package(r, &selection, scan)?;
        let preview = r.authority.freeze(
            &r.identity.scope,
            &p.to_json(Utc::now()).map_err(|e| e.to_string())?,
        );
        r.preview = Some(preview.clone());
        Ok(preview)
    })
    .await
}
async fn identity(app: &AppHandle) -> Identity {
    Identity::from_config(&app.state::<AgentState>().config.lock().await.clone())
}
async fn request<T: DeserializeOwned>(
    app: &AppHandle,
    id: &Identity,
    epoch: u64,
    route: ContextRoute,
    body: Option<serde_json::Value>,
) -> Result<T, String> {
    let state = app.state::<ContextState>().inner().clone();
    for attempt in 0..2 {
        if state.epoch.load(Ordering::SeqCst) != epoch || identity(app).await != *id {
            return Err("Consent or account changed; delivery unconfirmed".into());
        }
        let config = app.state::<AgentState>().config.lock().await.clone();
        if !matches!(
            route,
            ContextRoute::Revoke | ContextRoute::RevokeSummaries | ContextRoute::DeletePackage(_)
        ) && (config.kill_switch || config.update_required())
        {
            return Err("Workspace policy paused context sharing".into());
        }
        let token = account::access_token(app)
            .await
            .map_err(|_| "Sign in to share context")?;
        if state.epoch.load(Ordering::SeqCst) != epoch || identity(app).await != *id {
            return Err("Account changed".into());
        }
        if matches!(route, ContextRoute::Packages | ContextRoute::SummaryGrants) {
            let scope = id.scope.clone();
            with_runtime(app, move |r| {
                let preview = r.preview.as_ref().ok_or("Preview was cancelled")?;
                if r.revocation_pending()
                    || !r.personal_os_login
                    || !r.authority.permits(preview, &scope)
                {
                    return Err("Consent changed; delivery unconfirmed".into());
                }
                sources_unchanged(r, preview)
            })
            .await?;
        }
        if matches!(route, ContextRoute::Summaries) {
            let allowed = state
                .runtime
                .lock()
                .map_err(|_| "Context unavailable")?
                .as_ref()
                .is_some_and(|r| {
                    !r.revocation_pending()
                        && r.recurring.as_ref().is_some_and(|g| {
                            g.selection_generation
                                == state.sharing_generation.load(Ordering::SeqCst)
                                && body
                                    .as_ref()
                                    .and_then(|b| b.get("grantId"))
                                    .and_then(|v| v.as_str())
                                    == Some(g.grant_id.to_string().as_str())
                        })
                });
            if !allowed {
                return Err("Daily sharing was revoked".into());
            }
        }
        if state.epoch.load(Ordering::SeqCst) != epoch {
            return Err("Consent changed; delivery unconfirmed".into());
        }
        let client = AgentClient::new(&id.api, AGENT_VERSION).map_err(|_| "Cloud unavailable")?;
        let future = client.context_request::<T>(&token, route, body.as_ref());
        tokio::pin!(future);
        let response = loop {
            tokio::select! {result=&mut future=>break result,
                _=tokio::time::sleep(Duration::from_millis(100))=>{
                    if state.epoch.load(Ordering::SeqCst)!=epoch || identity(app).await!=*id {return Err("Consent changed; delivery unconfirmed. Delete shared context to confirm removal.".into());}
                }
            }
        };
        if state.epoch.load(Ordering::SeqCst) != epoch || identity(app).await != *id {
            return Err("Consent changed; delivery unconfirmed".into());
        }
        match response {
            Ok(value) => return Ok(value),
            Err(e) if e.token_rejected() && attempt == 0 => account::forget_token(app).await,
            Err(e) if e.signed_out() => {
                state.pause();
                let _ = account::forget_account(app).await;
                return Err("Account access revoked; delivery unconfirmed".into());
            }
            Err(_) => {
                return Err(
                    "Delivery unconfirmed or server unsupported; data remains available locally"
                        .into(),
                )
            }
        }
    }
    Err("Cloud unavailable".into())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Capabilities {
    schema_versions: Vec<u8>,
    policy_version: u8,
    max_bytes: usize,
}
async fn capability(app: &AppHandle, id: &Identity, epoch: u64) -> Result<(), String> {
    let c: Capabilities = request(app, id, epoch, ContextRoute::Capabilities, None).await?;
    if !c.schema_versions.contains(&1) || c.policy_version != 1 || c.max_bytes < MAX_PACKAGE_BYTES {
        return Err("Context sharing is not supported by this server; kept local".into());
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Grant {
    grant_id: Uuid,
    expires_at: DateTime<Utc>,
}
fn valid_result(id: &Identity, receipt: &ContextReceipt) -> bool {
    let Ok(base) = url::Url::parse(&id.origin) else {
        return false;
    };
    let Ok(result) = url::Url::parse(&receipt.result_url) else {
        return false;
    };
    result.origin() == base.origin()
        && result.username().is_empty()
        && result.password().is_none()
        && result.fragment().is_none()
        && id
            .code
            .as_ref()
            .is_some_and(|code| result.path() == format!("/{code}/assessments"))
        && result.query_pairs().collect::<Vec<_>>()
            == vec![(
                "assessmentId".into(),
                receipt.assessment_id.to_string().into(),
            )]
}
fn sources_unchanged(runtime: &Runtime, preview: &Preview) -> Result<(), String> {
    let p =
        ContextPackage::from_json(&preview.package_json, Utc::now()).map_err(|e| e.to_string())?;
    let episodes: Vec<_> = runtime
        .store
        .candidates()
        .into_iter()
        .flat_map(|c| c.episodes)
        .collect();
    if p.episodes.iter().any(|e| !episodes.contains(e)) {
        return Err("Episode evidence changed; prepare a new preview".into());
    }
    let workflows = runtime.store.workflows();
    for expected in &p.workflows {
        let Some(mut actual) = workflows.iter().find(|w| w.id == expected.id).cloned() else {
            return Err("Workflow removed; prepare a new preview".into());
        };
        actual
            .episode_ids
            .retain(|id| p.episodes.iter().any(|e| e.id == *id));
        if actual != *expected {
            return Err("Workflow evidence changed; prepare a new preview".into());
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn send_context_package(app: AppHandle, token: String) -> Result<ContextReceipt, String> {
    let state = app.state::<ContextState>().inner().clone();
    let _sending = state
        .send_lock
        .try_lock()
        .map_err(|_| "A context request is already running")?;
    let id = identity(&app).await;
    let scope = id.scope.clone();
    let preview = with_runtime(&app, move |r| {
        let p = r.preview.as_ref().ok_or("Prepare a new preview")?;
        if r.revocation_pending()
            || !r.personal_os_login
            || p.token != token
            || !r.authority.permits(p, &scope)
        {
            return Err("Preview is no longer authorized".into());
        }
        sources_unchanged(r, p)?;
        Ok(p.clone())
    })
    .await?;
    let epoch = state.epoch.load(Ordering::SeqCst);
    let package =
        ContextPackage::from_json(&preview.package_json, Utc::now()).map_err(|e| e.to_string())?;
    capability(&app, &id, epoch).await?;
    let grant:Grant=request(&app,&id,epoch,ContextRoute::Grants,Some(serde_json::json!({"packageId":package.package_id,"workspaceId":package.workspace_id,"accountDeviceId":package.account_device_id,"audience":package.audience,"policyVersion":1,"sha256":preview.sha256}))).await?;
    if grant.expires_at <= Utc::now()
        || grant.expires_at
            > Utc::now() + chrono::Duration::hours(24) + chrono::Duration::minutes(5)
    {
        return Err("Invalid grant expiry".into());
    }
    let receipt:ContextReceipt=request(&app,&id,epoch,ContextRoute::Packages,Some(serde_json::json!({"grantId":grant.grant_id,"packageJson":preview.package_json,"sha256":preview.sha256}))).await?;
    if receipt.package_id != package.package_id
        || !preview.receipt_matches(&receipt.sha256)
        || !valid_result(&id, &receipt)
    {
        return Err("Receipt mismatch; delivery unconfirmed".into());
    }
    Ok(receipt)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Deleted {
    deleted: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revoked {
    revoked: bool,
}
#[tauri::command]
pub async fn delete_shared_context(
    app: AppHandle,
    package_id: Option<Uuid>,
    account_scope: String,
) -> Result<(), String> {
    app.state::<ContextState>().pause();
    delete_shared_context_inner(app, package_id, account_scope).await
}
async fn delete_shared_context_inner(
    app: AppHandle,
    package_id: Option<Uuid>,
    account_scope: String,
) -> Result<(), String> {
    let state = app.state::<ContextState>().inner().clone();
    state.pause_for_preparation();
    let id = with_runtime_bound(&app, &account_scope, move |r| {
        r.invalidate();
        r.settings.paused = true;
        if package_id.is_none() {
            r.recurring = None;
            r.require_revocation(RevocationScope::All);
        }
        Ok(r.identity.clone())
    })
    .await?;
    let epoch = state.epoch.load(Ordering::SeqCst);
    if let Some(package_id) = package_id {
        let result: Deleted = request(
            &app,
            &id,
            epoch,
            ContextRoute::DeletePackage(package_id),
            None,
        )
        .await?;
        if !result.deleted {
            return Err("Remote deletion unconfirmed".into());
        }
    } else {
        let result: Revoked = request(&app, &id, epoch, ContextRoute::Revoke, None).await?;
        if !result.revoked {
            return Err("Remote revocation unconfirmed".into());
        }
        with_runtime_bound(&app, &account_scope, move |r| {
            if r.identity != id {
                return Err("Account changed".into());
            }
            r.recurring = None;
            r.acknowledge_revocation(RevocationScope::All);
            Ok(())
        })
        .await?;
    }
    Ok(())
}
async fn revoke_recurring(app: &AppHandle, expected_scope: Option<&str>) -> Result<(), String> {
    let state = app.state::<ContextState>().inner().clone();
    let scope = {
        let slot = state.runtime.lock().map_err(|_| "Context unavailable")?;
        if slot
            .as_ref()
            .is_some_and(|r| expected_scope.is_some_and(|s| s != r.identity.scope))
        {
            return Err("Account changed".into());
        }
        slot.as_ref()
            .filter(|r| r.recurring.is_some() || r.revocation_pending())
            .map(|r| r.identity.scope.clone())
    };
    if let Some(scope) = scope {
        revoke_selection(app, &state, &scope).await?;
    }
    Ok(())
}
async fn revoke_selection(
    app: &AppHandle,
    state: &ContextState,
    scope: &str,
) -> Result<(), String> {
    invalidate_selection_operation(state, scope, |identity| async move {
        let epoch = state.epoch.load(Ordering::SeqCst);
        let result: Revoked =
            request(app, &identity, epoch, ContextRoute::RevokeSummaries, None).await?;
        if !result.revoked {
            return Err("Remote revocation unconfirmed".into());
        }
        Ok(())
    })
    .await
}
// Matches Python json.dumps(sort_keys=True, ensure_ascii=False, separators=(",",":"))
// for this closed schema's bounded optional f64 metrics. All other numbers are integers.
fn summary_hash(package: &ContextPackage) -> Result<String, String> {
    fn canonical(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(map) => {
                let sorted: BTreeMap<_, _> = map.iter().collect();
                format!(
                    "{{{}}}",
                    sorted
                        .iter()
                        .map(|(k, v)| format!(
                            "{}:{}",
                            serde_json::to_string(k).unwrap(),
                            canonical(v)
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
            serde_json::Value::Array(items) => format!(
                "[{}]",
                items.iter().map(canonical).collect::<Vec<_>>().join(",")
            ),
            serde_json::Value::Number(number) if number.is_f64() => {
                let n = number.as_f64().unwrap();
                if n != 0.0 && n.abs() < 0.0001 {
                    let scientific = format!("{n:e}");
                    let (mantissa, exponent) = scientific.split_once('e').unwrap();
                    let exponent: i32 = exponent.parse().unwrap();
                    format!("{mantissa}e{exponent:+03}")
                } else {
                    number.to_string()
                }
            }
            _ => value.to_string(),
        }
    }
    let selection = serde_json::json!({"apps":package.apps,"workflows":package.workflows});
    Ok(hash(canonical(&selection).as_bytes()))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SummaryGrant {
    grant_id: Uuid,
    selection_sha256: String,
}
#[tauri::command]
pub async fn enable_context_summary(
    app: AppHandle,
    token: String,
    own_os_login: bool,
    account_scope: String,
) -> Result<(), String> {
    if !own_os_login {
        return Err("Shared OS logins support inventory only".into());
    }
    let state = app.state::<ContextState>().inner().clone();
    let selection_generation = state.sharing_generation.load(Ordering::SeqCst);
    let _lock = state
        .send_lock
        .try_lock()
        .map_err(|_| "A context request is already running")?;
    revoke_recurring(&app, Some(&account_scope)).await?;
    let id = identity(&app).await;
    let scope = id.scope.clone();
    let preview = with_runtime_bound(&app, &account_scope, move |r| {
        let p = r
            .preview
            .as_ref()
            .ok_or("Preview the daily selection first")?;
        if r.revocation_pending()
            || !r.personal_os_login
            || p.token != token
            || !r.authority.permits(p, &scope)
        {
            return Err("Preview changed".into());
        }
        sources_unchanged(r, p)?;
        Ok(p.clone())
    })
    .await?;
    let p =
        ContextPackage::from_json(&preview.package_json, Utc::now()).map_err(|e| e.to_string())?;
    if p.audience != Audience::Company
        || !p.episodes.is_empty()
        || p.scan.is_some()
        || p.workflows.iter().any(|w| !w.episode_ids.is_empty())
    {
        return Err("Daily summaries require company audience and no episodes or scan".into());
    }
    let epoch = state.epoch.load(Ordering::SeqCst);
    capability(&app, &id, epoch).await?;
    let grant:SummaryGrant=request(&app,&id,epoch,ContextRoute::SummaryGrants,Some(serde_json::json!({"workspaceId":p.workspace_id,"accountDeviceId":p.account_device_id,"policyVersion":1,"apps":p.apps,"workflows":p.workflows,"sharedOsLogin":false}))).await?;
    if grant.selection_sha256 != summary_hash(&p)?
        || grant.selection_sha256.len() != 64
        || !grant
            .selection_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid summary grant".into());
    }
    let approval_state = state.clone();
    with_runtime_bound(&app, &account_scope, move |r| {
        if approval_state.sharing_generation.load(Ordering::SeqCst) != selection_generation
            || r.identity != id
            || !r.authority.permits(&preview, &id.scope)
        {
            return Err("Selection changed; revoke shared context".into());
        }
        r.recurring = Some(Recurring {
            selection_generation,
            grant_id: grant.grant_id,
            selection_sha256: grant.selection_sha256,
            day: None,
            identity: id,
        });
        Ok(())
    })
    .await
}
#[tauri::command]
pub async fn open_context_result(app: AppHandle, receipt: ContextReceipt) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !valid_result(&identity(&app).await, &receipt) {
        return Err("Invalid assessment destination".into());
    }
    app.opener()
        .open_url(receipt.result_url, None::<&str>)
        .map_err(|_| "Could not open assessment".into())
}
/// No persisted queue: a normalized browser observation must pass all current vetoes.
pub fn ingest_browser_observation(state: &ContextState, action: Action) -> bool {
    if !state.gate.is_active() || !session_available() {
        state.pause();
        return false;
    }
    let Ok(mut slot) = state.runtime.try_lock() else {
        return false;
    };
    let Some(r) = slot.as_mut() else { return false };
    if !r.settings.allows_app("web:google-sheets")
        || !r
            .settings
            .allowed_domains
            .iter()
            .any(|d| d == "docs.google.com")
        || r.last_browser
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
    {
        return false;
    }
    if r.check_key().is_err() {
        state.pause();
        return false;
    }
    if !state.gate.is_active() || !session_available() {
        state.pause();
        return false;
    }
    r.last_browser = Some(Instant::now());
    r.store
        .append(ContextEvent {
            id: Uuid::new_v4(),
            at: Utc::now(),
            app_id: "web:google-sheets".into(),
            app_name: "Google Sheets".into(),
            action,
            object_category: ObjectCategory::Spreadsheet,
            provenance: Provenance::Inferred,
            gap: None,
        })
        .is_ok()
}
pub fn browser_status(state: &ContextState, pause: bool) -> bool {
    if pause {
        if let Ok(mut slot) = state.runtime.try_lock() {
            if let Some(r) = slot.as_mut() {
                r.last_browser = None;
            }
        }
        return false;
    }
    state.gate.is_active()
        && session_available()
        && state.runtime.try_lock().ok().is_some_and(|slot| {
            slot.as_ref().is_some_and(|r| {
                r.settings.allows_app("web:google-sheets")
                    && r.settings
                        .allowed_domains
                        .iter()
                        .any(|d| d == "docs.google.com")
            })
        })
}
pub fn start(app: AppHandle) {
    install_session_notifications(app.state::<ContextState>().inner().clone());
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last = Instant::now();
        let mut ticks = 0u64;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let state = handle.state::<ContextState>().inner().clone();
            if state.gate.is_active()
                && (last.elapsed() > Duration::from_secs(2) || !session_available())
            {
                state.pause();
            }
            last = Instant::now();
            let config = handle.state::<AgentState>().config.lock().await.clone();
            let id = Identity::from_config(&config);
            if state.gate.is_active() && (config.kill_switch || config.update_required()) {
                state.pause();
            }
            if let Ok(mut slot) = state.runtime.try_lock() {
                if let Some(r) = slot.as_mut() {
                    if r.identity != id {
                        state.pause();
                        let _ = state.retire_runtime(&mut slot);
                    }
                }
            }
            ticks += 1;
            if ticks == 1 || ticks.is_multiple_of(240) {
                if let Ok(dir) = handle.path().app_data_dir() {
                    let current = state
                        .runtime
                        .lock()
                        .ok()
                        .and_then(|r| r.as_ref().map(|r| r.identity.scope.clone()));
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        prune_other_scopes(&dir.join("context"), current.as_deref())
                    })
                    .await;
                    if !matches!(result, Ok(Ok(()))) {
                        if let Ok(mut slot) = state.runtime.lock() {
                            if let Some(r) = slot.as_mut() {
                                r.error=Some("Could not prune a previous account's encrypted history; erase local context or restore secure storage".into());
                            }
                        }
                    }
                }
            }
            if ticks.is_multiple_of(4) {
                let worker = state.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    let Ok(mut slot) = worker.runtime.try_lock() else {
                        return;
                    };
                    let Some(r) = slot.as_mut() else { return };
                    if ticks.is_multiple_of(240) {
                        let generation = r.authority.generation;
                        if let Err(e) = r.prune() {
                            r.error = Some(e);
                            worker.pause();
                        }
                        if r.authority.generation != generation {
                            worker.epoch.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    if !worker.gate.is_active() {
                        return;
                    }
                    if r.check_key().is_err() {
                        worker.pause();
                        r.error = Some("Secure storage unavailable".into());
                        return;
                    }
                    match r.collector.observe(&r.settings) {
                        Ok(Some(event)) if worker.gate.is_active() && session_available() => {
                            if let Err(e) = r.store.append(event) {
                                r.error = Some(e.to_string());
                                worker.pause();
                            }
                        }
                        Err(e) => {
                            r.error = Some(e.to_string());
                            worker.pause();
                        }
                        _ => {}
                    }
                    let gap = r.collector.last_gap();
                    if gap == Some(Gap::Locked) {
                        worker.pause();
                    }
                    if gap != r.last_gap {
                        r.last_gap = gap;
                        if let Some(gap) = gap {
                            let _ = r.store.append(ContextEvent::coverage_gap(Utc::now(), gap));
                        }
                    }
                })
                .await;
            }
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let state = app.state::<ContextState>().inner().clone();
            let Ok(_lock) = state.send_lock.try_lock() else {
                continue;
            };
            let day = Utc::now().date_naive();
            let Some(recurring) = next_summary(&state, day) else {
                continue;
            };
            let epoch = state.epoch.load(Ordering::SeqCst);
            // Coverage of the frozen summary selection is conservative; no fresh apps/episodes enter it.
            let result:Result<ContextReceipt,String>=request(&app,&recurring.identity,epoch,ContextRoute::Summaries,Some(serde_json::json!({"grantId":recurring.grant_id,"day":day,"coverage":{"observedDays":0,"gaps":["insufficient_evidence"]}}))).await;
            if let Ok(mut slot) = state.runtime.lock() {
                if let Some(r) = slot.as_mut() {
                    if r.recurring.as_ref().is_some_and(|g| {
                        g.grant_id == recurring.grant_id
                            && g.selection_sha256 == recurring.selection_sha256
                    }) {
                        match result {
                            Ok(receipt) if valid_result(&recurring.identity, &receipt) => {
                                if let Some(g) = r.recurring.as_mut() {
                                    g.day = Some(day);
                                }
                                r.error = None;
                            }
                            _ => r.error = Some("Daily summary delivery unconfirmed".into()),
                        }
                    }
                }
            };
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic_runtime(path: &std::path::Path, identity: Identity) -> Runtime {
        Runtime {
            identity,
            store: ContextStore::open(path, [17; 32]).unwrap(),
            settings: ContextSettings::default(),
            collector: Collector::default(),
            authority: Authority::default(),
            preview: None,
            recurring: None,
            last_gap: None,
            last_browser: None,
            error: None,
            personal_os_login: true,
            key_fingerprint: hash(&[17; 32]),
            pending_revocation: None,
        }
    }
    #[test]
    fn local_export_is_private_atomic_and_rejects_stale_snapshots() {
        let directory = std::env::temp_dir().join(format!("monitor-export-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("history.json");
        let unrelated = directory.join("history.json.tmp");
        std::fs::write(&path, b"original").unwrap();
        std::fs::write(&unrelated, b"unrelated").unwrap();
        let epoch = AtomicU64::new(4);
        assert!(write_history_export(&path, b"stale", &epoch, 3).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        write_history_export(&path, b"{\"activity\":[]}", &epoch, 4).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"activity\":[]}");
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let target_directory = directory.join("preserved");
        std::fs::create_dir(&target_directory).unwrap();
        assert!(
            write_history_export(&target_directory, b"cannot replace a directory", &epoch, 4)
                .is_err()
        );
        assert!(target_directory.is_dir());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 3);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn final_pending_revocation_survives_full_local_erasure() {
        for pending in [RevocationScope::All, RevocationScope::Summaries] {
            let dir =
                std::env::temp_dir().join(format!("context-erasure-intent-{}", Uuid::new_v4()));
            let state = ContextState::default();
            let identity = Identity::from_config(&AgentConfig::default());
            let scope = identity.scope.clone();
            let path = dir.join(format!("{scope}.bin"));
            let mut runtime = synthetic_runtime(&path, identity.clone());
            runtime
                .store
                .append(ContextEvent::coverage_gap(Utc::now(), Gap::Unsupported))
                .unwrap();
            runtime.require_revocation(pending);
            *state.runtime.lock().unwrap() = Some(runtime);
            let operation = state.begin_action();
            let summary = invalidate_selection_operation(&state, &scope, |_| async {
                if pending == RevocationScope::All {
                    Ok(())
                } else {
                    Err("synthetic offline".into())
                }
            })
            .await;
            assert_eq!(summary.is_ok(), pending == RevocationScope::All);
            erase_local_runtime(&state, &dir, &scope, operation).unwrap();
            assert!(!path.exists());
            // Another account cannot acquire the retained intent.
            let mut other = identity.clone();
            other.scope = "other-account".into();
            let unrelated = state
                .restore_runtime(synthetic_runtime(&dir.join("other.bin"), other))
                .unwrap();
            assert!(!unrelated.revocation_pending());
            drop(unrelated);
            // Same restore boundary used by get_context_history's fresh runtime initialization.
            let restored = state
                .restore_runtime(synthetic_runtime(&path, identity))
                .unwrap();
            assert_eq!(
                restored.pending_revocation,
                Some(pending),
                "erasure cannot erase unconfirmed cloud intent"
            );
            assert!(restored.error.is_some());
            assert!(restored.revocation_pending());
            *state.runtime.lock().unwrap() = Some(restored);
            {
                let mut slot = state.runtime.lock().unwrap();
                slot.as_mut().unwrap().acknowledge_revocation(pending);
                state.retire_runtime(&mut slot).unwrap();
            }
            let clean = state
                .restore_runtime(synthetic_runtime(
                    &path,
                    Identity::from_config(&AgentConfig::default()),
                ))
                .unwrap();
            assert!(!clean.revocation_pending());
            drop(clean);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[tokio::test]
    async fn final_sign_out_fences_scheduler_while_remote_revoke_is_pending() {
        let dir = std::env::temp_dir().join(format!("context-sign-out-{}", Uuid::new_v4()));
        let state = ContextState::default();
        let identity = Identity::from_config(&AgentConfig::default());
        let mut runtime = synthetic_runtime(&dir.join("synthetic.bin"), identity.clone());
        runtime.recurring = Some(Recurring {
            selection_generation: 0,
            grant_id: Uuid::new_v4(),
            selection_sha256: "synthetic".into(),
            day: None,
            identity,
        });
        *state.runtime.lock().unwrap() = Some(runtime);
        let day = Utc::now().date_naive();
        let snapshot = next_summary(&state, day).unwrap();
        let busy = state.runtime.lock().unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, waiting) = tokio::sync::oneshot::channel();
        let signing_out = sign_out_context(&state, async {
            started.send(()).unwrap();
            waiting.await.unwrap();
        });
        let scheduler = async {
            ready.await.unwrap();
            drop(busy);
            let no_new_request = next_summary(&state, day).is_none();
            let stale_snapshot =
                snapshot.selection_generation != state.sharing_generation.load(Ordering::SeqCst);
            release.send(()).unwrap();
            (no_new_request, stale_snapshot)
        };
        let ((), (no_new_request, stale_snapshot)) = tokio::join!(signing_out, scheduler);
        assert!(
            no_new_request,
            "no daily request may start during sign-out's remote wait"
        );
        assert!(stale_snapshot);
        *state.runtime.lock().unwrap() = None;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn summary_ack_cannot_clear_a_failed_explicit_revoke_all() {
        let dir = std::env::temp_dir().join(format!("context-revoke-scope-{}", Uuid::new_v4()));
        let state = ContextState::default();
        let identity = Identity::from_config(&AgentConfig::default());
        let scope = identity.scope.clone();
        let mut runtime = synthetic_runtime(&dir.join("synthetic.bin"), identity);
        // Same pending state left by an explicit revoke-all whose request failed.
        runtime.require_revocation(RevocationScope::All);
        *state.runtime.lock().unwrap() = Some(runtime);
        invalidate_selection_operation(&state, &scope, |_| async { Ok(()) })
            .await
            .unwrap();
        let slot = state.runtime.lock().unwrap();
        let r = slot.as_ref().unwrap();
        assert!(
            r.revocation_pending(),
            "a summary-only acknowledgment cannot confirm revoke-all"
        );
        assert!(
            r.error.is_some(),
            "the outstanding all-grants warning must remain visible"
        );
        assert_eq!(r.pending_revocation, Some(RevocationScope::All));
        drop(slot);
        {
            let mut slot = state.runtime.lock().unwrap();
            let r = slot.as_mut().unwrap();
            r.acknowledge_revocation(RevocationScope::All);
            assert!(!r.revocation_pending());
            assert!(r.error.is_none());
        }
        *state.runtime.lock().unwrap() = None;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn automatic_summary_revocation_never_promotes_to_revoke_all() {
        let dir = std::env::temp_dir().join(format!("context-summary-only-{}", Uuid::new_v4()));
        let state = ContextState::default();
        let identity = Identity::from_config(&AgentConfig::default());
        let scope = identity.scope.clone();
        let mut r = synthetic_runtime(&dir.join("synthetic.bin"), identity.clone());
        r.recurring = Some(Recurring {
            selection_generation: 0,
            grant_id: Uuid::new_v4(),
            selection_sha256: "synthetic".into(),
            day: None,
            identity,
        });
        *state.runtime.lock().unwrap() = Some(r);
        assert!(invalidate_selection_operation(&state, &scope, |_| async {
            Err("synthetic offline".into())
        })
        .await
        .is_err());
        {
            let slot = state.runtime.lock().unwrap();
            let r = slot.as_ref().unwrap();
            assert_eq!(r.pending_revocation, Some(RevocationScope::Summaries));
            assert!(r.revocation_pending());
        }
        invalidate_selection_operation(&state, &scope, |_| async { Ok(()) })
            .await
            .unwrap();
        {
            let mut slot = state.runtime.lock().unwrap();
            let r = slot.as_mut().unwrap();
            assert!(!r.revocation_pending());
            assert!(r.error.is_none());
            // An explicitly stronger acknowledgement also covers a summary-only request.
            r.require_revocation(RevocationScope::Summaries);
            r.acknowledge_revocation(RevocationScope::All);
            assert!(!r.revocation_pending());
        }
        *state.runtime.lock().unwrap() = None;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn delayed_old_account_save_cannot_copy_a_workflow_into_the_new_store() {
        let dir = std::env::temp_dir().join(format!("context-account-save-{}", Uuid::new_v4()));
        let state = ContextState::default();
        let old = Identity::from_config(&AgentConfig::default());
        let scope = old.scope.clone();
        *state.runtime.lock().unwrap() = Some(synthetic_runtime(&dir.join("a.bin"), old));
        let workflow = serde_json::from_str::<ContextPackage>(include_str!(
            "../fixtures/context-package-v1.json"
        ))
        .unwrap()
        .workflows
        .remove(0);
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, waiting) = tokio::sync::oneshot::channel();
        let saving = async {
            started.send(()).unwrap();
            waiting.await.unwrap();
            let mut slot = state.runtime.lock().unwrap();
            save_workflow_scoped(slot.as_mut().unwrap(), &scope, workflow)
        };
        let switch = async {
            ready.await.unwrap();
            let mut other = Identity::from_config(&AgentConfig::default());
            other.scope = "other-account".into();
            *state.runtime.lock().unwrap() = Some(synthetic_runtime(&dir.join("b.bin"), other));
            release.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(saving, switch);
        assert!(
            result.is_err(),
            "an old draft must not be adopted by the current store"
        );
        assert!(state
            .runtime
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .store
            .workflows()
            .is_empty());
        *state.runtime.lock().unwrap() = None;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn enable_command_cannot_resume_after_lock_during_preparation() {
        let state = ContextState::default();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, waiting) = tokio::sync::oneshot::channel();
        let available = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let command = settings_operation(
            &state,
            async {
                started.send(()).unwrap();
                waiting.await.unwrap();
                Ok(())
            },
            |action| {
                let state = state.clone();
                let available = available.clone();
                async move {
                    // Same final production activation as set_context_settings. No native observation.
                    let mut collector = Collector::default();
                    activate_settings(
                        &state,
                        &mut collector,
                        action,
                        state.gate.generation(),
                        available.load(Ordering::SeqCst),
                    )
                }
            },
        );
        let lock_and_unlock = async {
            ready.await.unwrap();
            available.store(false, Ordering::SeqCst);
            state.pause();
            available.store(true, Ordering::SeqCst);
            release.send(()).unwrap();
        };
        let (result, ()) = tokio::join!(command, lock_and_unlock);
        assert!(
            result.is_err(),
            "the original user command must stay cancelled after unlock"
        );
        assert!(!state.gate.is_active());
        settings_operation(
            &state,
            async {
                state.pause_for_preparation();
                Ok(())
            },
            |action| {
                let state = &state;
                async move {
                    let mut collector = Collector::default();
                    activate_settings(state, &mut collector, action, state.gate.generation(), true)
                }
            },
        )
        .await
        .unwrap();
        assert!(state.gate.is_active());
        state.pause();
    }
    #[tokio::test]
    async fn selection_change_stops_the_next_daily_request_before_offline_revocation() {
        let dir = std::env::temp_dir().join(format!("context-selection-test-{}", Uuid::new_v4()));
        let identity = Identity::from_config(&AgentConfig::default());
        let scope = identity.scope.clone();
        let store = ContextStore::open(&dir.join("synthetic.bin"), [17; 32]).unwrap();
        let runtime = Runtime {
            identity: identity.clone(),
            store,
            settings: ContextSettings::default(),
            collector: Collector::default(),
            authority: Authority::default(),
            preview: None,
            recurring: Some(Recurring {
                selection_generation: 0,
                grant_id: Uuid::new_v4(),
                selection_sha256: "synthetic".into(),
                day: None,
                identity,
            }),
            last_gap: None,
            last_browser: None,
            error: None,
            personal_os_login: true,
            key_fingerprint: hash(&[17; 32]),
            pending_revocation: None,
        };
        let state = ContextState::default();
        *state.runtime.lock().unwrap() = Some(runtime);
        let today = Utc::now().date_naive();
        assert!(next_summary(&state, today).is_some());
        let generation = state.sharing_generation.load(Ordering::SeqCst);
        assert!(
            invalidate_selection_operation(&state, "another-account", |_| async {
                panic!("must not revoke another account")
            })
            .await
            .is_err()
        );
        assert_eq!(state.sharing_generation.load(Ordering::SeqCst), generation);
        assert!(next_summary(&state, today).is_some());
        let result = invalidate_selection_operation(&state, &scope, |_| async {
            assert!(
                next_summary(&state, today).is_none(),
                "local scheduling must stop before any revoke wait"
            );
            Err("synthetic network unavailable".into())
        })
        .await;
        assert!(result.is_err());
        assert!(next_summary(&state, today).is_none());
        let guard = state.runtime.lock().unwrap();
        let r = guard.as_ref().unwrap();
        assert!(r.revocation_pending());
        assert!(r.error.is_some());
        drop(guard);
        *state.runtime.lock().unwrap() = None;
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn deleted_workflow_and_expired_sources_invalidate_a_real_store_preview() {
        let dir = std::env::temp_dir().join(format!("context-sources-test-{}", Uuid::new_v4()));
        let path = dir.join("synthetic.bin");
        let mut store = ContextStore::open(&path, [37; 32]).unwrap();
        let now = Utc::now();
        let mut p = ContextPackage::from_json(
            include_str!("../fixtures/context-package-v1.json"),
            DateTime::parse_from_rfc3339("2026-10-29T02:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        p.captured_from = now;
        p.captured_to = now;
        p.consented_at = now;
        store.save_workflow(p.workflows[0].clone()).unwrap();
        store
            .append(ContextEvent::coverage_gap(
                now - chrono::Duration::days(3),
                Gap::Unsupported,
            ))
            .unwrap();
        let mut r = Runtime {
            identity: Identity::from_config(&AgentConfig::default()),
            store,
            settings: ContextSettings::default(),
            collector: Collector::default(),
            authority: Authority::default(),
            preview: None,
            recurring: None,
            last_gap: None,
            last_browser: None,
            error: None,
            personal_os_login: true,
            key_fingerprint: hash(&[37; 32]),
            pending_revocation: None,
        };
        let preview = r
            .authority
            .freeze(&r.identity.scope, &p.to_json(now).unwrap());
        assert!(sources_unchanged(&r, &preview).is_ok());
        r.prune().unwrap();
        assert!(!r.authority.permits(&preview, &r.identity.scope));
        r.store.delete_workflow(p.workflows[0].id).unwrap();
        assert!(sources_unchanged(&r, &preview).is_err());
        drop(r);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn assessment_links_cannot_escape_the_configured_account_destination() {
        let mut id = Identity::from_config(&AgentConfig::default());
        id.code = Some("12345678".into());
        id.origin = "https://monitor.example".into();
        let mut receipt = ContextReceipt {
            package_id: Uuid::new_v4(),
            sha256: String::new(),
            received_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::days(90),
            assessment_id: Uuid::new_v4(),
            result_url: String::new(),
        };
        receipt.result_url = format!(
            "https://monitor.example/12345678/assessments?assessmentId={}",
            receipt.assessment_id
        );
        assert!(valid_result(&id, &receipt));
        for changed in [
            receipt
                .result_url
                .replace("monitor.example", "other.example"),
            receipt.result_url.replace("12345678", "87654321"),
            format!("{}&redirect=bad", receipt.result_url),
        ] {
            receipt.result_url = changed;
            assert!(!valid_result(&id, &receipt));
        }
    }
    #[test]
    fn erase_needs_no_key_removes_orphans_and_respects_live_store_locks() {
        let dir = std::env::temp_dir().join(format!("context-erasure-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let scope = "a".repeat(64);
        let path = dir.join(format!("{scope}.bin"));
        let mut store = ContextStore::open(&path, [91; 32]).unwrap();
        store
            .append(ContextEvent::coverage_gap(Utc::now(), Gap::Unsupported))
            .unwrap();
        let orphan = dir.join(format!("{scope}.bin.context-tmp.{}.tmp", Uuid::new_v4()));
        std::fs::copy(&path, &orphan).unwrap();
        std::fs::write(dir.join("unrelated.txt"), b"preserve").unwrap();
        assert!(erase_context_files(&dir).is_err());
        assert!(path.exists());
        drop(store);
        erase_context_files(&dir).unwrap();
        assert!(!path.exists());
        assert!(!orphan.exists());
        assert_eq!(
            std::fs::read(dir.join("unrelated.txt")).unwrap(),
            b"preserve"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn python_contract_fixture_and_summary_hash_round_trip() {
        let now = DateTime::parse_from_rfc3339("2026-10-29T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let p = ContextPackage::from_json(include_str!("../fixtures/context-package-v1.json"), now)
            .unwrap();
        let encoded = p.to_json(now).unwrap();
        assert_eq!(ContextPackage::from_json(&encoded, now).unwrap(), p);
        assert_eq!(
            summary_hash(&p).unwrap(),
            include_str!("../fixtures/context-summary-v1.sha256").trim()
        );
    }
    #[test]
    fn account_switch_changes_scope_and_revocation_cancels_native_gate() {
        let mut config = AgentConfig::default();
        let local = Identity::from_config(&config);
        config.account = Some(crate::agent::AccountConfig {
            device_id: Uuid::new_v4().to_string(),
            workspace_id: Some(Uuid::new_v4().to_string()),
            ..Default::default()
        });
        let signed_in = Identity::from_config(&config);
        assert_ne!(local.scope, signed_in.scope);
        config.account.as_mut().unwrap().device_id = Uuid::new_v4().to_string();
        assert_ne!(signed_in.scope, Identity::from_config(&config).scope);
        let state = ContextState::default();
        let mut collector = Collector::default();
        *state.native_gate.lock().unwrap() = Some(collector.session_gate());
        // Explicit synthetic opt-in; this test never calls observe/native capture.
        collector.set_session_active(true);
        let generation = state.gate.generation();
        assert!(state.gate.activate_explicitly(generation));
        let before = state.epoch.load(Ordering::SeqCst);
        state.pause();
        assert!(!state.gate.is_active());
        assert!(!state.gate.activate_explicitly(generation));
        assert!(!collector.session_gate().is_active());
        assert_ne!(before, state.epoch.load(Ordering::SeqCst));
    }
    #[test]
    fn frozen_preview_is_invalid_after_source_or_identity_change() {
        let mut authority = Authority::default();
        let frozen = authority.freeze("identity-a", "{\"approved\":true}");
        assert!(authority.permits(&frozen, "identity-a"));
        assert!(!authority.permits(&frozen, "identity-b"));
        assert_eq!(frozen.sha256, hash(frozen.package_json.as_bytes()));
        authority.invalidate();
        assert!(!authority.permits(&frozen, "identity-a"));
    }
    #[test]
    fn changed_bytes_or_receipt_cannot_reuse_consent() {
        let authority = Authority::default();
        let mut frozen = authority.freeze("identity-a", "{}");
        assert!(frozen.receipt_matches(&frozen.sha256));
        assert!(!frozen.receipt_matches(&hash(b"different")));
        frozen.package_json.push(' ');
        assert!(!authority.permits(&frozen, "identity-a"));
    }
}

fn next_summary(state: &ContextState, day: chrono::NaiveDate) -> Option<Recurring> {
    let generation = state.sharing_generation.load(Ordering::SeqCst);
    state
        .runtime
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .filter(|r| !r.revocation_pending())
                .and_then(|r| r.recurring.clone())
        })
        .filter(|r| r.day != Some(day) && r.selection_generation == generation)
}
async fn invalidate_selection_operation<F, P>(
    state: &ContextState,
    scope: &str,
    revoke: F,
) -> Result<(), String>
where
    F: FnOnce(Identity) -> P,
    P: std::future::Future<Output = Result<(), String>>,
{
    let (identity, generation) = {
        let mut slot = state.runtime.lock().map_err(|_| "Context unavailable")?;
        if slot.as_ref().is_some_and(|r| r.identity.scope != scope) {
            return Err("Account changed".into());
        }
        let generation = state
            .sharing_generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        state.epoch.fetch_add(1, Ordering::SeqCst);
        let identity = if let Some(r) = slot.as_mut() {
            r.invalidate();
            if r.recurring.take().is_some() {
                r.require_revocation(RevocationScope::Summaries);
            }
            if r.revocation_pending() {
                Some(r.identity.clone())
            } else {
                None
            }
        } else {
            None
        };
        (identity, generation)
    };
    if let Some(identity) = identity {
        revoke(identity.clone()).await?;
        let mut slot = state.runtime.lock().map_err(|_| "Context unavailable")?;
        if let Some(r) = slot.as_mut().filter(|r| {
            r.identity == identity && state.sharing_generation.load(Ordering::SeqCst) == generation
        }) {
            r.acknowledge_revocation(RevocationScope::Summaries);
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn invalidate_context_selection(
    app: AppHandle,
    account_scope: String,
) -> Result<(), String> {
    let state = app.state::<ContextState>().inner().clone();
    revoke_selection(&app, &state, &account_scope).await
}
#[tauri::command]
pub async fn cancel_context_preview(app: AppHandle, account_scope: String) {
    let state = app.state::<ContextState>();
    if let Ok(mut slot) = state.runtime.lock() {
        if let Some(r) = slot.as_mut().filter(|r| r.identity.scope == account_scope) {
            state.epoch.fetch_add(1, Ordering::SeqCst);
            r.invalidate();
        }
    };
}

pub async fn sign_out_context<F: std::future::Future>(
    state: &ContextState,
    remote_revoke: F,
) -> F::Output {
    state.prepare_sign_out();
    remote_revoke.await
}
fn erase_local_runtime(
    state: &ContextState,
    dir: &std::path::Path,
    scope: &str,
    operation: u64,
) -> Result<(), String> {
    let mut slot = state.runtime.lock().map_err(|_| "Context unavailable")?;
    if state.action_generation.load(Ordering::SeqCst) != operation
        || slot.as_ref().is_some_and(|r| r.identity.scope != scope)
    {
        return Err("Account changed".into());
    }
    state.retire_runtime(&mut slot)?;
    erase_context_files(dir)
}
fn context_scopes(dir: &std::path::Path) -> Result<Vec<String>, String> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    if std::fs::symlink_metadata(dir)
        .map_err(|_| "Context directory unavailable")?
        .file_type()
        .is_symlink()
    {
        return Err("Refusing linked context directory".into());
    }
    let mut scopes = BTreeSet::new();
    for entry in std::fs::read_dir(dir).map_err(|_| "Context directory unavailable")? {
        let entry = entry.map_err(|_| "Context entry unavailable")?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some((scope, _)) = name.split_once('.') else {
            continue;
        };
        if scope.len() == 64 && scope.bytes().all(|b| b.is_ascii_hexdigit()) {
            scopes.insert(scope.to_owned());
        }
    }
    Ok(scopes.into_iter().collect())
}
/// Erasure needs no decryption key. Lock the store namespace before removing its exact files.
fn erase_context_files(dir: &std::path::Path) -> Result<(), String> {
    for scope in context_scopes(dir)? {
        let lock_path = dir.join(format!("{scope}.context-lock"));
        if std::fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Refusing linked context lock".into());
        }
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|_| "Context lock unavailable")?;
        lock.try_lock()
            .map_err(|_| "Context is open elsewhere; close it before erasing")?;
        for entry in std::fs::read_dir(dir).map_err(|_| "Context directory unavailable")? {
            let entry = entry.map_err(|_| "Context entry unavailable")?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == format!("{scope}.bin")
                || (name.starts_with(&format!("{scope}.bin.context-tmp."))
                    && name.ends_with(".tmp"))
            {
                std::fs::remove_file(entry.path())
                    .map_err(|_| "Local context deletion incomplete")?;
            }
        }
        // Keep the lock inode stable so another process cannot open a replacement lock.
    }
    #[cfg(unix)]
    if dir.exists() {
        std::fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Context deletion could not be synced")?;
    }
    Ok(())
}
fn prune_other_scopes(dir: &std::path::Path, current: Option<&str>) -> Result<(), String> {
    for scope in context_scopes(dir)? {
        if current == Some(scope.as_str()) {
            continue;
        }
        let path = dir.join(format!("{scope}.bin"));
        if !path.exists() {
            continue;
        }
        let secret = key(&scope, false)?;
        let mut store = ContextStore::open(&path, secret).map_err(|e| e.to_string())?;
        store.prune(Utc::now()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
