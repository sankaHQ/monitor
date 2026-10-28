//! Durable local usage-event store (SQLite, WAL).
//!
//! Design: events are recorded locally regardless of the sharing consent —
//! the employee always sees their own stats. Every row is in one of three
//! upload states: pending (it may be uploaded while it lies inside the
//! consent window), uploaded (the server acknowledged it; uploads stay
//! at-least-once and the server dedupes on `event_id`), or withheld
//! (recorded outside the consent window, so it never leaves the device).
//! Everything older than the retention window is deleted whatever its state,
//! and deleted rows are overwritten on disk.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use monitor_core::UsageEvent;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::{DEFAULT_BATCH_SIZE, MAX_QUEUE_AGE_DAYS};

/// Values of the `uploaded` column.
const PENDING: i64 = 0;
const UPLOADED: i64 = 1;
const WITHHELD: i64 = 2;

#[derive(Debug, Clone)]
pub struct SqliteQueue {
    path: PathBuf,
}

pub type StoreResult<T> = Result<T, rusqlite::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAppStat {
    pub app_id: String,
    pub app_name: String,
    pub total_seconds: i64,
}

/// What one pass of the local data policy changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PolicyOutcome {
    /// Rows deleted because they passed the retention window.
    pub pruned: usize,
    /// Pending rows outside the consent window, now never uploaded.
    pub withheld: usize,
}

