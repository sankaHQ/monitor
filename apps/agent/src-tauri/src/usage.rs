//! Foreground-usage collection and upload.
//!
//! Collection runs while the device is enrolled and the workspace has not
//! paused the agent, and always stays local first (the employee sees their
//! own stats in the Transparency pane). Upload happens only for usage that
//! started after the employee opted into sharing — and the server enforces
//! consent independently, so a buggy client cannot leak.
//! No titles, URLs, documents, or contents: the collector reads only the
//! frontmost application's id (bundle id on macOS, executable name on
//! Windows) + display name, plus system idle time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{Duration as ChronoDuration, Local, Utc};
use monitor_core::UsageEvent;
use monitor_store::SqliteQueue;
use monitor_usage::{ClosedInterval, FocusSample, IntervalBuilder};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

pub const POLL_INTERVAL_SECS: u64 = 5;
pub const CHECKPOINT_SECS: i64 = 900;
pub const UPLOAD_INTERVAL_SECS: u64 = 300;
const MIN_INTERVAL_SECS: i64 = 1;

/// One foreground observation.
#[derive(Debug, Clone)]
pub struct Frontmost {
    pub app_id: String,
    pub app_name: String,
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2_app_kit::NSWorkspace;

    use super::Frontmost;

    /// Frontmost app as (bundle id, localized name). `None` when there is no
    /// frontmost app or it exposes no bundle identifier.
    pub fn frontmost() -> Option<Frontmost> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let bundle_id = app.bundleIdentifier()?.to_string();
        let name = app
            .localizedName()
            .map(|value| value.to_string())
            .unwrap_or_else(|| bundle_id.clone());
        Some(Frontmost {
            app_id: bundle_id,
            app_name: name,
        })
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }

    /// Seconds since the last HID input (any event type). Needs no TCC
    /// permission — it reveals *that* input happened, never what.
    pub fn idle_seconds() -> f64 {
        // 1 = kCGEventSourceStateHIDSystemState, u32::MAX = kCGAnyInputEventType
        unsafe { CGEventSourceSecondsSinceLastEventType(1, u32::MAX) }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::Frontmost;

    /// Foreground window resolved to its executable — see
    /// `monitor_usage::windows`.
    pub fn frontmost() -> Option<Frontmost> {
        let window = monitor_usage::windows::foreground_window()?;
        Some(Frontmost {
            app_id: window.app_id,
            app_name: window.app_name,
        })
    }

    pub fn idle_seconds() -> f64 {
        monitor_usage::windows::idle_seconds()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::Frontmost;

    pub fn frontmost() -> Option<Frontmost> {
        None
    }

    pub fn idle_seconds() -> f64 {
        0.0
    }
}

#[derive(Clone)]
pub struct UsageRuntime {
    pub queue: SqliteQueue,
    pub collecting: Arc<AtomicBool>,
    pub idle_threshold_secs: Arc<AtomicU64>,
    /// The open focus interval. The collector holds this lock for a whole
    /// poll, so a wipe can never race the write of an interval it discards.
    builder: Arc<Mutex<IntervalBuilder>>,
}

impl UsageRuntime {
    pub fn new(queue: SqliteQueue, collecting: bool) -> Self {
        Self {
            queue,
            collecting: Arc::new(AtomicBool::new(collecting)),
            idle_threshold_secs: Arc::new(AtomicU64::new(300)),
            builder: Arc::new(Mutex::new(IntervalBuilder::default())),
        }
    }

    pub fn set_collecting(&self, enabled: bool) {
        self.collecting.store(enabled, Ordering::Relaxed);
    }

    pub fn set_idle_threshold(&self, seconds: u64) {
        self.idle_threshold_secs
            .store(seconds.clamp(60, 3600), Ordering::Relaxed);
    }

    /// Stops collection, drops the open interval unrecorded and deletes the
    /// whole local history: the device is leaving its workspace.
    pub fn discard_and_wipe(&self) -> Result<usize, String> {
        let mut builder = self.builder.lock().unwrap_or_else(PoisonError::into_inner);
        self.set_collecting(false);
        *builder = IntervalBuilder::default();
        self.queue
            .wipe()
            .map_err(|err| format!("could not delete the local usage history: {err}"))
    }
}

pub fn queue_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("app data dir unavailable: {err}"))?;
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir.join("usage.db"))
}

/// Local per-app stats since local midnight — the employee's own view.
pub fn today_stats(queue: &SqliteQueue, limit: usize) -> Vec<monitor_store::LocalAppStat> {
    let midnight = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(Local).single())
        .map(|local| local.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - ChronoDuration::hours(24));
    queue.app_stats_since(midnight, limit).unwrap_or_default()
}

fn persist(queue: &SqliteQueue, closed: ClosedInterval) {
    let duration = closed.ended_at - closed.sample.at;
    if duration < ChronoDuration::seconds(MIN_INTERVAL_SECS) {
        return;
    }
    let event = UsageEvent {
        event_id: Uuid::new_v4(),
        app_id: closed.sample.app_id,
        app_name: closed.sample.app_name,
        started_at: closed.sample.at,
        ended_at: closed.ended_at,
        ended_by_idle: closed.ended_by_idle,
    };
    let _ = queue.enqueue(&event);
}

/// Dedicated OS thread: polls the frontmost app + idle state and turns them
/// into closed intervals. Poll cost is a few microseconds every 5 seconds.
pub fn spawn_collector(runtime: UsageRuntime) {
    std::thread::Builder::new()
        .name("usage-collector".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
            let mut builder = runtime
                .builder
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !runtime.collecting.load(Ordering::Relaxed) {
                if let Some(closed) = builder.on_idle(Utc::now()) {
                    persist(&runtime.queue, closed);
                }
                continue;
            }
            let now = Utc::now();
            let idle_secs = platform::idle_seconds();
            let threshold = runtime.idle_threshold_secs.load(Ordering::Relaxed).max(60);
            if idle_secs >= threshold as f64 {
                // Attribute the interval only up to the last input.
                let last_input = now - ChronoDuration::seconds(idle_secs as i64);
                if let Some(closed) = builder.on_idle(last_input) {
                    persist(&runtime.queue, closed);
                }
                continue;
            }
            let Some(front) = platform::frontmost() else {
                continue;
            };
            let changed = builder
                .current()
                .map(|current| current.app_id != front.app_id)
                .unwrap_or(true);
            if changed {
                let sample = FocusSample {
                    app_id: front.app_id,
                    app_name: front.app_name,
                    at: now,
                };
                if let Some(closed) = builder.on_focus(sample) {
                    persist(&runtime.queue, closed);
                }
            } else if let Some(current) = builder.current() {
                if now - current.at >= ChronoDuration::seconds(CHECKPOINT_SECS) {
                    if let Some(closed) = builder.checkpoint(now) {
                        persist(&runtime.queue, closed);
                    }
                }
            }
        })
        .expect("spawn usage collector thread");
}
