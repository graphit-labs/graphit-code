//! Observations and administrative hints for the existing project supervisor, never a scheduler.
use graphit_daemon::supervisor::{ProjectRescanHandle, SupervisionKey};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    io,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Default, Serialize)]
pub(crate) struct Phase {
    pub state: String,
    pub observed_at: Option<u128>,
    pub last_completed_at: Option<u128>,
    pub last_error: Option<String>,
    pub generation: Option<u64>,
}
#[derive(Clone, Default, Serialize)]
pub(crate) struct Observation {
    pub phases: BTreeMap<String, Phase>,
    pub request_sequence: u64,
    pub completed_request_sequence: u64,
    pub running: bool,
    pub last_batch_error: Option<String>,
}
#[derive(Default)]
struct State {
    handles: Vec<ProjectRescanHandle>,
    observations: HashMap<SupervisionKey, Observation>,
}
#[derive(Default)]
pub(crate) struct IndexControl {
    state: Mutex<State>,
    pub changed: tokio::sync::Notify,
}
pub(crate) struct BatchObservation {
    control: Arc<IndexControl>,
    key: SupervisionKey,
    ticket: u64,
    finished: bool,
}
impl BatchObservation {
    pub fn finish(mut self) {
        self.finished = true;
    }
}
impl Drop for BatchObservation {
    fn drop(&mut self) {
        if let Ok(mut data) = self.control.state.lock() {
            let value = data.observations.entry(self.key.clone()).or_default();
            value.running = false;
            value.completed_request_sequence = value.completed_request_sequence.max(self.ticket);
            value.last_batch_error = (!self.finished).then(|| {
                "Producer batch interrupted before normal completion; inspect phase errors and supervisor availability".to_owned()
            });
            if let Some(error) = &value.last_batch_error {
                for phase in value
                    .phases
                    .values_mut()
                    .filter(|phase| phase.state == "running")
                {
                    phase.state = "failed".to_owned();
                    phase.observed_at = now();
                    phase.last_error = Some(error.clone());
                }
            }
        }
        self.control.changed.notify_waiters();
    }
}
fn now() -> Option<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|v| v.as_millis())
}
impl IndexControl {
    pub fn attach(&self, handles: Arc<[ProjectRescanHandle]>) -> io::Result<()> {
        self.state
            .lock()
            .map_err(|_| io::Error::other("index observations poisoned"))?
            .handles = handles.to_vec();
        Ok(())
    }
    pub fn observe(
        &self,
        key: &SupervisionKey,
        phase: &str,
        state: &str,
        generation: Option<u64>,
        error: Option<String>,
    ) {
        if let Ok(mut data) = self.state.lock() {
            let value = data
                .observations
                .entry(key.clone())
                .or_default()
                .phases
                .entry(phase.to_owned())
                .or_default();
            value.state = state.to_owned();
            value.observed_at = now();
            if state == "completed" {
                value.last_completed_at = value.observed_at;
            }
            value.last_error = error;
            value.generation = generation;
        }
        self.changed.notify_waiters();
    }
    pub fn snapshot(&self, key: &SupervisionKey) -> io::Result<Observation> {
        Ok(self
            .state
            .lock()
            .map_err(|_| io::Error::other("index observations poisoned"))?
            .observations
            .get(key)
            .cloned()
            .unwrap_or_default())
    }
    pub fn available(&self, key: &SupervisionKey) -> bool {
        self.state.lock().is_ok_and(|state| {
            state
                .handles
                .iter()
                .any(|handle| handle.key() == key && handle.is_available())
        })
    }
    pub fn request(&self, key: &SupervisionKey) -> io::Result<Option<u64>> {
        let mut data = self
            .state
            .lock()
            .map_err(|_| io::Error::other("index observations poisoned"))?;
        let Some(handle) = data
            .handles
            .iter()
            .find(|handle| handle.key() == key)
            .cloned()
        else {
            return Ok(None);
        };
        // The existing Notify coalesces requests. Sequence is observation correlation,
        // not a new job, execution owner, queue or completion guarantee.
        if !handle.request_rescan() {
            return Ok(None);
        }
        let value = data.observations.entry(key.clone()).or_default();
        value.request_sequence = value
            .request_sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("index request sequence overflow"))?;
        self.changed.notify_waiters();
        Ok(Some(value.request_sequence))
    }
    pub fn batch(self: &Arc<Self>, key: &SupervisionKey, rescan: bool) -> BatchObservation {
        let ticket = self.begin_batch(key, rescan);
        BatchObservation {
            control: Arc::clone(self),
            key: key.clone(),
            ticket,
            finished: false,
        }
    }
    pub fn begin_batch(&self, key: &SupervisionKey, rescan: bool) -> u64 {
        let ticket = if let Ok(mut data) = self.state.lock() {
            let value = data.observations.entry(key.clone()).or_default();
            value.running = true;
            if rescan { value.request_sequence } else { 0 }
        } else {
            0
        };
        self.changed.notify_waiters();
        ticket
    }
    #[cfg(all(test, target_os = "linux"))]
    pub fn end_batch(&self, key: &SupervisionKey, ticket: u64) {
        if let Ok(mut data) = self.state.lock() {
            let value = data.observations.entry(key.clone()).or_default();
            value.running = false;
            value.completed_request_sequence = value.completed_request_sequence.max(ticket);
        }
        self.changed.notify_waiters();
    }
}

