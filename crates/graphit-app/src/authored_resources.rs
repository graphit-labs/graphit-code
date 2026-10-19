//! Authored Resources enumeration and captured reading shared by admitted ingresses.
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyPolicy, SourceReadLimits,
    },
    project_identity::ProjectId,
    project_paths::ProjectPaths,
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, ResourcesQuery, SourceRef,
    policy::{HubResourcePolicy, RequestContext},
};
use serde_json::json;
use std::{
    io::{self, Read},
    num::NonZeroUsize,
    path::Path,
};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub mod client;
pub(crate) mod inputs;
#[cfg(test)]
pub(crate) mod pause;
use inputs::Query;

pub(crate) const LIST: &str = "graphit_hub_authored_list";
pub(crate) const REVIEW: &str = "graphit_hub_authored_review";
pub(crate) const CONTENT: &str = "graphit_hub_authored_content";

#[derive(Debug)]
pub(crate) enum ReadFailure {
    SourceMissing,
    FileMissing,
    NotText,
}
impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SourceMissing => "authored source missing",
            Self::FileMissing => "authored file missing",
            Self::NotText => "authored content is not text",
        })
    }
}
impl std::error::Error for ReadFailure {}
fn missing(error: io::Error, source: bool) -> io::Error {
    if error.kind() == io::ErrorKind::NotFound {
        io::Error::new(
            error.kind(),
            if source {
                ReadFailure::SourceMissing
            } else {
                ReadFailure::FileMissing
            },
        )
    } else {
        error
    }
}
fn projection(error: graphit_hub::ProjectionError) -> io::Error {
    io::Error::new(error.kind(), error)
}

/// The ingress retains PROJECT authority and validates the query before this read.
pub(crate) fn read(
    (paths, id): (&ProjectPaths, ProjectId),
    scope: &CapturedPublicationScope,
    query: Query,
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<Vec<u8>> {
    let result = match query {
        Query::List(input) => {
            let id = input
                .id
                .as_ref()
                .map(|value| ArtifactId::new(value, policy.projection()).map_err(projection))
                .transpose()?;
            let version = input
                .version
                .as_ref()
                .map(|value| ConcreteVersion::new(value, policy.projection()).map_err(projection))
                .transpose()?;
            let after = input.after(policy)?;
            let page = graphit_hub::list_resources(
                paths,
                ResourcesQuery {
                    artifact_type: input.artifact_type,
                    id: id.as_ref(),
                    version: version.as_ref(),
                    after: after.as_ref(),
                    limit: NonZeroUsize::new(input.limit).ok_or(io::ErrorKind::InvalidInput)?,
                },
                policy.projection(),
                &mut *before,
            )
            .map_err(projection)?;
            let next_cursor = page
                .next_after
                .map(|after| input.next_cursor(after, policy))
                .transpose()?;
            let entries: Vec<_> = page.entries.into_iter().map(|item| json!({"artifact_type":item.artifact_type,"id":item.id,"version":item.version,"name":null})).collect();
            json!({"status":"ok","project_id":input.project_id,"project_version":input.project_version,"entries":entries,"next_cursor":next_cursor})
        }
        Query::Review(input) => {
            let reader = capture(
                paths,
                id,
                scope,
                (input.artifact_type, &input.id, &input.version),
                policy,
                original,
            )?;
            before()?;
            let mut entries = Vec::with_capacity(input.limit);
            let mut observed = 0usize;
            let mut more = false;
            for node in reader.nodes() {
                before()?;
                if node.relative().as_os_str().is_empty() {
                    continue;
                }
                if observed < input.offset {
                    observed += 1;
                    continue;
                }
                if entries.len() == input.limit {
                    more = true;
                    break;
                }
                let path = wire_path(node.relative(), policy.content().max_scalar_bytes)?;
                entries.push(json!({"path":path,"kind":match node.kind() { graphit_fs::NodeKind::File => "File", graphit_fs::NodeKind::Directory => "Directory" },"bytes":node.bytes()}));
            }
            reader.revalidate_bindings()?;
            before()?;
            let next_offset = more.then(|| input.offset + entries.len());
            json!({"status":"ok","project_id":input.project_id,"project_version":input.project_version,"artifact_type":input.artifact_type,"id":input.id,"version":input.version,"source":"Resources","metadata":null,"entries":entries,"next_offset":next_offset})
        }
        Query::Content(input) => {
            let reader = capture(
                paths,
                id,
                scope,
                (input.artifact_type, &input.id, &input.version),
                policy,
                original,
            )?;
            before()?;
            let file = reader
                .open_file(Path::new(&input.path))
                .map_err(|e| missing(e, false))?;
            if file.len() > policy.content().max_scalar_bytes as u64 {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            let mut bytes = Vec::with_capacity(
                usize::try_from(file.len()).map_err(|_| io::ErrorKind::FileTooLarge)?,
            );
            file.stream(0)?.read_to_end(&mut bytes)?;
            reader.validate_file_bytes(Path::new(&input.path), &bytes, original.snapshot())?;
            before()?;
            let content = String::from_utf8(bytes)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, ReadFailure::NotText))?;
            if content.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    ReadFailure::NotText,
                ));
            }
            json!({"status":"ok","project_id":input.project_id,"project_version":input.project_version,"artifact_type":input.artifact_type,"id":input.id,"version":input.version,"source":"Resources","content":{"path":input.path,"content":content,"bytes":file.len()}})
        }
    };
    before()?;
    let bytes = serde_json::to_vec(&result).map_err(io::Error::other)?;
    if bytes.len() > policy.content().max_output_bytes {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    before()?;
    Ok(bytes)
}

fn capture(
    paths: &ProjectPaths,
    producer: graphit_core::project_identity::ProjectId,
    scope: &CapturedPublicationScope,
    identity: (ArtifactType, &str, &str),
    policy: &HubResourcePolicy,
    original: &RequestContext<'_>,
) -> io::Result<CapturedSourceReader> {
    let source = SourceRef::resources(
        paths,
        producer,
        identity.0,
        ArtifactId::new(identity.1, policy.projection()).map_err(projection)?,
        ConcreteVersion::new(identity.2, policy.projection()).map_err(projection)?,
        policy.projection(),
    )
    .map_err(projection)?;
    let relative = source
        .directory()
        .as_path()
        .strip_prefix(paths.root())
        .map_err(io::Error::other)?;
    let reader = CapturedSourceReader::capture_project(
        scope,
        relative,
        graphit_fs::NodeKind::Directory,
        &policy.snapshot_policy(CopyPolicy::RejectFileLinks, [])?,
        SourceReadLimits::new(
            64 * 1024,
            64 * 1024,
            policy.content().max_scalar_bytes as u64,
        )?,
        original.snapshot(),
    )
    .map_err(|e| missing(e, true))?;
    #[cfg(test)]
    pause::observed(producer, original)?;
    Ok(reader)
}
fn wire_path(path: &Path, maximum: usize) -> io::Result<String> {
    let mut result = String::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        let value = part.to_str().ok_or(io::ErrorKind::InvalidData)?;
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(value);
        if result.len() > maximum {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
    }
    Ok(result)
}
