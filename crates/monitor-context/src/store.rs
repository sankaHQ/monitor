//! Single-owner encrypted snapshot. The application supplies a per-OS-user keychain key.
use crate::*;
use chrono::{Duration, Local};
use ring::{
    aead, digest,
    rand::{SecureRandom, SystemRandom},
};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const HEADER: &[u8] = b"SANKACTX1";
const ENVELOPE_BYTES: usize = 9 + 12 + 16;
const CAPACITY_RESERVE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Accepted,
    Capacity,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextCandidate {
    pub id: Uuid,
    pub steps: Vec<ContextStep>,
    pub event_ids: Vec<Uuid>,
    pub episodes: Vec<ContextEpisode>,
    pub observed_days: u8,
    pub repeat_count: u16,
    pub evidence_expired: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Occurrence {
    id: Uuid,
    day: NaiveDate,
    ended_at: DateTime<Utc>,
    steps: Vec<ContextStep>,
    event_ids: Vec<Uuid>,
}
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    events: Vec<ContextEvent>,
    occurrences: Vec<Occurrence>,
    workflows: Vec<ContextWorkflow>,
    capacity_gap: Option<ContextEvent>,
}

pub struct ContextStore {
    path: PathBuf,
    _lock: File,
    key: aead::LessSafeKey,
    state: State,
    max_bytes: usize,
}
impl ContextStore {
    pub fn open(path: &Path, key: [u8; 32]) -> Result<Self> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        // Refuse redirected files; user data must stay in the selected OS-profile directory.
        for target in [parent, path] {
            if fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(ContextError::Invalid("context path is a symlink"));
            }
        }
        let lock_path = path.with_extension("context-lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(ContextError::Invalid("context lock is a symlink"));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(&lock_path)?;
        lock.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => ContextError::Busy,
            std::fs::TryLockError::Error(e) => ContextError::Io(e),
        })?;
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, &key)
                .map_err(|_| ContextError::Encryption)?,
        );
        let mut store = Self {
            path: path.to_path_buf(),
            _lock: lock,
            key,
            state: State::default(),
            max_bytes: MAX_STORE_BYTES,
        };
        if path.exists() {
            store.state = store.read()?;
        }
        Ok(store)
    }
    pub fn append(&mut self, event: ContextEvent) -> Result<AppendOutcome> {
        event.validate()?;
        ensure(
            !self.state.events.iter().any(|e| e.id == event.id),
            "duplicate context event",
        )?;
        let mut next = self.state.clone();
        next.events.push(event.clone());
        next.events.sort_by_key(|e| (e.at, e.id));
        rebuild_occurrences(&mut next);
        match self.write(&next, CAPACITY_RESERVE) {
            Err(ContextError::Capacity) => {
                let mut gap = self.state.clone();
                gap.capacity_gap = Some(ContextEvent::coverage_gap(event.at, Gap::Capacity));
                self.write(&gap, 0)?;
                self.state = gap;
                return Ok(AppendOutcome::Capacity);
            }
            result => result?,
        }
        self.state = next;
        Ok(AppendOutcome::Accepted)
    }
    pub fn events(&self) -> Vec<ContextEvent> {
        let mut events = self.state.events.clone();
        if let Some(gap) = &self.state.capacity_gap {
            events.push(gap.clone());
        }
        events.sort_by_key(|e| (e.at, e.id));
        events
    }
    pub fn delete(&mut self, ids: &[Uuid]) -> Result<usize> {
        let ids: HashSet<_> = ids.iter().copied().collect();
        let mut next = self.state.clone();
        let before = next.events.len() + usize::from(next.capacity_gap.is_some());
        next.events.retain(|e| !ids.contains(&e.id));
        if next
            .capacity_gap
            .as_ref()
            .is_some_and(|e| ids.contains(&e.id))
        {
            next.capacity_gap = None;
        }
        let removed: HashSet<_> = next
            .occurrences
            .iter()
            .filter(|o| o.event_ids.iter().any(|id| ids.contains(id)))
            .map(|o| o.id)
            .collect();
        next.occurrences.retain(|o| !removed.contains(&o.id));
        for workflow in &mut next.workflows {
            workflow.episode_ids.retain(|id| !removed.contains(id));
        }
        self.write(&next, 0)?;
        self.state = next;
        Ok(before - self.state.events.len() - usize::from(self.state.capacity_gap.is_some()))
    }
    /// Explicitly erase all local context, including expired derived evidence and saved workflows.
    pub fn clear(&mut self) -> Result<()> {
        let empty = State::default();
        self.write(&empty, 0)?;
        self.state = empty;
        Ok(())
    }
    /// Content expires in 48h. Minimized derived occurrences last 30d; no exact focus times leave the store.
    pub fn prune(&mut self, now: DateTime<Utc>) -> Result<usize> {
        let mut next = self.state.clone();
        let before = next.events.len();
        next.events.retain(|e| e.at >= now - Duration::hours(48));
        next.occurrences
            .retain(|o| o.ended_at >= now - Duration::days(30));
        let live: HashSet<_> = next.events.iter().map(|e| e.id).collect();
        let supported: HashSet<_> = next
            .occurrences
            .iter()
            .filter(|o| o.event_ids.iter().all(|id| live.contains(id)))
            .map(|o| o.id)
            .collect();
        for workflow in &mut next.workflows {
            workflow.episode_ids.retain(|id| supported.contains(id));
        }
        if next
            .capacity_gap
            .as_ref()
            .is_some_and(|e| e.at < now - Duration::hours(48))
        {
            next.capacity_gap = None;
        }
        self.write(&next, 0)?;
        self.state = next;
        Ok(before - self.state.events.len())
    }
    pub fn save_workflow(&mut self, workflow: ContextWorkflow) -> Result<()> {
        workflow.validate()?;
        ensure(
            workflow
                .episode_ids
                .iter()
                .all(|id| self.state.occurrences.iter().any(|o| o.id == *id)),
            "unknown workflow evidence",
        )?;
        let mut next = self.state.clone();
        next.workflows.retain(|w| w.id != workflow.id);
        next.workflows.push(workflow);
        self.write(&next, CAPACITY_RESERVE)?;
        self.state = next;
        Ok(())
    }
    pub fn delete_workflow(&mut self, id: Uuid) -> Result<()> {
        let mut next = self.state.clone();
        next.workflows.retain(|w| w.id != id);
        self.write(&next, 0)?;
        self.state = next;
        Ok(())
    }
    pub fn workflows(&self) -> Vec<ContextWorkflow> {
        self.state.workflows.clone()
    }
    pub fn candidates(&self) -> Vec<ContextCandidate> {
        let mut groups: BTreeMap<String, Vec<&Occurrence>> = BTreeMap::new();
        for occurrence in &self.state.occurrences {
            if occurrence.steps.len() < 2 {
                continue;
            }
            let signature =
                serde_json::to_string(&occurrence.steps).expect("normalized steps serialize");
            groups.entry(signature).or_default().push(occurrence);
        }
        let live: HashSet<_> = self.state.events.iter().map(|e| e.id).collect();
        groups
            .into_iter()
            .filter_map(|(signature, occurrences)| {
                let days: HashSet<_> = occurrences.iter().map(|o| o.day).collect();
                if occurrences.len() < 3 || days.len() < 2 {
                    return None;
                }
                let event_ids: Vec<_> = occurrences
                    .iter()
                    .flat_map(|o| o.event_ids.clone())
                    .collect();
                let hash = digest::digest(&digest::SHA256, signature.as_bytes());
                Some(ContextCandidate {
                    id: Uuid::from_bytes(hash.as_ref()[..16].try_into().expect("digest length")),
                    steps: occurrences[0].steps.clone(),
                    observed_days: days.len().min(30) as u8,
                    repeat_count: occurrences.len().min(10000) as u16,
                    evidence_expired: event_ids.iter().any(|id| !live.contains(id)),
                    event_ids,
                    episodes: occurrences
                        .iter()
                        .take(50)
                        .map(|o| ContextEpisode {
                            id: o.id,
                            day: o.ended_at.date_naive(),
                            repeat_count: 1,
                            extraction_version: 1,
                            steps: o.steps.clone(),
                            gaps: if o.event_ids.iter().any(|id| !live.contains(id)) {
                                vec![Gap::InsufficientEvidence]
                            } else {
                                vec![]
                            },
                        })
                        .collect(),
                })
            })
            .collect()
    }
    fn read(&self) -> Result<State> {
        let file = File::open(&self.path)?;
        ensure(
            file.metadata()?.len() <= self.max_bytes as u64,
            "context store too large",
        )?;
        let mut bytes = Vec::new();
        file.take(self.max_bytes as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure(
            bytes.len() >= ENVELOPE_BYTES && bytes.starts_with(HEADER),
            "invalid encrypted store",
        )?;
        let nonce: [u8; 12] = bytes[HEADER.len()..HEADER.len() + 12]
            .try_into()
            .map_err(|_| ContextError::Encryption)?;
        let plaintext = self
            .key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(HEADER),
                &mut bytes[HEADER.len() + 12..],
            )
            .map_err(|_| ContextError::Encryption)?;
        let state: State = serde_json::from_slice(plaintext)?;
        for event in &state.events {
            event.validate()?;
        }
        for workflow in &state.workflows {
            workflow.validate()?;
        }
        Ok(state)
    }
    fn write(&self, state: &State, reserve: usize) -> Result<()> {
        let mut plaintext = serde_json::to_vec(state)?;
        if plaintext.len() + ENVELOPE_BYTES + reserve > self.max_bytes {
            return Err(ContextError::Capacity);
        }
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| ContextError::Encryption)?;
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(HEADER),
                &mut plaintext,
            )
            .map_err(|_| ContextError::Encryption)?;
        let temp = self.path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = private_file(&temp)?;
            file.write_all(HEADER)?;
            file.write_all(&nonce)?;
            file.write_all(&plaintext)?;
            file.sync_all()?;
            fs::rename(&temp, &self.path)?;
            #[cfg(unix)]
            File::open(
                self.path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )?
            .sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}
