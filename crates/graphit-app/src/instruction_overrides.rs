//! The last successful sync is a local copy of the active Broker's instruction layer.
use crate::source_server::SourcePolicy;
use graphit_auth::{AuthenticatedCaller, RoleIssuer};
use graphit_core::{
    file_lock::FileLock,
    local_files::{atomic_replace, read_regular},
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::policy::RequestContext;
use graphit_runtime::instructions::{CapturedInstruction, InstructionOrigin};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io, path::Path};

const MAX_BYTES: usize = 512 * 1024;

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Templates {
    version: u32,
    templates: BTreeMap<String, String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cached {
    origin: String,
    templates: Templates,
}

pub(crate) async fn sync(
    issuer: &RoleIssuer,
    caller: &AuthenticatedCaller<'_>,
    installation: &InstallationPaths,
    allow_loopback_http: bool,
    original: &RequestContext<'_>,
) -> io::Result<serde_json::Value> {
    let Some(origin) = issuer.instruction_broker_origin(original)? else {
        return Ok(serde_json::json!({"status":"local_provider"}));
    };
    let _lock =
        FileLock::try_acquire(installation.global_path(&["broker-agent-instructions.lock"])?)?;
    let response = caller
        .fetch_broker_instructions(&origin, MAX_BYTES, allow_loopback_http)
        .await?;
    let (status, templates) = match response {
        Some(bytes) => {
            let templates: Templates = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if templates.version != 1
                || templates.templates.iter().any(|(name, body)| {
                    !graphit_runtime::instructions::valid_instruction_name(name)
                        || body.contains('\0')
                })
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid Broker instruction bundle",
                ));
            }
            ("reconciled", templates)
        }
        None => (
            "not_supported",
            Templates {
                version: 1,
                ..Templates::default()
            },
        ),
    };
    original.check()?;
    let count = templates.templates.len();
    let bytes = serde_json::to_vec(&Cached { origin, templates }).map_err(io::Error::other)?;
    let path = installation.global_path(&["broker-agent-instructions.json"])?;
    if !read_regular(&path, MAX_BYTES * 2)?.is_some_and(|(current, _)| *current == bytes) {
        atomic_replace(&path, &bytes, None, original.cancellation())?;
    }
    Ok(serde_json::json!({"status":status,"templates":count}))
}

#[derive(Default)]
pub(crate) struct BrokerInstructions(BTreeMap<String, String>);

impl BrokerInstructions {
    pub(crate) fn load(
        issuer: &RoleIssuer,
        global_root: &Path,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        let Some(origin) = issuer.instruction_broker_origin(original)? else {
            return Ok(Self::default());
        };
        let path = global_root.join("broker-agent-instructions.json");
        let Some((bytes, _)) = read_regular(&path, MAX_BYTES * 2)? else {
            return Ok(Self::default());
        };
        let cache: Cached = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        original.check()?;
        if cache.templates.templates.is_empty() || origin != cache.origin {
            return Ok(Self::default());
        }
        Ok(Self(cache.templates.templates))
    }

    pub(crate) fn candidates<'a>(
        &'a self,
        defaults: &'a [CapturedInstruction<'a>],
    ) -> impl Iterator<Item = CapturedInstruction<'a>> {
        defaults.iter().filter_map(|template| {
            self.0
                .get(template.name)
                .map(|markdown| CapturedInstruction {
                    name: template.name,
                    origin: InstructionOrigin::Broker,
                    markdown,
                })
        })
    }
}

/// Reuse the ordinary local instruction locations for policy and Coordinator templates.
pub(crate) fn local_candidates(
    defaults: &[CapturedInstruction<'_>],
    global_root: &Path,
    index: NativeDocumentFileLimits,
    project: Option<&ProjectPaths>,
    original: &RequestContext<'_>,
) -> io::Result<Vec<crate::managed_roles::TemplateObservation>> {
    let mut observations = Vec::new();
    let roots = project
        .map(|paths| {
            (
                paths.root(),
                Path::new(paths.brand().directory()).join("rules"),
                InstructionOrigin::Project,
            )
        })
        .into_iter()
        .chain(std::iter::once((
            global_root,
            "rules".into(),
            InstructionOrigin::Global,
        )));
    for (root, prefix, origin) in roots {
        original.check()?;
        if !graphit_core::local_files::directory_exists(&root.join(&prefix))? {
            continue;
        }
        for template in defaults {
            observations.push(crate::managed_roles::capture_template(
                root,
                &prefix.join(template.name),
                origin,
                index,
                original,
            )?);
        }
    }
    Ok(observations)
}

pub(crate) struct InstructionSources<'a> {
    broker: BrokerInstructions,
    global_root: &'a Path,
    index: NativeDocumentFileLimits,
    project: Option<&'a ProjectPaths>,
}

impl<'a> InstructionSources<'a> {
    pub(crate) fn capture(
        issuer: &RoleIssuer,
        policy: &'a SourcePolicy,
        project: Option<&'a ProjectPaths>,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        Self::capture_at(
            issuer,
            policy.installation.root(),
            policy.index,
            project,
            original,
        )
    }

    pub(crate) fn capture_project(
        issuer: &RoleIssuer,
        project: &'a ProjectPaths,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        let content = original.content_limits();
        Self::capture_at(
            issuer,
            project.global_root(),
            NativeDocumentFileLimits::new(
                content.max_input_bytes,
                graphit_core::native_path::MAX_PATH_BYTES,
                128,
                content.max_scalar_bytes,
            )?,
            Some(project),
            original,
        )
    }

    pub(crate) fn capture_at(
        issuer: &RoleIssuer,
        global_root: &'a Path,
        index: NativeDocumentFileLimits,
        project: Option<&'a ProjectPaths>,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        Ok(Self {
            broker: BrokerInstructions::load(issuer, global_root, original)?,
            global_root,
            index,
            project,
        })
    }

    pub(crate) fn render(
        &self,
        entry: &str,
        defaults: &[CapturedInstruction<'_>],
        variables: &[graphit_runtime::instructions::InstructionVariable<'_>],
        limits: graphit_runtime::instructions::InstructionLimits,
        original: &RequestContext<'_>,
    ) -> Result<
        graphit_runtime::instructions::RenderedInstruction,
        graphit_runtime::instructions::InstructionError,
    > {
        use graphit_runtime::instructions::{InstructionError, render_instruction};
        let local = local_candidates(
            defaults,
            self.global_root,
            self.index,
            self.project,
            original,
        )
        .map_err(InstructionError::Request)?;
        let candidates = defaults
            .iter()
            .map(|template| CapturedInstruction {
                name: template.name,
                origin: template.origin,
                markdown: template.markdown,
            })
            .chain(self.broker.candidates(defaults))
            .chain(local.iter().filter_map(|item| {
                Some(CapturedInstruction {
                    name: &item.name,
                    origin: item.origin,
                    markdown: item.body.as_deref()?,
                })
            }))
            .collect::<Vec<_>>();
        let value = render_instruction(entry, &candidates, variables, limits, original.snapshot())?;
        for observation in local {
            observation
                .revalidate(original)
                .map_err(InstructionError::Request)?;
        }
        Ok(value)
    }
}
