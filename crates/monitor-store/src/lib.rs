//! Local event buffering: the durable SQLite (WAL) queue used by the agent
//! ([`sqlite::SqliteQueue`]), plus the in-memory implementation used in tests.

pub mod sqlite;

pub use sqlite::{LocalAppStat, PolicyOutcome, SqliteQueue};

use monitor_core::UsageEvent;
use uuid::Uuid;

/// Local records older than this are dropped regardless of sharing state.
pub const MAX_QUEUE_AGE_DAYS: i64 = 14;
/// Upload batch ceiling; uploads also trigger on a 5-minute jittered timer.
pub const DEFAULT_BATCH_SIZE: usize = 500;

/// At-least-once queue: events stay queued until explicitly acknowledged
/// after a successful upload.
pub trait EventQueue {
    fn enqueue(&mut self, event: UsageEvent);
    /// Up to `limit` oldest events, left in place.
    fn peek_batch(&self, limit: usize) -> Vec<UsageEvent>;
    /// Remove events acknowledged by the server.
    fn ack(&mut self, event_ids: &[Uuid]);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Default)]
pub struct InMemoryQueue {
    events: Vec<UsageEvent>,
}

impl EventQueue for InMemoryQueue {
    fn enqueue(&mut self, event: UsageEvent) {
        self.events.push(event);
    }

    fn peek_batch(&self, limit: usize) -> Vec<UsageEvent> {
        self.events.iter().take(limit).cloned().collect()
    }

    fn ack(&mut self, event_ids: &[Uuid]) {
        self.events.retain(|e| !event_ids.contains(&e.event_id));
    }

    fn len(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn event(n: u128) -> UsageEvent {
        let at = chrono::Utc.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap();
        UsageEvent {
            event_id: Uuid::from_u128(n),
            app_id: "com.example.app".into(),
            app_name: "Example".into(),
            started_at: at,
            ended_at: at,
            ended_by_idle: false,
        }
    }

    #[test]
    fn enqueue_peek_ack_roundtrip() {
        let mut q = InMemoryQueue::default();
        for n in 1..=3 {
            q.enqueue(event(n));
        }
        assert_eq!(q.len(), 3);

        let batch = q.peek_batch(2);
        assert_eq!(batch.len(), 2);
        assert_eq!(q.len(), 3, "peek must not remove");

        let ids: Vec<Uuid> = batch.iter().map(|e| e.event_id).collect();
        q.ack(&ids);
        assert_eq!(q.len(), 1);
        assert_eq!(q.peek_batch(10)[0].event_id, Uuid::from_u128(3));
    }
}