fn private_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn rebuild_occurrences(state: &mut State) {
    let live: HashSet<_> = state.events.iter().map(|e| e.id).collect();
    // Preserve expired summaries; recompute only occurrences backed entirely by live events.
    state
        .occurrences
        .retain(|o| o.event_ids.iter().any(|id| !live.contains(id)));
    let summarized: HashSet<_> = state
        .occurrences
        .iter()
        .flat_map(|o| o.event_ids.iter().copied())
        .collect();
    let mut current: Option<Occurrence> = None;
    for event in &state.events {
        if summarized.contains(&event.id) {
            continue;
        }
        if event.gap.is_some() {
            if let Some(o) = current.take() {
                state.occurrences.push(o);
            }
            continue;
        }
        let day = event.at.with_timezone(&Local).date_naive();
        let split = current.as_ref().is_some_and(|o| {
            event.at - o.ended_at > Duration::minutes(5) || o.day != day || o.steps.len() >= 30
        });
        if split {
            state
                .occurrences
                .push(current.take().expect("current occurrence"));
        }
        let occurrence = current.get_or_insert_with(|| Occurrence {
            id: event.id,
            day,
            ended_at: event.at,
            steps: vec![],
            event_ids: vec![],
        });
        let step = ContextStep {
            ordinal: occurrence.steps.len() as u8,
            app_id: event.app_id.clone(),
            action: event.action,
            object_category: event.object_category,
            provenance: event.provenance,
        };
        if !occurrence.steps.last().is_some_and(|s| {
            s.app_id == step.app_id
                && s.action == step.action
                && s.object_category == step.object_category
        }) {
            occurrence.steps.push(step);
        }
        occurrence.event_ids.push(event.id);
        occurrence.ended_at = event.at;
    }
    if let Some(o) = current {
        state.occurrences.push(o);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn event(app: &str, at: DateTime<Utc>) -> ContextEvent {
        ContextEvent {
            id: Uuid::new_v4(),
            at,
            app_id: app.into(),
            app_name: "Synthetic app".into(),
            action: Action::Navigate,
            object_category: ObjectCategory::Unknown,
            provenance: Provenance::Observed,
            gap: None,
        }
    }
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("monitor-context-{}.enc", Uuid::new_v4()))
    }
    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 29, 10, 0, 0).unwrap()
    }
    #[test]
    fn encrypted_reopen_authenticates_and_never_writes_plaintext() {
        let path = path();
        let mut store = ContextStore::open(&path, [7; 32]).unwrap();
        let sample = event("com.example.sensitive-marker", at());
        store.append(sample.clone()).unwrap();
        assert!(
            !String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("sensitive-marker")
        );
        drop(store);
        assert!(ContextStore::open(&path, [8; 32]).is_err());
        let store = ContextStore::open(&path, [7; 32]).unwrap();
        assert_eq!(store.events(), vec![sample]);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn candidates_require_three_sequences_two_days_and_deleted_evidence_disappears() {
        let path = path();
        let mut store = ContextStore::open(&path, [7; 32]).unwrap();
        for minutes in [0, 10, 20] {
            for (i, app) in ["a", "b"].iter().enumerate() {
                store
                    .append(event(
                        app,
                        at() + Duration::minutes(minutes) + Duration::seconds(i as i64),
                    ))
                    .unwrap();
            }
        }
        assert!(store.candidates().is_empty());
        for (i, app) in ["a", "b"].iter().enumerate() {
            store
                .append(event(
                    app,
                    at() + Duration::days(1) + Duration::seconds(i as i64),
                ))
                .unwrap();
        }
        let candidates = store.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].repeat_count, 4);
        assert_eq!(candidates[0].episodes[0].day, at().date_naive());
        let workflow = ContextWorkflow {
            id: Uuid::new_v4(),
            name: "Reviewed workflow".into(),
            purpose: String::new(),
            app_ids: vec!["a".into(), "b".into()],
            steps: vec!["Reviewed business step".into()],
            exceptions: String::new(),
            required_features: String::new(),
            frequency_per_week: None,
            minutes_per_run: None,
            confirmed: true,
            owns_custom_code: None,
            episode_ids: vec![candidates[0].episodes.last().unwrap().id],
        };
        store.save_workflow(workflow.clone()).unwrap();
        let ids: Vec<_> = store
            .events()
            .into_iter()
            .filter(|e| e.at.date_naive() != at().date_naive())
            .map(|e| e.id)
            .collect();
        store.delete(&ids).unwrap();
        assert!(store.candidates().is_empty());
        assert_eq!(store.workflows()[0].name, workflow.name);
        assert!(store.workflows()[0].episode_ids.is_empty());
        store.prune(at() + Duration::days(31)).unwrap();
        assert!(store.events().is_empty());
        assert!(store.candidates().is_empty());
        store.clear().unwrap();
        drop(store);
        let store = ContextStore::open(&path, [7; 32]).unwrap();
        assert!(store.workflows().is_empty());
        assert!(store.events().is_empty());
        assert!(store.candidates().is_empty());
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn partial_expiry_never_reuses_an_occurrence_tail_as_new_evidence() {
        let path = path();
        let mut store = ContextStore::open(&path, [7; 32]).unwrap();
        for day in 0..3 {
            for (i, app) in ["a", "b", "c"].iter().enumerate() {
                store
                    .append(event(
                        app,
                        at() + Duration::days(day) + Duration::seconds(i as i64),
                    ))
                    .unwrap();
            }
        }
        for day in 0..3 {
            store
                .prune(at() + Duration::days(day + 2) + Duration::milliseconds(500))
                .unwrap();
            store
                .append(ContextEvent::coverage_gap(
                    at() + Duration::days(day + 3),
                    Gap::Excluded,
                ))
                .unwrap();
        }
        let candidates = store.candidates();
        assert_eq!(
            candidates.len(),
            1,
            "expired tails must not create a second candidate"
        );
        assert!(candidates[0].evidence_expired);
        assert_eq!(candidates[0].repeat_count, 3);
        let id = candidates[0].event_ids[0];
        drop(store);
        let mut store = ContextStore::open(&path, [7; 32]).unwrap();
        assert!(store.candidates()[0].evidence_expired);
        store.delete(&[id]).unwrap();
        assert!(
            store.candidates().is_empty(),
            "deletion applies even after raw expiry"
        );
        // Derived occurrences that no longer qualify as a candidate must also be erasable.
        store.clear().unwrap();
        drop(store);
        let store = ContextStore::open(&path, [7; 32]).unwrap();
        assert!(store.state.occurrences.is_empty());
        assert!(store.events().is_empty());
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn capacity_drops_new_content_and_retains_a_visible_gap() {
        let path = path();
        let mut store = ContextStore::open(&path, [7; 32]).unwrap();
        store.max_bytes = 1024;
        store.append(event("a", at())).unwrap();
        let result = store
            .append(event(&"b".repeat(200), at() + Duration::seconds(1)))
            .unwrap();
        assert_eq!(result, AppendOutcome::Capacity);
        assert_eq!(
            store
                .events()
                .iter()
                .filter(|e| e.gap == Some(Gap::Capacity))
                .count(),
            1
        );
        assert!(!store.events().iter().any(|e| e.app_id.starts_with('b')));
        let gap = store
            .events()
            .into_iter()
            .find(|e| e.gap == Some(Gap::Capacity))
            .unwrap();
        assert_eq!(
            store
                .events()
                .into_iter()
                .find(|e| e.gap == Some(Gap::Capacity))
                .unwrap()
                .id,
            gap.id
        );
        assert_eq!(store.delete(&[gap.id]).unwrap(), 1);
        assert!(!store.events().iter().any(|e| e.gap.is_some()));
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}
