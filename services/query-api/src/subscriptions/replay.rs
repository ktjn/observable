//! Bounded in-memory replay so a dropped connection can resume from
//! `Last-Event-ID` instead of always resynchronizing (PUSH-006).
//!
//! Nothing here is durable: no table, no cache server, no lease. A stream's
//! recently-sent `(id, event)` pairs are held in process memory for at most
//! `RETENTION` after the connection ends, and only so a *reconnect* can resume --
//! the design's `Last-Event-ID` resume is unsatisfiable if the buffer dies with the
//! connection that produced it, since a reconnect is by definition a new
//! connection. The retained value is event bytes only; it is never an
//! authorization authority. A resume re-validates the session from scratch and
//! additionally requires the tenant and every view's query revision to match, so a
//! retained buffer can never deliver another tenant's or another query's data.

use super::protocol::SseEvent;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Recently-sent data events kept per stream.
pub const REPLAY_CAPACITY: usize = 64;

/// How long a closed stream's buffer stays resumable.
pub const RETENTION: Duration = Duration::from_secs(60);

/// Ceiling on retained closed streams per process (PUSH-008: bounded memory).
pub const MAX_RETAINED_STREAMS: usize = 1024;

/// The identity of one SSE stream, carried in each event's `id:` so a reconnect
/// can name the buffer it wants to resume from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamId(String);

impl StreamId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Formats the SSE `id:` for a sequence number in this stream.
    pub fn event_id(&self, seq: u64) -> String {
        format!("{}.{seq}", self.0)
    }
}

impl Default for StreamId {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed `Last-Event-ID` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub stream: StreamId,
    pub seq: u64,
}

impl Cursor {
    pub fn parse(raw: &str) -> Option<Self> {
        let (stream, seq) = raw.rsplit_once('.')?;
        if stream.is_empty() || !stream.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            stream: StreamId(stream.to_string()),
            seq: seq.parse().ok()?,
        })
    }
}

/// What a stream must still match for its retained buffer to be resumable: the
/// same tenant and the same set of view query revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeKey {
    pub tenant_id: Uuid,
    /// `(view_id, query_revision)` pairs, sorted.
    pub views: Vec<(String, String)>,
}

impl ResumeKey {
    pub fn new(tenant_id: Uuid, mut views: Vec<(String, String)>) -> Self {
        views.sort();
        Self { tenant_id, views }
    }
}

/// The per-connection ring of recently-sent data events.
#[derive(Debug, Default)]
pub struct ReplayBuffer {
    entries: VecDeque<(u64, SseEvent)>,
}

impl ReplayBuffer {
    pub fn push(&mut self, seq: u64, event: SseEvent) {
        if self.entries.len() == REPLAY_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((seq, event));
    }

