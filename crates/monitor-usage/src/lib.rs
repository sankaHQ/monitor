//! Foreground-focus tracking.
//!
//! The interval state machine is pure so it can be tested without OS APIs.
//! Platform collectors: the macOS one (NSWorkspace) lives in the agent crate;
//! the Windows one (`GetForegroundWindow` + `GetLastInputInfo`) is [`windows`].

use chrono::{DateTime, Utc};

#[cfg(target_os = "windows")]
pub mod windows;

/// Windows app id: the executable's file name, lowercased (`msedge.exe`).
/// Classification to app slugs happens server-side against this string.
pub fn app_id_from_path(path: &str) -> Option<String> {
    let name = path.rsplit(['\\', '/']).next()?.trim();
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

/// Fallback display name when an executable carries no `FileDescription`
/// version resource: `msedge.exe` → `Msedge`.
pub fn friendly_name_from_app_id(app_id: &str) -> String {
    let stem = app_id.strip_suffix(".exe").unwrap_or(app_id);
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => app_id.to_string(),
    }
}

/// Idle for at least this long closes the current interval (server-tunable).
pub const DEFAULT_IDLE_THRESHOLD_SECS: u64 = 300;
/// Open intervals are checkpointed at most this often so a crash loses ≤1 min.
pub const DEFAULT_HEARTBEAT_SECS: u64 = 60;

/// A foreground-app observation at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusSample {
    pub app_id: String,
    pub app_name: String,
    pub at: DateTime<Utc>,
}

/// A closed focus interval: the app that had focus and when it lost it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedInterval {
    pub sample: FocusSample,
    pub ended_at: DateTime<Utc>,
    pub ended_by_idle: bool,
}

/// Turns focus samples and idle signals into closed intervals.
#[derive(Debug, Default)]
pub struct IntervalBuilder {
    current: Option<FocusSample>,
}

impl IntervalBuilder {
    /// A new app took focus; closes the previous interval, if any.
    pub fn on_focus(&mut self, sample: FocusSample) -> Option<ClosedInterval> {
        let closed = self.current.take().map(|prev| ClosedInterval {
            sample: prev,
            ended_at: sample.at,
            ended_by_idle: false,
        });
        self.current = Some(sample);
        closed
    }

    /// The user went idle; closes the current interval and stops attribution
    /// until the next focus sample.
    pub fn on_idle(&mut self, at: DateTime<Utc>) -> Option<ClosedInterval> {
        self.current.take().map(|prev| ClosedInterval {
            sample: prev,
            ended_at: at,
            ended_by_idle: true,
        })
    }

    /// Closes the current interval at `at` and immediately reopens it for the
    /// same app. Called when an interval grows past the checkpoint window so
    /// a crash loses at most one window and long sessions gain granularity.
    pub fn checkpoint(&mut self, at: DateTime<Utc>) -> Option<ClosedInterval> {
        let previous = self.current.take()?;
        self.current = Some(FocusSample {
            app_id: previous.app_id.clone(),
            app_name: previous.app_name.clone(),
            at,
        });
        Some(ClosedInterval {
            sample: previous,
            ended_at: at,
            ended_by_idle: false,
        })
    }

    pub fn current(&self) -> Option<&FocusSample> {
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 9, min, 0).unwrap()
    }

    fn sample(app: &str, min: u32) -> FocusSample {
        FocusSample {
            app_id: format!("com.example.{app}"),
            app_name: app.to_string(),
            at: at(min),
        }
    }

    #[test]
    fn focus_switch_closes_previous_interval() {
        let mut b = IntervalBuilder::default();
        assert_eq!(b.on_focus(sample("slack", 0)), None);
        let closed = b.on_focus(sample("chrome", 5)).unwrap();
        assert_eq!(closed.sample.app_name, "slack");
        assert_eq!(closed.ended_at, at(5));
        assert!(!closed.ended_by_idle);
        assert_eq!(b.current().unwrap().app_name, "chrome");
    }

    #[test]
    fn checkpoint_splits_long_intervals_without_losing_the_app() {
        let mut b = IntervalBuilder::default();
        b.on_focus(sample("slack", 0));
        let closed = b.checkpoint(at(15)).unwrap();
        assert_eq!(closed.sample.app_name, "slack");
        assert_eq!(closed.ended_at, at(15));
        assert!(!closed.ended_by_idle);
        let current = b.current().unwrap();
        assert_eq!(current.app_name, "slack");
        assert_eq!(current.at, at(15), "reopened at the checkpoint boundary");
        assert_eq!(b.checkpoint(at(16)).unwrap().sample.at, at(15));
    }

    #[test]
    fn windows_app_id_is_the_lowercased_executable_name() {
        assert_eq!(
            app_id_from_path(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
            Some("msedge.exe".into())
        );
        assert_eq!(
            app_id_from_path("C:/Apps/Claude.EXE"),
            Some("claude.exe".into())
        );
        assert_eq!(app_id_from_path(""), None);
        assert_eq!(app_id_from_path(r"C:\dir\"), None);
    }

    #[test]
    fn friendly_name_falls_back_to_the_capitalized_stem() {
        assert_eq!(friendly_name_from_app_id("msedge.exe"), "Msedge");
        assert_eq!(friendly_name_from_app_id("excel"), "Excel");
        assert_eq!(friendly_name_from_app_id(""), "");
    }

    #[test]
    fn idle_closes_current_and_stops_attribution() {
        let mut b = IntervalBuilder::default();
        b.on_focus(sample("slack", 0));
        let closed = b.on_idle(at(10)).unwrap();
        assert!(closed.ended_by_idle);
        assert_eq!(b.current(), None);
        // A second idle signal with nothing open closes nothing.
        assert_eq!(b.on_idle(at(11)), None);
    }
}
