// In-memory session store for the two-phase NLQ pipeline (`POST /v1/nlq/prepare` +
// `POST /v1/nlq/complete`).
//
// Why in-memory and not a Postgres table: this repo's `cargo test --workspace --lib --bins`
// CI job runs with no live Postgres instance, and the session is inherently short-lived,
// single-process-scoped, correlation-only data — not something that needs to survive a
// process restart or be shared across replicas (ADR-027 already frames the whole LLM config
// surface as local-development-targeted, not high-availability production infra).
//
// The server is the sole authority over `repair_attempt`: callers never supply it, and it is
// only ever bumped by `NlqSessionStore` bookkeeping in response to a `NeedsRepair` outcome.

use crate::llm_adapter::NlqQueryRequest;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How long a prepared session may sit idle before it's treated as expired (lazily evicted
/// on next lookup — no background sweep task).
pub const SESSION_TTL: Duration = Duration::from_secs(10 * 60);

/// A single in-flight NLQ pipeline session, spanning a `/prepare` call and one or more
/// `/complete` calls (one per repair turn).
pub struct NlqPipelineSession {
    pub tenant_id: Uuid,
    /// The original request, so `resume_nlq_pipeline` can use it on `/complete`.
    pub req: NlqQueryRequest,
    /// The `question` `prepare_nlq_pipeline` computed.
    pub original_question: String,
    pub repair_attempt: usize,
    pub created_at: Instant,
}

impl NlqPipelineSession {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > SESSION_TTL
    }
}

#[derive(Clone, Default)]
pub struct NlqSessionStore {
    inner: Arc<Mutex<HashMap<Uuid, NlqPipelineSession>>>,
}

impl NlqSessionStore {
    /// Stores a newly prepared session and returns its opaque token.
    pub fn insert(&self, tenant_id: Uuid, req: NlqQueryRequest, original_question: String) -> Uuid {
        let token = Uuid::new_v4();
        let session = NlqPipelineSession {
            tenant_id,
            req,
            original_question,
            repair_attempt: 0,
            created_at: Instant::now(),
        };
        self.inner
            .lock()
            .expect("nlq session store mutex poisoned")
            .insert(token, session);
        token
    }

    /// Removes and returns the session for `token`, if it exists, is not expired, and belongs
    /// to `tenant_id`. Returns `None` on any of: unknown token, expired session (lazily evicted
    /// as a side effect), or a `tenant_id` mismatch (the never-trust-the-client tenant check —
    /// in this case the session is left in place for its rightful owner, not evicted).
    ///
    /// The session is removed from the store on a successful lookup — callers that want to
    /// keep it alive for another repair turn must call [`Self::put_back`] with the (mutated)
    /// session before returning.
    pub fn take_for_resume(&self, token: Uuid, tenant_id: Uuid) -> Option<NlqPipelineSession> {
        let mut map = self.inner.lock().expect("nlq session store mutex poisoned");
        match map.entry(token) {
            Entry::Occupied(entry) => {
                if entry.get().is_expired() {
                    entry.remove();
                    return None;
                }
                if entry.get().tenant_id != tenant_id {
                    return None;
                }
                Some(entry.remove())
            }
            Entry::Vacant(_) => None,
        }
    }

    /// Re-inserts a session under the same token — used after a `NeedsRepair` outcome so the
    /// client can call `/complete` again with the next raw LLM response.
    pub fn put_back(&self, token: Uuid, session: NlqPipelineSession) {
        self.inner
            .lock()
            .expect("nlq session store mutex poisoned")
            .insert(token, session);
    }

    /// Number of currently-stored sessions, including any that are expired but not yet
    /// lazily evicted. Test-only convenience.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("nlq session store mutex poisoned")
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_adapter::NlqQueryMode;

    fn req(question: &str) -> NlqQueryRequest {
        NlqQueryRequest {
            question: Some(question.to_string()),
            service_name: None,
            base_ir: None,
            mode: NlqQueryMode::Execute,
        }
    }

    #[test]
    fn insert_then_take_succeeds() {
        let store = NlqSessionStore::default();
        let tenant_id = Uuid::new_v4();
        let token = store.insert(tenant_id, req("how many errors"), "how many errors".into());

        let session = store
            .take_for_resume(token, tenant_id)
            .expect("session should be retrievable immediately after insert");
        assert_eq!(session.tenant_id, tenant_id);
        assert_eq!(session.original_question, "how many errors");
        assert_eq!(session.repair_attempt, 0);
        assert_eq!(store.len(), 0, "take_for_resume should remove the session");
    }

    #[test]
    fn take_with_wrong_tenant_returns_none_and_keeps_session() {
        let store = NlqSessionStore::default();
        let tenant_id = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let token = store.insert(tenant_id, req("q"), "q".into());

        assert!(store.take_for_resume(token, other_tenant).is_none());
        assert_eq!(
            store.len(),
            1,
            "wrong-tenant lookup must not evict the session"
        );

        // The rightful owner can still retrieve it afterward.
        assert!(store.take_for_resume(token, tenant_id).is_some());
    }

    #[test]
    fn take_with_unknown_token_returns_none() {
        let store = NlqSessionStore::default();
        assert!(
            store
                .take_for_resume(Uuid::new_v4(), Uuid::new_v4())
                .is_none()
        );
    }

    #[test]
    fn take_with_expired_session_returns_none_and_evicts() {
        let store = NlqSessionStore::default();
        let tenant_id = Uuid::new_v4();
        let token = Uuid::new_v4();
        let mut session = NlqPipelineSession {
            tenant_id,
            req: req("q"),
            original_question: "q".into(),
            repair_attempt: 0,
            created_at: Instant::now(),
        };
        // Backdate creation past the TTL without a real sleep.
        session.created_at -= SESSION_TTL + Duration::from_secs(1);
        store.put_back(token, session);

        assert!(store.take_for_resume(token, tenant_id).is_none());
        assert_eq!(
            store.len(),
            0,
            "expired session should be evicted on lookup"
        );
    }

    #[test]
    fn put_back_allows_resume_with_incremented_repair_attempt() {
        let store = NlqSessionStore::default();
        let tenant_id = Uuid::new_v4();
        let token = store.insert(tenant_id, req("q"), "q".into());

        let mut session = store.take_for_resume(token, tenant_id).unwrap();
        session.repair_attempt += 1;
        store.put_back(token, session);

        let session = store
            .take_for_resume(token, tenant_id)
            .expect("session should still be present after put_back");
        assert_eq!(session.repair_attempt, 1);
    }
}
