//! A managed-role run on the active daemon; only its child receives the bearer.
mod context;
mod native;
use crate::managed_roles::{
    self, CompiledDefinition, InstallRequest, InstallResources, ManagedRoleName,
};
use graphit_ai::cli_provider::CliCompletionProvider;
use graphit_auth::{
    AdminSessionHandle, ExternalEffect, Permission, ProjectCapture, RunHandle, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{ArtifactStore, CapturedPublicationScope},
    file_lock::Cancellation,
};
use graphit_hub::{NativeBinding, native::role_publication, policy::RequestContext};
use serde::{Deserialize, Serialize};
use std::{io, path::Path, time::Instant};
use zeroize::Zeroizing;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub install: InstallRequest,
    pub role: ManagedRoleName,
    pub model: Option<String>,
    pub prompt: String,
}
#[derive(Default, Deserialize, Serialize)]
pub struct RunOutcome {
    pub answer: Option<String>,
    pub run_closed: bool,
    pub failure: Option<String>,
}
/// SDK causes and residual handles stay private to the service owner, never on the wire.
pub struct RunExecution {
    pub outcome: RunOutcome,
    pub cause: Option<io::Error>,
    pub native: Option<Box<dyn std::error::Error + Send + Sync>>,
    pub residual: Option<RunResidual>,
}
pub struct RunResidual {
    pub session: AdminSessionHandle,
    pub run: RunHandle,
    native: Option<native::NativeCleanup>,
}
/// The retained Runtime caller authorizes the native effect; Role tools get only their own run.
pub async fn execute_run(
    resources: InstallResources<'_>,
    credential: &str,
    input: &RunRequest,
    original: &RequestContext<'_>,
) -> RunExecution {
    let binding = input.install.binding.unwrap_or(resources.policy.binding);
    let provider = match binding {
        NativeBinding::Codex => CliCompletionProvider::Codex,
        NativeBinding::Claude => CliCompletionProvider::Claude,
        NativeBinding::Cursor => CliCompletionProvider::Cursor,
        NativeBinding::Qwen => CliCompletionProvider::Qwen,
        NativeBinding::Antigravity => CliCompletionProvider::Antigravity,
        NativeBinding::KiroV3 => CliCompletionProvider::KiroV3,
        NativeBinding::OpenCode => CliCompletionProvider::OpenCode,
        NativeBinding::Kimi => CliCompletionProvider::Kimi,
    };
    execute_with_program(
        resources,
        credential,
        input,
        original,
        Path::new(provider.executable()),
    )
    .await
}
pub(crate) async fn execute_with_program(
    resources: InstallResources<'_>,
    credential: &str,
    input: &RunRequest,
    original: &RequestContext<'_>,
    program: &Path,
) -> RunExecution {
    let mut execution = RunExecution {
        outcome: RunOutcome::default(),
        cause: None,
        native: None,
        residual: None,
    };
    let issuer = resources.auth.issuer();
    let binding = input.install.binding.unwrap_or(resources.policy.binding);
    let setup = (|| -> io::Result<_> {
        let deadline = original.snapshot().required_deadline()?;
        let content = resources.policy.content;
        if resources.modules.len() != resources.projects.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Role project configuration alignment invalid",
            ));
        }
        if input.install.destination != managed_roles::InstallDestination::Project
            || input.prompt.is_empty()
            || input.prompt.len() > content.max_input_bytes
            || !input.install.skill_bundles.is_empty()
            || input
                .model
                .as_ref()
                .is_some_and(|model| model.is_empty() || model.len() > content.max_scalar_bytes)
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let caller = issuer
            .authenticate_runtime(credential, original)
            .map_err(io::Error::other)?;
        let session = issuer.bootstrap_admin(&caller).map_err(io::Error::other)?;
        let parent = issuer
            .admin_parent(&caller, &session)
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::External(ExternalEffect::NativeLaunch))
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::Configure)
            .map_err(io::Error::other)?;
        parent
            .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
            .map_err(io::Error::other)?;
        let (paths, id, version) =
            managed_roles::select_project(resources.projects, resources.current, &input.install)?;
        let scope = CapturedPublicationScope::project(
            paths,
            *id,
            resources.policy.scope,
            original.snapshot(),
        )?;
        let record = scope.binding()?;
        let mut captured_install = input.install.clone();
        captured_install.project_version = Some(version.version().to_owned());
        let CompiledDefinition {
            name,
            instructions,
            native,
            definition,
            observed,
            ..
        } = managed_roles::compile_definition(
            Some(paths),
            input.role,
            &captured_install,
            resources.policy,
            &parent,
            original,
        )?;
        let reference = issuer
            .configure_tool_owned_definition(&parent, definition)
            .map_err(io::Error::other)?;
        let hub = &resources.policy.native_install.hub;
        let store = ArtifactStore::new(scope.clone(), hub.artifacts());
        let profile = paths.root().join(&native.relative_entry);
        role_publication::publish_role(
            &store,
            role_publication::RoleDestination::Project(paths.root()),
            native,
            &reference,
            resources.policy.index,
            original,
            || {
                parent
                    .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
                    .map_err(io::Error::other)?;
                for observation in &observed {
                    observation.revalidate(original)?;
                }
                original.snapshot().check()
            },
        )
        .map_err(io::Error::other)?;
        let scope_input = ScopeInput {
            installation: resources.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                version,
            }),
            providers: &[],
            binding,
            global_version: "",
        };
        let encoded = Zeroizing::new(
            reference
                .encode(content.max_input_bytes, original)
                .map_err(io::Error::other)?,
        );
        let reference_text = std::str::from_utf8(&encoded)
            .map_err(io::Error::other)?
            .to_owned();
        let selection = serde_json::to_string(&serde_json::json!({
            "project_dir": graphit_core::native_path::NativePath::new(paths.root().to_owned())?,
            "project_id": id,
            "project_version": version.version(),
        }))?;
        let global_key = paths.brand().environment_variable("GLOBAL_DIR")?;
        let global_root = resources
            .policy
            .installation
            .root()
            .to_str()
            .ok_or(io::ErrorKind::InvalidInput)?;
        let executable = std::env::current_exe()?;
        let executable_directory = executable.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let inherited_path = std::env::var_os("PATH");
        let native_path = std::env::join_paths(
            std::iter::once(executable_directory.to_owned())
                .chain(inherited_path.iter().flat_map(std::env::split_paths)),
        )
        .map_err(|cause| io::Error::new(io::ErrorKind::InvalidInput, cause))?;

        let run = issuer
            .start_run(
                &parent,
                &scope_input,
                deadline.saturating_duration_since(std::time::Instant::now()),
            )
            .map_err(io::Error::other)?;
        let bearer = match issuer.issue(
            &parent,
            &run,
            &reference,
            &managed_roles::permissions(input.role.role()),
            &scope_input,
        ) {
            Ok(bearer) => bearer,
            Err(error) => {
                execution.residual = Some(RunResidual {
                    session,
                    run,
                    native: None,
                });
                return Err(io::Error::other(error));
            }
        };
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| {
                ![
                    "GRAPHIT_ROLE_BEARER",
                    "GRAPHIT_ROLE_REFERENCE",
                    "GRAPHIT_ROLE_SELECTION",
                    "GRAPHIT_ROLE_BINDING",
                ]
                .iter()
                .any(|private| key.as_os_str() == std::ffi::OsStr::new(private))
                    && key.as_os_str() != std::ffi::OsStr::new(global_key.as_str())
                    && key.as_os_str() != std::ffi::OsStr::new("PATH")
            })
            .collect();
        environment.extend([
            (
                "GRAPHIT_ROLE_BEARER".into(),
                bearer.expose_for_transport().into(),
            ),
            (
                "GRAPHIT_ROLE_REFERENCE".into(),
                reference_text.as_str().into(),
            ),
            ("GRAPHIT_ROLE_SELECTION".into(), selection.into()),
            ("GRAPHIT_ROLE_BINDING".into(), binding.as_str().into()),
            (global_key.into(), global_root.into()),
            ("PATH".into(), native_path),
        ]);
        Ok((
            session,
            run,
            environment,
            name,
            instructions,
            profile,
            bearer,
            Zeroizing::new(reference_text),
            observed,
            reference,
            scope,
            record,
            version,
        ))
    })();
    match setup {
        Ok((
            session,
            run,
            environment,
            name,
            instructions,
            profile,
            bearer,
            reference,
            observations,
            role_reference,
            scope,
            record,
            version,
        )) => {
            let content = resources.policy.content;
            let mut native_cleanup = None;
            let result = async {
                let project = managed_roles::select_project(
                    resources.projects,
                    resources.current,
                    &input.install,
                )?;
                let selected = resources
                    .projects
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, project))
                    .ok_or(io::ErrorKind::InvalidInput)?;
                let grant = issuer
                    .verify(
                        bearer.expose_for_transport(),
                        &role_reference,
                        ScopeInput {
                            installation: resources.auth.installation(),
                            project: Some(ProjectCapture {
                                scope: ScopeCapture {
                                    live: &scope,
                                    record: &record,
                                },
                                version,
                            }),
                            providers: &[],
                            binding,
                            global_version: "",
                        },
                        original,
                    )
                    .map_err(io::Error::other)?;
                let prompt = context::prompt(
                    &resources,
                    selected,
                    input,
                    original,
                    role_reference.role(),
                    &grant,
                )
                .await?;
                issuer
                    .verify(
                        bearer.expose_for_transport(),
                        &role_reference,
                        ScopeInput {
                            installation: resources.auth.installation(),
                            project: Some(ProjectCapture {
                                scope: ScopeCapture {
                                    live: &scope,
                                    record: &record,
                                },
                                version,
                            }),
                            providers: &[],
                            binding,
                            global_version: "",
                        },
                        original,
                    )
                    .map_err(io::Error::other)?;
                for observation in &observations {
                    observation.revalidate(original)?;
                }
                original.snapshot().check()?;
                let recheck = || {
                    issuer
                        .verify(
                            bearer.expose_for_transport(),
                            &role_reference,
                            ScopeInput {
                                installation: resources.auth.installation(),
                                project: Some(ProjectCapture {
                                    scope: ScopeCapture {
                                        live: &scope,
                                        record: &record,
                                    },
                                    version,
                                }),
                                providers: &[],
                                binding,
                                global_version: "",
                            },
                            original,
                        )
                        .map(|_| ())
                        .map_err(io::Error::other)
                };
                native::run(
                    original.snapshot(),
                    native::NativeInput {
                        binding,
                        program,
                        cwd: project.0.root(),
                        profile: &profile,
                        environment,
                        name: &name,
                        instructions: &instructions,
                        allows_shell: input.role.role().allows_shell(),
                        model: input.model.as_deref(),
                        prompt: prompt.into_owned(),
                    },
                    content,
                    &recheck,
                )
                .await
            }
            .await;
            match result {
                Ok(native) => {
                    native_cleanup = native.cleanup;
                    execution.native = native.failure;
                    if execution.native.is_some() {
                        execution.outcome.failure = Some("native Role run failed".into());
                    }
                    let checked = issuer.verify(
                        bearer.expose_for_transport(),
                        &role_reference,
                        ScopeInput {
                            installation: resources.auth.installation(),
                            project: Some(ProjectCapture {
                                scope: ScopeCapture {
                                    live: &scope,
                                    record: &record,
                                },
                                version,
                            }),
                            providers: &[],
                            binding,
                            global_version: "",
                        },
                        original,
                    );
                    if let Err(error) = checked {
                        execution
                            .cause
                            .get_or_insert_with(|| io::Error::other(error));
                    }
                    execution.outcome.answer = native.answer.map(|answer| {
                        answer
                            .replace(bearer.expose_for_transport(), "[redacted]")
                            .replace(reference.as_str(), "[redacted]")
                    });
                    for observation in &observations {
                        if let Err(error) = observation.revalidate(original) {
                            execution.cause.get_or_insert(error);
                            break;
                        }
                    }
                }
                Err(error) => execution.cause = Some(error),
            }
            execution.residual = Some(RunResidual {
                session,
                run,
                native: native_cleanup,
            });
        }
        Err(error) => {
            execution.cause = Some(error);
        }
    }
    if let Some(mut residual) = execution.residual.take() {
        let (closed, cause) = residual
            .close(
                resources.auth,
                credential,
                resources.policy.content,
                resources.policy.timeout,
            )
            .await;
        if let Some(cause) = cause {
            execution.cause.get_or_insert(cause);
        }
        execution.outcome.run_closed = closed;
        if !closed {
            execution.residual = Some(residual);
        }
    }
    if let Some(cause) = &execution.cause {
        execution.outcome.answer = None;
        execution.outcome.failure =
            Some(format!("managed role operation failed: {:?}", cause.kind()));
    }
    execution
}

