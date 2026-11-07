//! `lookup-principals` contract: resolve user principals of a tenant to `active`, `departed`
//! (known user without a role in the tenant) or `unknown`. Wire types, HTTP client and the
//! provider-owned programmable mock.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalState {
    Active,
    Departed,
    Unknown,
}

/// `POST /internal/principals/resolve`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub tenant_id: Uuid,
    pub user_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPrincipal {
    pub user_id: Uuid,
    pub state: PrincipalState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolveResponse {
    pub principals: Vec<ResolvedPrincipal>,
}

/// Maximum principals per request.
pub const MAX_RESOLVE: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryUnavailable;

#[allow(async_fn_in_trait)]
pub trait PrincipalDirectory {
    /// Returns one entry per requested id, in request order (duplicates collapsed).
    async fn resolve(
        &self,
        tenant_id: Uuid,
        user_ids: &[Uuid],
    ) -> Result<Vec<ResolvedPrincipal>, DirectoryUnavailable>;
}

/// Order-preserving de-duplication used by the provider and the mock.
pub fn dedup(ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

#[derive(Clone)]
pub struct HttpPrincipalDirectory {
    base_url: String,
    internal_token: String,
    client: reqwest::Client,
}

impl HttpPrincipalDirectory {
    pub fn new(
        base_url: impl Into<String>,
        internal_token: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            internal_token: internal_token.into(),
            client,
        }
    }
}

impl PrincipalDirectory for HttpPrincipalDirectory {
    async fn resolve(
        &self,
        tenant_id: Uuid,
        user_ids: &[Uuid],
    ) -> Result<Vec<ResolvedPrincipal>, DirectoryUnavailable> {
        let response = self
            .client
            .post(format!("{}/internal/principals/resolve", self.base_url))
            .header("X-Internal-Token", &self.internal_token)
            .json(&ResolveRequest {
                tenant_id,
                user_ids: user_ids.to_vec(),
            })
            .send()
            .await
            .map_err(|_| DirectoryUnavailable)?;
        if response.status().as_u16() != 200 {
            return Err(DirectoryUnavailable);
        }
        let body: ResolveResponse = response.json().await.map_err(|_| DirectoryUnavailable)?;
        Ok(body.principals)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MockScenario {
    #[default]
    Directory,
    Unavailable,
    Delay(Duration),
}

#[derive(Default)]
struct MockState {
    scenario: MockScenario,
    users: HashSet<Uuid>,
    members: HashMap<Uuid, HashSet<Uuid>>,
}

/// Provider-owned programmable mock of `lookup-principals`.
#[derive(Clone, Default)]
pub struct MockPrincipalDirectory {
    state: Arc<Mutex<MockState>>,
}

impl MockPrincipalDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut MockState) -> R) -> R {
        f(&mut self.state.lock().expect("mock state poisoned"))
    }

    pub fn reset(&self) {
        self.with(|s| *s = MockState::default());
    }

    pub fn set_scenario(&self, scenario: MockScenario) {
        self.with(|s| s.scenario = scenario);
    }

    /// A known user without membership in any tenant (departed everywhere).
    pub fn add_user(&self, user_id: Uuid) {
        self.with(|s| {
            s.users.insert(user_id);
        });
    }

    pub fn add_member(&self, tenant_id: Uuid, user_id: Uuid) {
        self.with(|s| {
            s.users.insert(user_id);
            s.members.entry(tenant_id).or_default().insert(user_id);
        });
    }
}

impl MockPrincipalDirectory {
    /// The user leaves the tenant: still known, so it resolves `departed` there.
    pub fn remove_member(&self, tenant_id: Uuid, user_id: Uuid) {
        self.with(|s| {
            if let Some(m) = s.members.get_mut(&tenant_id) {
                m.remove(&user_id);
            }
        });
    }
}

impl PrincipalDirectory for MockPrincipalDirectory {
    async fn resolve(
        &self,
        tenant_id: Uuid,
        user_ids: &[Uuid],
    ) -> Result<Vec<ResolvedPrincipal>, DirectoryUnavailable> {
        match self.with(|s| s.scenario) {
            MockScenario::Unavailable => return Err(DirectoryUnavailable),
            MockScenario::Delay(d) => tokio::time::sleep(d).await,
            MockScenario::Directory => {}
        }
        if user_ids.len() > MAX_RESOLVE {
            return Err(DirectoryUnavailable);
        }
        Ok(self.with(|s| {
            dedup(user_ids)
                .into_iter()
                .map(|user_id| {
                    let state = if s
                        .members
                        .get(&tenant_id)
                        .is_some_and(|m| m.contains(&user_id))
                    {
                        PrincipalState::Active
                    } else if s.users.contains(&user_id) {
                        PrincipalState::Departed
                    } else {
                        PrincipalState::Unknown
                    };
                    ResolvedPrincipal { user_id, state }
                })
                .collect()
        }))
    }
}
