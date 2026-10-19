//! Native ACP transport custody reused by Role executions.
use super::process::{Input, NativeFailure};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use crate::cli_launch_policy::CliLaunchPolicy;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use crate::{
    acp_process::{self, AcpLaunch, AcpProcessOwner},
    knowledge_kiro_process::{KiroProcess, KiroProcessOwner},
};
use agent_client_protocol::schema::v1::{NewSessionRequest, SessionConfigId, SessionConfigValueId};
use graphit_ai::{
    cli_kiro::{
        self, KiroConfigSelection, KiroDeliveryPhase, KiroEngine, KiroLimits, KiroPrompt,
        KiroSessionStart, KiroSink,
    },
    cli_opencode::{
        self, OpencodeLimits, OpencodePhase, OpencodeRequest, OpencodeSessionStart, OpencodeSink,
    },
    cli_opencode_wire::OpencodeUpdate,
};
use graphit_core::file_io::SnapshotContext;
use graphit_hub::{NativeBinding, native::ContentLimits};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use std::time::Instant;
use std::{io, sync::Arc};

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
enum Owner {
    Kiro(Arc<KiroProcessOwner>),
    Opencode(Arc<AcpProcessOwner>),
}
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(in crate::managed_role_run) struct AcpRun(Option<Owner>);
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
impl AcpRun {
    pub async fn close(&mut self, deadline: Instant) -> (bool, Option<io::Error>) {
        let Some(owner) = self.0.as_ref() else {
            return (true, None);
        };
        let (joined, clean) = match owner {
            Owner::Kiro(owner) => {
                owner.stop();
                let joined = owner.observe(deadline).await;
                (joined, !owner.spawned() || owner.clean_joined().await)
            }
            Owner::Opencode(owner) => {
                owner.stop();
                let joined = owner.observe(deadline).await;
                (joined, joined && owner.clean().await)
            }
        };
        if !joined {
            return (false, Some(io::ErrorKind::TimedOut.into()));
        }
        self.0 = None;
        if !clean {
            return (
                true,
                Some(io::Error::other("Role ACP child cleanup failed")),
            );
        }
        (true, None)
    }
}
pub(super) struct AcpResult {
    pub answer: Option<String>,
    pub failure: Option<NativeFailure>,
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub process: AcpRun,
}
struct Sink<'a>(&'a (dyn Fn() -> io::Result<()> + Sync));
impl KiroSink for Sink<'_> {
    async fn push<'a>(
        &'a mut self,
        _: KiroDeliveryPhase,
        _: &'a agent_client_protocol::schema::v1::SessionNotification,
        original: &'a SnapshotContext,
    ) -> io::Result<()> {
        original.check()?;
        (self.0)()
    }
}
impl OpencodeSink for Sink<'_> {
    fn recheck(&self, original: &SnapshotContext) -> io::Result<()> {
        original.check()?;
        (self.0)()
    }
    fn session_bound(&mut self, _: &str, _: bool, original: &SnapshotContext) -> io::Result<()> {
        self.recheck(original)
    }
    async fn push<'a>(
        &'a mut self,
        _: OpencodePhase,
        _: &'a OpencodeUpdate,
        original: &'a SnapshotContext,
    ) -> io::Result<()> {
        self.recheck(original)
    }
}
pub(super) async fn run(
    original: &Arc<SnapshotContext>,
    input: Input<'_>,
    content: ContentLimits,
    recheck: &(dyn Fn() -> io::Result<()> + Sync),
) -> io::Result<AcpResult> {
    recheck()?;
    original.check()?;
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    let policy = CliLaunchPolicy::capture();
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    let permit = Arc::new(tokio::sync::Semaphore::new(1))
        .try_acquire_owned()
        .map_err(io::Error::other)?;
    let mut sink = Sink(recheck);
    match input.binding {
        NativeBinding::KiroV3 => {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            let owner = KiroProcessOwner::new(None);
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            let transport = KiroProcess {
                program: input.program.to_owned(),
                args: vec!["acp".into(), "--agent-engine=v3".into()],
                cwd: input.cwd.to_owned(),
                environment: input.environment,
                original: Arc::clone(original),
                service_stop: original.cancellation().clone(),
                limits: policy.process,
                frame_bytes: policy.wire.frame_bytes,
                permit: Some(permit),
                owner: Arc::clone(&owner),
            };
            let session = NewSessionRequest::new(input.cwd).meta(
                serde_json::json!({"kiro":{"modeId":input.name}})
                    .as_object()
                    .expect("Role mode object")
                    .clone(),
            );
            let prompt = KiroPrompt {
                text: input.prompt,
                selection: input.model.map(|model| KiroConfigSelection {
                    config_id: SessionConfigId::new("model"),
                    value_id: SessionConfigValueId::new(model),
                    meta: None,
                }),
            };
            let result = cli_kiro::run_kiro(
                original,
                transport,
                KiroEngine::V3,
                KiroSessionStart::New(session),
                prompt,
                KiroLimits {
                    prompt_text_bytes: content.max_input_bytes,
                    response_text_bytes: content.max_output_bytes,
                    notifications: content.max_collection_entries as u64,
                    id_bytes: content.max_scalar_bytes,
                },
                &mut sink,
            )
            .await;
            let (answer, failure) = match result {
                Ok(receipt) => (receipt.answer().map(str::to_owned), None),
                Err(cause) => (None, Some(Box::new(cause) as NativeFailure)),
            };
            Ok(AcpResult {
                answer,
                failure,
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                process: AcpRun(Some(Owner::Kiro(owner))),
            })
        }
        NativeBinding::OpenCode => {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            let (transport, owner) = match acp_process::spawn(AcpLaunch {
                program: input.program,
                cwd: input.cwd,
                environment: &input.environment,
                original: Arc::clone(original),
                service_stop: original.cancellation().clone(),
                limits: policy.process,
                wire: policy.wire,
                permit,
            }) {
                Ok(spawned) => spawned,
                Err(failure) => {
                    let Some(owner) = failure.owner else {
                        return Err(failure.error);
                    };
                    return Ok(AcpResult {
                        answer: None,
                        failure: Some(Box::new(failure.error) as NativeFailure),
                        process: AcpRun(Some(Owner::Opencode(owner))),
                    });
                }
            };
            let result = cli_opencode::run_opencode(
                original,
                transport,
                OpencodeRequest {
                    session: OpencodeSessionStart::Create,
                    cwd: input.cwd.to_owned(),
                    model: input.model.map(str::to_owned),
                    agent: Some(input.name.to_owned()),
                    prompt: input.prompt,
                },
                OpencodeLimits {
                    prompt_text_bytes: content.max_input_bytes,
                    response_text_bytes: content.max_output_bytes,
                    notifications: content.max_collection_entries as u64,
                    id_bytes: content.max_scalar_bytes,
                },
                &mut sink,
            )
            .await;
            let (answer, failure) = match result {
                Ok(receipt) => (receipt.eligible_text().map(str::to_owned), None),
                Err(cause) => (None, Some(Box::new(cause) as NativeFailure)),
            };
            Ok(AcpResult {
                answer,
                failure,
                #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                process: AcpRun(Some(Owner::Opencode(owner))),
            })
        }
        _ => Err(io::ErrorKind::Unsupported.into()),
    }
}