impl SqliteQueue {
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let queue = Self {
            path: path.as_ref().to_path_buf(),
        };
        let conn = queue.connect()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS uploaded_days (
              app_id TEXT NOT NULL,
              day TEXT NOT NULL,
              PRIMARY KEY (app_id, day)
            );
            CREATE TABLE IF NOT EXISTS usage_events (
              event_id TEXT PRIMARY KEY,
              app_id TEXT NOT NULL,
              app_name TEXT NOT NULL,
              started_at TEXT NOT NULL,
              ended_at TEXT NOT NULL,
              ended_by_idle INTEGER NOT NULL,
              uploaded INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS usage_events_uploaded_idx
              ON usage_events (uploaded, started_at);
            CREATE INDEX IF NOT EXISTS usage_events_started_idx
              ON usage_events (started_at);
            ",
        )?;
        Ok(queue)
    }

    fn connect(&self) -> StoreResult<Connection> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Deleted rows are zeroed on disk instead of lingering in free pages.
        conn.pragma_update(None, "secure_delete", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }

    pub fn enqueue(&self, event: &UsageEvent) -> StoreResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR IGNORE INTO usage_events
               (event_id, app_id, app_name, started_at, ended_at, ended_by_idle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.event_id.to_string(),
                event.app_id,
                event.app_name,
                event.started_at.to_rfc3339(),
                event.ended_at.to_rfc3339(),
                event.ended_by_idle as i64,
            ],
        )?;
        Ok(())
    }

    /// Oldest pending events that started at or after `since`, the start of
    /// the consent window, left in place until acknowledged. Nothing recorded
    /// before the employee opted in is ever returned.
    pub fn pending(&self, limit: usize, since: DateTime<Utc>) -> StoreResult<Vec<UsageEvent>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT event_id, app_id, app_name, started_at, ended_at, ended_by_idle
             FROM usage_events
             WHERE uploaded = ?1 AND started_at >= ?2
             ORDER BY started_at ASC
             LIMIT ?3",
        )?;
        let limit = limit.min(DEFAULT_BATCH_SIZE) as i64;
        let rows = statement.query_map(params![PENDING, since.to_rfc3339(), limit], |row| {
            let event_id: String = row.get(0)?;
            let started_at: String = row.get(3)?;
            let ended_at: String = row.get(4)?;
            Ok(UsageEvent {
                event_id: Uuid::parse_str(&event_id).unwrap_or_default(),
                app_id: row.get(1)?,
                app_name: row.get(2)?,
                started_at: parse_rfc3339(&started_at),
                ended_at: parse_rfc3339(&ended_at),
                ended_by_idle: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect()
    }

    /// Acknowledged daily facts survive restarts, so later focus intervals do not
    /// cause another upload of the same app/day.
    pub fn uploaded_days(&self) -> StoreResult<BTreeSet<(String, NaiveDate)>> {
        let conn = self.connect()?;
        let mut query = conn.prepare("SELECT app_id, day FROM uploaded_days")?;
        let rows = query.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }

    pub fn remember_uploaded_days(&self, days: &[(String, NaiveDate)]) -> StoreResult<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        for (app, day) in days {
            tx.execute(
                "INSERT OR IGNORE INTO uploaded_days(app_id, day) VALUES (?1, ?2)",
                params![app, day.to_string()],
            )?;
        }
        tx.commit()
    }

    pub fn mark_uploaded(&self, event_ids: &[Uuid]) -> StoreResult<usize> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        let conn = self.connect()?;
        let placeholders = vec!["?"; event_ids.len()].join(", ");
        let ids: Vec<String> = event_ids.iter().map(Uuid::to_string).collect();
        let updated = conn.execute(
            &format!(
                "UPDATE usage_events SET uploaded = {UPLOADED} WHERE event_id IN ({placeholders})"
            ),
            rusqlite::params_from_iter(ids.iter()),
        )?;
        Ok(updated)
    }

    /// How many events `pending(_, since)` would still hand to the uploader.
    pub fn pending_count(&self, since: DateTime<Utc>) -> StoreResult<u64> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM usage_events WHERE uploaded = ?1 AND started_at >= ?2",
            params![PENDING, since.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn is_empty(&self) -> StoreResult<bool> {
        let conn = self.connect()?;
        let any: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM usage_events) OR EXISTS(SELECT 1 FROM uploaded_days)",
            [],
            |row| row.get(0),
        )?;
        Ok(!any)
    }

    /// Local per-app totals since `since`, in every upload state — the
    /// employee's own view, independent of sharing consent.
    pub fn app_stats_since(
        &self,
        since: DateTime<Utc>,
        limit: usize,
    ) -> StoreResult<Vec<LocalAppStat>> {
        let conn = self.connect()?;
        let mut statement = conn.prepare(
            "SELECT app_id, max(app_name),
                    CAST(round(sum(julianday(ended_at) - julianday(started_at)) * 86400) AS INTEGER)
             FROM usage_events
             WHERE started_at >= ?1
             GROUP BY app_id
             ORDER BY 3 DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since.to_rfc3339(), limit as i64], |row| {
            Ok(LocalAppStat {
                app_id: row.get(0)?,
                app_name: row.get(1)?,
                total_seconds: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Deletes every stored event and compacts the file so none of the history
    /// stays on disk — used when the device unenrolls.
    pub fn wipe(&self) -> StoreResult<usize> {
        let conn = self.connect()?;
        let deleted = conn.execute("DELETE FROM usage_events", [])?;
        conn.execute("DELETE FROM uploaded_days", [])?;
        conn.execute_batch("VACUUM")?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(deleted)
    }

    /// The local data policy, applied on its own timer whatever the consent
    /// state: rows older than the retention window are deleted (uploaded or
    /// not), and pending rows outside the consent window are withheld for
    /// good. `upload_since == None` (sharing off) withholds every pending row.
    /// Withheld rows stay in the employee's own view until they expire.
    pub fn apply_policy(
        &self,
        now: DateTime<Utc>,
        upload_since: Option<DateTime<Utc>>,
    ) -> StoreResult<PolicyOutcome> {
        let cutoff = now - chrono::Duration::days(MAX_QUEUE_AGE_DAYS);
        let conn = self.connect()?;
        let pruned = conn.execute(
            "DELETE FROM usage_events WHERE started_at < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        conn.execute(
            "DELETE FROM uploaded_days WHERE day < ?1",
            params![cutoff.date_naive().to_string()],
        )?;
        let withheld = match upload_since {
            Some(since) => conn.execute(
                "UPDATE usage_events SET uploaded = ?1 WHERE uploaded = ?2 AND started_at < ?3",
                params![WITHHELD, PENDING, since.to_rfc3339()],
            )?,
            None => conn.execute(
                "UPDATE usage_events SET uploaded = ?1 WHERE uploaded = ?2",
                params![WITHHELD, PENDING],
            )?,
        };
        Ok(PolicyOutcome { pruned, withheld })
    }
}

fn parse_rfc3339(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 9, minute, 0).unwrap()
    }

    fn event(n: u128, app: &str, minute: u32) -> UsageEvent {
        UsageEvent {
            event_id: Uuid::from_u128(n),
            app_id: format!("com.example.{app}"),
            app_name: app.to_string(),
            started_at: at(minute),
            ended_at: at(minute) + Duration::minutes(5),
            ended_by_idle: false,
        }
    }

    fn names(events: &[UsageEvent]) -> Vec<&str> {
        events.iter().map(|event| event.app_name.as_str()).collect()
    }

    #[test]
    fn acknowledged_days_survive_restart_and_leave_with_the_local_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daily.db");
        let day = at(0).date_naive();
        let queue = SqliteQueue::open(&path).unwrap();
        let fact = ("test.app".to_string(), day);
        queue
            .remember_uploaded_days(&[fact.clone(), fact.clone()])
            .unwrap();
        let reopened = SqliteQueue::open(&path).unwrap();
        assert_eq!(reopened.uploaded_days().unwrap(), BTreeSet::from([fact]));
        assert!(!reopened.is_empty().unwrap());
        reopened
            .apply_policy(at(0) + Duration::days(MAX_QUEUE_AGE_DAYS + 1), None)
            .unwrap();
        assert!(reopened.uploaded_days().unwrap().is_empty());
        reopened
            .remember_uploaded_days(&[("test.app".into(), day)])
            .unwrap();
        reopened.wipe().unwrap();
        assert!(reopened.is_empty().unwrap());
    }

    #[test]
    fn enqueue_pending_upload_roundtrip_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        {
            let queue = SqliteQueue::open(&path).unwrap();
            queue.enqueue(&event(1, "slack", 0)).unwrap();
            queue.enqueue(&event(2, "chrome", 10)).unwrap();
            // Duplicate enqueue is ignored (crash-replay safety).
            queue.enqueue(&event(1, "slack", 0)).unwrap();
        }

        let queue = SqliteQueue::open(&path).unwrap();
        assert_eq!(queue.pending_count(at(0)).unwrap(), 2);
        let batch = queue.pending(500, at(0)).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].app_name, "slack", "oldest first");

        queue.mark_uploaded(&[batch[0].event_id]).unwrap();
        assert_eq!(queue.pending_count(at(0)).unwrap(), 1);
        assert_eq!(queue.pending(500, at(0)).unwrap()[0].app_name, "chrome");
    }

    #[test]
    fn local_stats_include_uploaded_and_pending() {
        let dir = tempfile::tempdir().unwrap();
        let queue = SqliteQueue::open(dir.path().join("usage.db")).unwrap();
        queue.enqueue(&event(1, "slack", 0)).unwrap();
        queue.enqueue(&event(2, "slack", 10)).unwrap();
        queue.enqueue(&event(3, "chrome", 20)).unwrap();
        queue.mark_uploaded(&[Uuid::from_u128(1)]).unwrap();

        let since = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        let stats = queue.app_stats_since(since, 5).unwrap();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].app_name, "slack");
        assert_eq!(stats[0].total_seconds, 600);
        assert_eq!(stats[1].total_seconds, 300);
    }

    #[test]
    fn pending_never_returns_usage_from_before_the_consent_window() {
        let dir = tempfile::tempdir().unwrap();
        let queue = SqliteQueue::open(dir.path().join("usage.db")).unwrap();
        queue.enqueue(&event(1, "before", 0)).unwrap();
        queue.enqueue(&event(2, "at-consent", 10)).unwrap();
        queue.enqueue(&event(3, "after", 20)).unwrap();

        assert_eq!(
            names(&queue.pending(500, at(10)).unwrap()),
            ["at-consent", "after"]
        );
    }

    #[test]
    fn opting_in_withholds_the_backlog_but_keeps_it_local() {
        let dir = tempfile::tempdir().unwrap();
        let queue = SqliteQueue::open(dir.path().join("usage.db")).unwrap();
        queue.enqueue(&event(1, "before-1", 0)).unwrap();
        queue.enqueue(&event(2, "before-2", 5)).unwrap();

        // Opting in at :10 withholds the backlog for good: a later, wider
        // window does not bring it back.
        let opted_in = at(10);
        let outcome = queue.apply_policy(opted_in, Some(opted_in)).unwrap();
        assert_eq!(outcome.withheld, 2);
        queue.enqueue(&event(3, "after", 15)).unwrap();
        assert_eq!(names(&queue.pending(500, at(0)).unwrap()), ["after"]);

        // Sharing off again: whatever is still pending is withheld too.
        assert_eq!(queue.apply_policy(at(20), None).unwrap().withheld, 1);
        assert_eq!(queue.pending_count(at(0)).unwrap(), 0);

        // The employee's own view still shows all of it.
        assert_eq!(queue.app_stats_since(at(0), 10).unwrap().len(), 3);
    }

    #[test]
    fn wipe_leaves_no_trace_of_the_history_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        let queue = SqliteQueue::open(&path).unwrap();
        for n in 0..50 {
            queue
                .enqueue(&event(n, "distinctive-app-name", n as u32))
                .unwrap();
        }
        assert_eq!(queue.wipe().unwrap(), 50);

        let mut bytes = std::fs::read(&path).unwrap();
        if let Ok(wal) = std::fs::read(dir.path().join("usage.db-wal")) {
            bytes.extend(wal);
        }
        let needle = b"distinctive-app-name";
        assert!(!bytes.windows(needle.len()).any(|window| window == needle));
    }
}