impl RunResidual {
    pub(crate) async fn close(
        &mut self,
        auth: &graphit_daemon::auth_startup::DaemonAuth,
        credential: &str,
        content: graphit_hub::native::ContentLimits,
        timeout: std::time::Duration,
    ) -> (bool, Option<io::Error>) {
        let Some(deadline) = Instant::now().checked_add(timeout) else {
            return (false, Some(io::ErrorKind::InvalidInput.into()));
        };
        let mut first = None;
        if let Some(native) = self.native.as_mut() {
            let (joined, cause) = native.close(deadline).await;
            first = cause;
            if !joined {
                return (false, first);
            }
            self.native = None;
        }
        let cancel = Cancellation::default();
        let closed = (|| -> io::Result<()> {
            let cleanup =
                RequestContext::new(content, &cancel, deadline).map_err(io::Error::other)?;
            let caller = auth
                .issuer()
                .authenticate_runtime(credential, &cleanup)
                .map_err(io::Error::other)?;
            let parent = auth
                .issuer()
                .admin_parent(&caller, &self.session)
                .map_err(io::Error::other)?;
            auth.issuer()
                .close_run(&parent, &self.run)
                .map_err(io::Error::other)
        })();
        let known = closed.is_ok();
        if let Err(cause) = closed {
            first.get_or_insert(cause);
        }
        (known, first)
    }
}