#[derive(Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IndexModule {
    Ast,
    Knowledge,
}
impl IndexModule {
    fn name(self) -> &'static str {
        match self {
            Self::Ast => "ast",
            Self::Knowledge => "knowledge",
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Query {
    #[serde(default)]
    pub wait_ms: u64,
    pub module: Option<IndexModule>,
}
#[allow(clippy::too_many_arguments)] // Original caller, scope and producer policy remain explicit.
pub(crate) async fn query<'a>(
    control: &Arc<IndexControl>,
    key: &SupervisionKey,
    run: bool,
    input: Query,
    issuer: &'a graphit_auth::RoleIssuer,
    caller: &'a graphit_auth::AuthenticatedCaller<'a>,
    original: &'a graphit_hub::policy::RequestContext<'a>,
    scope: graphit_auth::ScopeInput<'a>,
    policy: &crate::task_server::TaskPolicy,
) -> Result<serde_json::Value, serde_json::Value> {
    use graphit_auth::{Obligation, Permission, ProjectReadModule, ProjectRecordAuthority};
    use serde_json::json;
    let error = |code| json!({"code":code});
    if input.wait_ms > policy.timeout.as_millis().min(u128::from(u64::MAX)) as u64
        || (!run && input.wait_ms != 0)
    {
        return Err(error("invalid_arguments"));
    }
    if run
        && !matches!(
            caller.authorize(Permission::Configure),
            Ok(Obligation::Unconditional)
        )
    {
        return Err(error("authentication_or_permission_denied"));
    }
    let authority = ProjectRecordAuthority::new(issuer);
    let mut admitted = Vec::new();
    for (name, module) in [
        ("ast", ProjectReadModule::Ast),
        ("knowledge", ProjectReadModule::Knowledge),
    ] {
        if input.module.is_some_and(|selected| selected.name() != name) {
            continue;
        }
        if let Ok(admission) = authority.admit_authenticated_read(caller, original, scope, module) {
            admitted.push((name, admission));
        }
    }
    if admitted.is_empty() {
        return Err(error("authentication_or_permission_denied"));
    }
    let accepted = if run {
        Some(
            control
                .request(key)
                .map_err(|_| error("index_control_unavailable"))?
                .ok_or_else(|| error("supervisor_unavailable"))?,
        )
    } else {
        None
    };
    let timed_out = if let Some(ticket) = accepted.filter(|_| input.wait_ms > 0) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(input.wait_ms);
        loop {
            let notified = control.changed.notified();
            if control
                .snapshot(key)
                .map_err(|_| error("index_control_unavailable"))?
                .completed_request_sequence
                >= ticket
            {
                break false;
            }
            tokio::select! {()=notified=>{},()=original.cancellation().cancelled()=>return Err(error("request_cancelled_or_expired")),()=tokio::time::sleep_until(deadline.into())=>break true,}
        }
    } else {
        false
    };
    let mut observed = control
        .snapshot(key)
        .map_err(|_| error("index_control_unavailable"))?;
    observed.phases.retain(|phase, _| {
        admitted
            .iter()
            .any(|(module, _)| phase.starts_with(&format!("{module}.")))
    });
    let mut corpora = serde_json::Map::new();
    for (name, admission) in &admitted {
        let absent = if *name == "knowledge" {
            let read = graphit_data::knowledge_index::read_native_knowledge_heads(
                original.snapshot(),
                admission,
                graphit_data::knowledge_index::KnowledgeHeadReadLimits {
                    max_slug_bytes: std::num::NonZeroUsize::new(policy.content.max_scalar_bytes)
                        .ok_or_else(|| error("invalid_policy"))?,
                    batch_rows: policy.read.batch_rows,
                },
            )
            .await
            .map_err(|_| error("corpus_observation_failed"))?;
            match read {
                graphit_data::knowledge_index::KnowledgeHeadRead::NoCorpus => {
                    corpora.insert((*name).to_owned(), json!({"state":"no_corpus"}));
                    true
                }
                graphit_data::knowledge_index::KnowledgeHeadRead::Published {
                    generation,
                    heads,
                } => {
                    corpora.insert((*name).to_owned(),json!({"state":if heads.is_empty(){"published_empty"}else{"published"},"generation":generation}));
                    false
                }
            }
        } else {
            corpora.insert((*name).to_owned(), json!({"state":"unobserved"}));
            false
        };
        for phase in if *name == "ast" {
            &["ast.source_graph", "ast.lexical", "ast.embeddings"][..]
        } else {
            &["knowledge.documents_lexical", "knowledge.embeddings"][..]
        } {
            observed
                .phases
                .entry((*phase).to_owned())
                .or_insert_with(|| Phase {
                    state: if absent { "no_corpus" } else { "unobserved" }.to_owned(),
                    ..Phase::default()
                });
        }
        admission
            .recheck()
            .map_err(|_| error("authentication_or_permission_denied"))?;
    }
    original
        .check()
        .map_err(|_| error("request_cancelled_or_expired"))?;
    let processed = accepted.is_some_and(|ticket| observed.completed_request_sequence >= ticket);
    let failed = observed.last_batch_error.is_some()
        || observed
            .phases
            .values()
            .any(|phase| phase.state == "failed");
    Ok(
        json!({"project_id":key.id,"project_version":key.version,"accepted":accepted.is_some(),"request_sequence":accepted,"processed":processed,"timed_out":timed_out,"outcome":if !processed{"observed"}else if failed{"failed"}else{"processed"},"supervisor_available":control.available(key),"requested_module":input.module,"scheduled_scope":"project","path_requests_supported":false,"state":if observed.running{"running"}else if observed.request_sequence>observed.completed_request_sequence{"pending"}else if failed{"failed"}else if observed.phases.values().all(|phase|phase.state=="no_corpus"){"no_corpus"}else if observed.phases.values().any(|phase|phase.state=="completed"){"completed"}else{"unobserved"},"corpora":corpora,"observation":observed}),
    )
}