    /// Events strictly after `seq`, or `None` when `seq` is no longer covered by
    /// the buffer and continuity therefore cannot be proven.
    pub fn since(&self, seq: u64) -> Option<Vec<(u64, SseEvent)>> {
        let covers = self
            .entries
            .front()
            .is_some_and(|(first, _)| *first <= seq.saturating_add(1))
            && self.entries.back().is_some_and(|(last, _)| *last >= seq);
        if !covers {
            return None;
        }
        Some(
            self.entries
                .iter()
                .filter(|(s, _)| *s > seq)
                .cloned()
                .collect(),
        )
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

struct Retained {
    key: ResumeKey,
    buffer: ReplayBuffer,
    next_seq: u64,
    retained_at: Instant,
}

/// What a reconnect may do with its `Last-Event-ID`.
#[derive(Debug, PartialEq)]
pub enum Resume {
    /// Continue the named stream: replay these events, then carry on from `next_seq`.
    Replay {
        events: Vec<(u64, SseEvent)>,
        next_seq: u64,
    },
    /// Continuity cannot be proven; the caller emits `reset` and a fresh snapshot.
    Reset,
}

/// Process-local retention of closed streams' replay buffers.
#[derive(Clone, Default)]
pub struct ReplayStore {
    inner: Arc<Mutex<HashMap<String, Retained>>>,
}

impl ReplayStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hands a finished connection's buffer over for possible resume.
    pub fn retain(&self, stream: &StreamId, key: ResumeKey, buffer: ReplayBuffer, next_seq: u64) {
        if buffer.is_empty() {
            return;
        }
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        Self::evict(&mut guard);
        if guard.len() >= MAX_RETAINED_STREAMS {
            return;
        }
        guard.insert(
            stream.as_str().to_string(),
            Retained {
                key,
                buffer,
                next_seq,
                retained_at: Instant::now(),
            },
        );
    }

    /// Claims a retained buffer for a reconnect. A hit requires the cursor's
    /// stream to still be retained, the resume key (tenant + every view's query
    /// revision) to match, and the cursor to be inside the buffer's window.
    pub fn resume(&self, cursor: &Cursor, key: &ResumeKey) -> Resume {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        Self::evict(&mut guard);
        let Some(retained) = guard.get(cursor.stream.as_str()) else {
            return Resume::Reset;
        };
        if retained.key != *key {
            return Resume::Reset;
        }
        let Some(events) = retained.buffer.since(cursor.seq) else {
            return Resume::Reset;
        };
        let next_seq = retained.next_seq;
        guard.remove(cursor.stream.as_str());
        Resume::Replay { events, next_seq }
    }

    fn evict(guard: &mut HashMap<String, Retained>) {
        guard.retain(|_, retained| retained.retained_at.elapsed() < RETENTION);
    }

    #[cfg(test)]
    fn retained_count(&self) -> usize {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        Self::evict(&mut guard);
        guard.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(revision: u64) -> SseEvent {
        SseEvent::Replace {
            view_id: "v".into(),
            query_revision: "r1".into(),
            revision,
            data: json!({"n": revision}),
        }
    }

    fn key() -> ResumeKey {
        ResumeKey::new(
            Uuid::from_u128(7),
            vec![("v".to_string(), "r1".to_string())],
        )
    }

    fn filled(upto: u64) -> (StreamId, ReplayBuffer) {
        let mut buffer = ReplayBuffer::default();
        for seq in 1..=upto {
            buffer.push(seq, event(seq));
        }
        (StreamId::new(), buffer)
    }

    #[test]
    fn an_event_id_round_trips_through_a_cursor() {
        let stream = StreamId::new();
        let cursor = Cursor::parse(&stream.event_id(42)).unwrap();
        assert_eq!(cursor.stream, stream);
        assert_eq!(cursor.seq, 42);
    }

    #[test]
    fn a_malformed_last_event_id_is_not_a_cursor() {
        for raw in ["", "nodot", "zzz.1", ".5", "abc.notanumber"] {
            assert!(Cursor::parse(raw).is_none(), "{raw} parsed as a cursor");
        }
    }

    #[test]
    fn the_ring_buffer_keeps_only_the_most_recent_events() {
        let (_, buffer) = filled(REPLAY_CAPACITY as u64 + 10);
        assert_eq!(buffer.len(), REPLAY_CAPACITY);
        // The evicted prefix is no longer resumable.
        assert!(buffer.since(1).is_none());
    }

    #[test]
    fn a_covered_cursor_replays_only_later_events() {
        let (_, buffer) = filled(5);
        let events = buffer.since(3).unwrap();
        assert_eq!(
            events.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![4, 5]
        );
    }

    #[test]
    fn a_cursor_at_the_head_replays_nothing_and_is_still_covered() {
        let (_, buffer) = filled(5);
        assert_eq!(buffer.since(5).unwrap(), vec![]);
    }

    #[test]
    fn a_cursor_beyond_the_buffer_is_not_covered() {
        let (_, buffer) = filled(5);
        assert!(buffer.since(9).is_none());
    }

    #[test]
    fn a_retained_stream_resumes_from_its_cursor() {
        let store = ReplayStore::new();
        let (stream, buffer) = filled(5);
        store.retain(&stream, key(), buffer, 6);

        let cursor = Cursor::parse(&stream.event_id(3)).unwrap();
        match store.resume(&cursor, &key()) {
            Resume::Replay { events, next_seq } => {
                assert_eq!(
                    events.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                    vec![4, 5]
                );
                assert_eq!(next_seq, 6);
            }
            Resume::Reset => panic!("a retained, matching stream must resume"),
        }
    }

    #[test]
    fn a_resumed_stream_is_claimed_once_so_two_tabs_cannot_share_it() {
        let store = ReplayStore::new();
        let (stream, buffer) = filled(5);
        store.retain(&stream, key(), buffer, 6);
        let cursor = Cursor::parse(&stream.event_id(3)).unwrap();

        assert!(matches!(
            store.resume(&cursor, &key()),
            Resume::Replay { .. }
        ));
        assert_eq!(store.resume(&cursor, &key()), Resume::Reset);
    }

    #[test]
    fn an_unknown_stream_resets() {
        let store = ReplayStore::new();
        let cursor = Cursor::parse(&StreamId::new().event_id(1)).unwrap();
        assert_eq!(store.resume(&cursor, &key()), Resume::Reset);
    }

    #[test]
    fn another_tenants_cursor_never_replays_a_retained_buffer() {
        let store = ReplayStore::new();
        let (stream, buffer) = filled(5);
        store.retain(&stream, key(), buffer, 6);

        let other_tenant = ResumeKey::new(
            Uuid::from_u128(999),
            vec![("v".to_string(), "r1".to_string())],
        );
        let cursor = Cursor::parse(&stream.event_id(3)).unwrap();
        assert_eq!(store.resume(&cursor, &other_tenant), Resume::Reset);
    }

    #[test]
    fn a_changed_query_revision_resets_instead_of_replaying_stale_results() {
        let store = ReplayStore::new();
        let (stream, buffer) = filled(5);
        store.retain(&stream, key(), buffer, 6);

        let changed = ResumeKey::new(
            Uuid::from_u128(7),
            vec![("v".to_string(), "r2".to_string())],
        );
        let cursor = Cursor::parse(&stream.event_id(3)).unwrap();
        assert_eq!(store.resume(&cursor, &changed), Resume::Reset);
    }

    #[test]
    fn an_empty_buffer_is_not_retained() {
        let store = ReplayStore::new();
        let stream = StreamId::new();
        store.retain(&stream, key(), ReplayBuffer::default(), 1);
        assert_eq!(store.retained_count(), 0);
    }
}
