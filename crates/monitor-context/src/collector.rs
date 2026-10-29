//! Native extraction has no persistence capability; policy is checked again after the bounded read.
use crate::*;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc,
    },
    time::{Duration as StdDuration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Button,
    MenuItem,
    Toggle,
    StaticText,
    Editable,
    Password,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrowserContext {
    NotBrowser,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observation {
    pub app_id: String,
    pub app_name: String,
    pub role: Role,
    pub action: Action,
    pub browser: BrowserContext,
    pub permission: bool,
    pub unlocked: bool,
}
enum NativeKind {
    TextEdit,
    ClassicNotepad,
}
pub(crate) struct NativeProfile(NativeKind);
/// A user allowlist is necessary but cannot establish that a surface is native.
pub(crate) fn native_profile(
    settings: &ContextSettings,
    app_id: &str,
    location: &str,
    system_directory: Option<&str>,
) -> std::result::Result<NativeProfile, Gap> {
    if !settings.allows_app(app_id) || sensitive_app(app_id) {
        return Err(Gap::Excluded);
    }
    if browser_app(app_id) {
        return Err(Gap::PrivateOrUnknownBrowser);
    }
    // Verified system TextEdit metadata; Windows location comes from GetSystemDirectoryW.
    if system_directory.is_none()
        && app_id == "com.apple.TextEdit"
        && location == "/System/Applications/TextEdit.app"
    {
        return Ok(NativeProfile(NativeKind::TextEdit));
    }
    if app_id == "notepad.exe"
        && system_directory.is_some_and(|directory| {
            !directory.is_empty()
                && location.eq_ignore_ascii_case(&format!(
                    "{}\\notepad.exe",
                    directory.trim_end_matches('\\')
                ))
        })
    {
        return Ok(NativeProfile(NativeKind::ClassicNotepad));
    }
    Err(Gap::Unsupported)
}
impl NativeProfile {
    pub(crate) fn read_action(
        &self,
        role: Role,
        ancestors: &[&str],
        read: impl FnOnce() -> std::result::Result<String, Gap>,
    ) -> std::result::Result<Action, Gap> {
        if !matches!(role, Role::Button | Role::MenuItem) {
            return Ok(Action::Unknown);
        }
        let supported = !ancestors.is_empty()
            && ancestors.len() <= 8
            && match (&self.0, role) {
                (NativeKind::TextEdit, Role::Button) => {
                    ancestors.last() == Some(&"AXWindow")
                        && ancestors.contains(&"AXToolbar")
                        && ancestors[..ancestors.len() - 1]
                            .iter()
                            .all(|r| matches!(*r, "AXGroup" | "AXToolbar"))
                }
                (NativeKind::TextEdit, Role::MenuItem) => {
                    ancestors.last() == Some(&"AXApplication")
                        && ancestors.contains(&"AXMenuBar")
                        && ancestors[..ancestors.len() - 1]
                            .iter()
                            .all(|r| matches!(*r, "AXMenu" | "AXMenuBarItem" | "AXMenuBar"))
                }
                (NativeKind::ClassicNotepad, _) => {
                    ancestors.last() == Some(&"Win32Window")
                        && ancestors[..ancestors.len() - 1].iter().all(|r| {
                            matches!(
                                *r,
                                "Win32Menu" | "Win32MenuBar" | "Win32Toolbar" | "Win32Pane"
                            )
                        })
                }
                _ => false,
            };
        if !supported {
            return Err(Gap::Unsupported);
        }
        Ok(normalize_action(role, &read()?))
    }
}
// Exact noneditable labels only; this is a focus interpretation, never proof of execution.
pub(crate) fn normalize_action(role: Role, label: &str) -> Action {
    if !matches!(role, Role::Button | Role::MenuItem) || label.len() > 127 {
        return Action::Unknown;
    }
    match label {
        "Open" | "Open..." | "Open…" => Action::Navigate,
        "Read" => Action::Read,
        "Search" => Action::Search,
        "Create" | "New" => Action::Create,
        "Edit" => Action::Edit,
        "Export" | "Export..." | "Export…" => Action::Export,
        "Import" | "Import..." | "Import…" => Action::Import,
        "Approve" => Action::Approve,
        "Send" => Action::Send,
        _ => Action::Unknown,
    }
}
fn filter_observation(
    settings: &ContextSettings,
    observation: &Observation,
) -> std::result::Result<(), Gap> {
    if !settings.allows_app(&observation.app_id) || sensitive_app(&observation.app_id) {
        return Err(Gap::Excluded);
    }
    if !observation.permission {
        return Err(Gap::PermissionDenied);
    }
    if !observation.unlocked {
        return Err(Gap::Locked);
    }
    if observation.browser == BrowserContext::Unknown || browser_app(&observation.app_id) {
        return Err(Gap::PrivateOrUnknownBrowser);
    }
    match observation.role {
        Role::Editable | Role::Password => Err(Gap::Excluded),
        Role::Unknown => Err(Gap::Unsupported),
        _ => Ok(()),
    }
}
pub(crate) fn browser_app(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    [
        "safari",
        "chrome",
        "chromium",
        "firefox",
        "edge",
        "brave",
        "opera",
        "vivaldi",
        "arc",
        "zen-browser",
        "waterfox",
        "librewolf",
    ]
    .iter()
    .any(|name| id.contains(name))
}
pub(crate) fn sensitive_app(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    [
        "1password",
        "bitwarden",
        "keepass",
        "lastpass",
        "dashlane",
        "keychain",
        "credential",
        "loginwindow",
        "authenticator",
        "mstsc",
        "remote",
        "screensharing",
        "teamviewer",
        "anydesk",
        "citrix",
        "vmware",
        "parallels",
    ]
    .iter()
    .any(|name| id.contains(name))
}

/// A lock/session callback can pause capture without waiting for the collector mutex.
/// Only the collector's explicit enable/resume entry point can activate a new session.
#[derive(Clone, Default)]
pub struct SessionGate(Arc<AtomicU64>);
impl SessionGate {
    pub fn pause(&self) {
        self.set_active(false);
    }
    pub fn is_active(&self) -> bool {
        self.generation() & 1 == 1
    }
    fn generation(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn accepts(&self, generation: u64) -> bool {
        generation & 1 == 1 && self.generation() == generation
    }
    fn set_active(&self, active: bool) {
        let mut previous = self.generation();
        loop {
            let next = (previous.wrapping_add(2) & !1) | u64::from(active);
            match self
                .0
                .compare_exchange(previous, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return,
                Err(current) => previous = current,
            }
        }
    }
    fn resume(&self) {
        self.set_active(true);
    }
}

/// Default is inert. The UI must call `set_session_active(true)` only on a deliberate
/// Enable/Resume gesture, and call false on lock, sleep, session change or unknown state.
#[derive(Default)]
pub struct Collector {
    active: SessionGate,
    pending: Option<Receiver<std::result::Result<Observation, Gap>>>,
    last_accepted: Option<Instant>,
    previous: Option<Observation>,
    gap: Option<Gap>,
}
impl Collector {
    pub fn set_session_active(&mut self, active: bool) {
        if active {
            self.active.resume();
        } else {
            self.active.pause();
        }
        self.previous = None;
        // An old read can never be attributed to a resumed session.
        if !active {
            self.gap = Some(Gap::Locked);
        }
    }
    pub fn session_gate(&self) -> SessionGate {
        self.active.clone()
    }
    pub fn last_gap(&self) -> Option<Gap> {
        self.gap
    }
    pub fn observe(&mut self, settings: &ContextSettings) -> Result<Option<ContextEvent>> {
        if !settings.enabled || settings.paused || settings.allowed_app_ids.is_empty() {
            self.gap = Some(Gap::Excluded);
            return Ok(None);
        }
        if !self.active.is_active() {
            self.gap = Some(Gap::Locked);
            return Ok(None);
        }
        if self
            .last_accepted
            .is_some_and(|at| at.elapsed() < StdDuration::from_secs(1))
        {
            return Ok(None);
        }
        if self.provider_busy() {
            self.gap = Some(Gap::Unsupported);
            return Ok(None);
        }
        let (send, receive) = mpsc::sync_channel(1);
        let deadline = Instant::now() + StdDuration::from_millis(CAPTURE_DEADLINE_MS);
        let generation = self.active.generation();
        let active = self.active.clone();
        let settings_snapshot = settings.clone();
        std::thread::Builder::new()
            .name("context-focus".into())
            .spawn(move || {
                let result = native_observe(&settings_snapshot, deadline, &active);
                let _ = send.send(result);
            })?;
        let Some(observation) = self.finish_read(receive, generation, deadline) else {
            return Ok(None);
        };
        self.accept_observation(settings, observation)
    }
    fn accept_observation(
        &mut self,
        settings: &ContextSettings,
        observation: Observation,
    ) -> Result<Option<ContextEvent>> {
        if let Err(gap) = filter_observation(settings, &observation) {
            self.previous = None;
            self.gap = Some(gap);
            return Ok(None);
        }
        self.gap = None;
        if self.previous.as_ref() == Some(&observation) {
            return Ok(None);
        }
        let event = ContextEvent {
            id: Uuid::new_v4(),
            at: Utc::now(),
            app_id: observation.app_id.clone(),
            app_name: observation.app_name.clone(),
            action: observation.action,
            object_category: ObjectCategory::Unknown,
            provenance: if observation.action == Action::Unknown {
                Provenance::Observed
            } else {
                Provenance::Inferred
            },
            gap: None,
        };
        event.validate()?;
        self.previous = Some(observation);
        self.last_accepted = Some(Instant::now());
        Ok(Some(event))
    }
    // A timed-out provider retains one worker/result; late observations are discarded.
    fn provider_busy(&mut self) -> bool {
        let Some(pending) = &self.pending else {
            return false;
        };
        if matches!(pending.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            return true;
        }
        self.pending = None;
        false
    }
    fn finish_read(
        &mut self,
        receive: Receiver<std::result::Result<Observation, Gap>>,
        generation: u64,
        deadline: Instant,
    ) -> Option<Observation> {
        let result = receive.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        if matches!(result, Err(RecvTimeoutError::Timeout)) {
            self.pending = Some(receive);
        }
        if !self.active.accepts(generation) {
            self.previous = None;
            self.gap = Some(Gap::Locked);
            return None;
        }
        let observation = match result {
            Ok(Ok(observation)) => observation,
            Ok(Err(gap)) => {
                self.previous = None;
                self.gap = Some(gap);
                return None;
            }
            Err(RecvTimeoutError::Timeout) => {
                self.gap = Some(Gap::Unsupported);
                return None;
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.gap = Some(Gap::Unsupported);
                return None;
            }
        };
        Some(observation)
    }
}
fn native_observe(
    settings: &ContextSettings,
    deadline: Instant,
    active: &SessionGate,
) -> std::result::Result<Observation, Gap> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::observe(settings, deadline, active)
    }
    #[cfg(target_os = "windows")]
    {
        crate::windows::observe(settings, deadline, active)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (settings, deadline, active);
        Err(Gap::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_allowlisting_cannot_authorize_unknown_browser_labels() {
        let settings = ContextSettings {
            enabled: true,
            allowed_app_ids: vec![
                "work-browser.exe".into(),
                "org.example.CustomWeb".into(),
                "com.apple.TextEdit".into(),
                "notepad.exe".into(),
            ],
            allowed_domains: vec!["example.com".into()],
            ..Default::default()
        };
        for (app, location) in [
            ("work-browser.exe", "C:\\Tools\\work-browser.exe"),
            ("org.example.CustomWeb", "/Applications/CustomWeb.app"),
            ("com.apple.TextEdit", "/Applications/RenamedBrowser.app"),
        ] {
            let result = native_profile(&settings, app, location, None).and_then(|profile| {
                profile.read_action(Role::Button, &["AXToolbar", "AXWindow"], || {
                    panic!("unknown app must not read its label")
                })
            });
            assert_eq!(result, Err(Gap::Unsupported));
        }
        let profile = native_profile(
            &settings,
            "com.apple.TextEdit",
            "/System/Applications/TextEdit.app",
            None,
        )
        .unwrap();
        assert_eq!(
            profile
                .read_action(Role::Button, &["AXToolbar", "AXWindow"], || Ok(
                    "Open".into()
                ))
                .unwrap(),
            Action::Navigate
        );
        assert_eq!(
            profile.read_action(
                Role::Button,
                &["AXGroup", "AXWebArea", "AXWindow"],
                || panic!("embedded browser must not read its label")
            ),
            Err(Gap::Unsupported)
        );
        for (app, location) in [
            (
                "work-browser.exe",
                "C:\\Windows\\System32\\work-browser.exe",
            ),
            ("notepad.exe", "C:\\Tools\\notepad.exe"),
        ] {
            assert!(
                native_profile(&settings, app, location, Some("C:\\Windows\\System32")).is_err()
            );
        }
        let profile = native_profile(
            &settings,
            "notepad.exe",
            "C:\\WINDOWS\\system32\\notepad.exe",
            Some("C:\\Windows\\System32"),
        )
        .unwrap();
        assert_eq!(
            profile
                .read_action(
                    Role::MenuItem,
                    &["Win32Menu", "Win32MenuBar", "Win32Window"],
                    || Ok("Open...".into())
                )
                .unwrap(),
            Action::Navigate
        );
        assert_eq!(
            profile.read_action(Role::Button, &["ChromePane", "Win32Window"], || panic!(
                "web control must not read its label"
            )),
            Err(Gap::Unsupported)
        );
    }
    #[test]
    fn supported_button_labels_produce_distinct_inferred_same_app_steps_without_text() {
        let settings = ContextSettings {
            enabled: true,
            allowed_app_ids: vec!["safe".into()],
            ..Default::default()
        };
        let mut collector = Collector::default();
        let path =
            std::env::temp_dir().join(format!("context-normalization-{}.enc", Uuid::new_v4()));
        let mut store = ContextStore::open(&path, [7; 32]).unwrap();
        let start: DateTime<Utc> = "2026-10-28T10:00:00Z".parse().unwrap();
        for minutes in [0, 10, 1440] {
            for (i, label) in ["Read", "Export", "Import"].iter().enumerate() {
                let observation = Observation {
                    app_id: "safe".into(),
                    app_name: "Synthetic app".into(),
                    role: Role::Button,
                    action: normalize_action(Role::Button, label),
                    browser: BrowserContext::NotBrowser,
                    permission: true,
                    unlocked: true,
                };
                let mut event = collector
                    .accept_observation(&settings, observation.clone())
                    .unwrap()
                    .expect("action changes must survive same-app deduplication");
                assert!(collector
                    .accept_observation(&settings, observation)
                    .unwrap()
                    .is_none());
                event.at = start + Duration::minutes(minutes) + Duration::seconds(i as i64);
                store.append(event).unwrap();
            }
        }
        let candidates = store.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0]
                .steps
                .iter()
                .map(|s| s.action)
                .collect::<Vec<_>>(),
            [Action::Read, Action::Export, Action::Import]
        );
        assert!(candidates[0]
            .steps
            .iter()
            .all(|s| s.provenance == Provenance::Inferred));
        let secret = "Export private-synthetic-name";
        for role in [
            Role::Editable,
            Role::Password,
            Role::StaticText,
            Role::Unknown,
        ] {
            assert_eq!(normalize_action(role, "Export"), Action::Unknown);
        }
        assert_eq!(normalize_action(Role::Button, secret), Action::Unknown);
        assert_eq!(
            normalize_action(Role::Button, &"Export".repeat(30)),
            Action::Unknown
        );
        let unknown = Observation {
            app_id: "safe".into(),
            app_name: "Synthetic app".into(),
            role: Role::Button,
            action: normalize_action(Role::Button, secret),
            browser: BrowserContext::NotBrowser,
            permission: true,
            unlocked: true,
        };
        let unknown = collector
            .accept_observation(&settings, unknown)
            .unwrap()
            .unwrap();
        assert_eq!(unknown.action, Action::Unknown);
        store.append(unknown).unwrap();
        assert!(!serde_json::to_string(&store.events())
            .unwrap()
            .contains("private-synthetic-name"));
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn paused_timeout_keeps_the_stalled_worker_until_its_result_is_discarded() {
        let mut collector = Collector::default();
        collector.set_session_active(true);
        let generation = collector.active.generation();
        let (send, receive) = mpsc::sync_channel(1);
        let (release, stalled) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            stalled.recv().unwrap();
            let _ = send.send(Ok(Observation {
                app_id: "safe".into(),
                app_name: "Synthetic late app".into(),
                role: Role::Button,
                action: Action::Export,
                browser: BrowserContext::NotBrowser,
                permission: true,
                unlocked: true,
            }));
        });
        collector.session_gate().pause();
        assert!(collector
            .finish_read(receive, generation, Instant::now())
            .is_none());
        collector.set_session_active(true);
        let busy = collector.provider_busy();
        // Release/join even on failure; the regression must not leak its synthetic worker.
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(
            busy,
            "resume must not admit a second provider while the first is stalled"
        );
        assert!(!collector.provider_busy());
        assert!(collector.previous.is_none());
    }
    #[test]
    fn pause_handle_invalidates_reads_even_after_explicit_resume() {
        let mut collector = Collector::default();
        let gate = collector.session_gate();
        assert!(!gate.is_active());
        collector.set_session_active(true);
        let generation = gate.generation();
        assert!(gate.accepts(generation));
        gate.clone().pause();
        assert!(!gate.is_active());
        assert!(!gate.accepts(generation));
        collector.set_session_active(true);
        assert!(gate.is_active());
        assert!(!gate.accepts(generation));
    }
    #[test]
    fn unsafe_or_disallowed_focus_never_becomes_a_content_event() {
        let settings = ContextSettings {
            enabled: true,
            allowed_app_ids: vec!["safe".into()],
            ..Default::default()
        };
        let safe = Observation {
            app_id: "safe".into(),
            app_name: "Synthetic app".into(),
            role: Role::Button,
            action: Action::Unknown,
            browser: BrowserContext::NotBrowser,
            permission: true,
            unlocked: true,
        };
        assert!(filter_observation(&settings, &safe).is_ok());
        for sample in [
            Observation {
                app_id: "excluded".into(),
                ..safe.clone()
            },
            Observation {
                role: Role::Editable,
                ..safe.clone()
            },
            Observation {
                role: Role::Password,
                ..safe.clone()
            },
            Observation {
                browser: BrowserContext::Unknown,
                ..safe.clone()
            },
            Observation {
                unlocked: false,
                ..safe.clone()
            },
            Observation {
                permission: false,
                ..safe.clone()
            },
        ] {
            assert!(filter_observation(&settings, &sample).is_err());
        }
        assert!(filter_observation(&ContextSettings::default(), &safe).is_err());
    }
}
