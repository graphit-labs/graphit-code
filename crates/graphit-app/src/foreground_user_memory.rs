//! Readonly Memory of the actual Web principal under the trusted installation.
use crate::{
    foreground_memory::{self, Filter, Query, Record},
    task_server::{
        TaskPolicy,
        browser::codec,
        request::{Arguments, Selection},
    },
};
use graphit_auth::{
    AuthError, RoleIssuer, ScopeCapture, ServiceUseError, UserReadAdmission,
    broker::{BrokerVerifier, oauth::web::AuthenticatedWebCaller},
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, StoreOwner, UserIdentity},
    file_io::SnapshotContext,
    project_paths::InstallationPaths,
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use graphit_records::{
    memory_store::{Memory, MemoryStoreError},
    user_memory_store::{
        NativeUserMemoryStore, UserMemoryGeneration, UserMemoryListRead, UserMemoryOwner,
        UserMemoryScopeId,
    },
};
use serde::{Deserialize, Serialize};
use std::{io, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input<T> {
    pub(crate) input: T,
}

pub(crate) fn parse(
    bytes: &[u8],
    limits: ContentLimits,
    operation: &str,
) -> Result<Arguments, &'static str> {
    let query = match operation {
        "user_memory_list" => {
            let body: Input<Filter> = codec::decode(bytes, limits)?;
            body.input.validate(limits)?;
            Query::List(body.input)
        }
        "user_memory_get" | "user_memory_history" => {
            let body: Input<Record> = codec::decode(bytes, limits)?;
            if limits.max_scalar_bytes < 26 {
                return Err("invalid_arguments");
            }
            if operation == "user_memory_get" {
                Query::Get(body.input)
            } else {
                Query::History(body.input)
            }
        }
        "user_memory_create" | "user_memory_update" | "user_memory_delete" => {
            return foreground_memory::mutation::parse_user(bytes, limits, operation);
        }
        "user_memory_search" => {
            let body: Input<foreground_memory::search::Input> = codec::decode(bytes, limits)?;
            body.input.validate(limits)?;
            Query::Search(body.input)
        }
        _ => return Err("invalid_arguments"),
    };
    Ok(Arguments {
        selection: Selection::default(),
        input: Some(serde_json::to_value(query).map_err(|_| "invalid_arguments")?),
    })
}

#[derive(Debug)]
pub(crate) enum CaptureError {
    Subject(ServiceUseError),
    Auth(AuthError),
    Physical(io::Error),
}
impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subject(error) => std::fmt::Display::fmt(error, f),
            Self::Auth(error) => std::fmt::Display::fmt(error, f),
            Self::Physical(error) => std::fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for CaptureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Subject(error) => error,
            Self::Auth(error) => error,
            Self::Physical(error) => error,
        })
    }
}
impl From<ServiceUseError> for CaptureError {
    fn from(e: ServiceUseError) -> Self {
        Self::Subject(e)
    }
}
impl From<AuthError> for CaptureError {
    fn from(e: AuthError) -> Self {
        Self::Auth(e)
    }
}
impl From<io::Error> for CaptureError {
    fn from(e: io::Error) -> Self {
        Self::Physical(e)
    }
}

pub(crate) fn capture<'a>(
    issuer: &'a RoleIssuer,
    installation: ScopeCapture<'a>,
    caller: &'a AuthenticatedWebCaller<'a>,
    verifier: &'a BrokerVerifier,
    original: &'a RequestContext<'a>,
    policy: &TaskPolicy,
) -> Result<CapturedPublicationScope, CaptureError> {
    original.snapshot().check()?;
    let StoreOwner::Installation { installation_id } = installation.live.owner() else {
        return Err(AuthError::Denied.into());
    };
    let verified = issuer.user_from_web(caller, verifier, original)?;
    let identity = UserIdentity::new(
        verified.issuer()?.to_owned(),
        verified.subject()?.to_owned(),
    )?;
    let paths = InstallationPaths::resolve(
        installation.live.brand().clone(),
        installation.live.owner_root(),
        None,
        Some(installation.live.owner_root()),
    )?;
    Ok(CapturedPublicationScope::user(
        &paths,
        installation_id.clone(),
        identity,
        policy.scope,
        original.snapshot(),
    )?)
}

#[derive(Serialize)]
pub(crate) struct Response<'a, T> {
    pub(crate) scope_id: &'a UserMemoryScopeId,
    #[serde(rename = "currentOwner")]
    pub(crate) current_owner: &'a UserMemoryOwner,
    pub(crate) native: &'a T,
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Catalogue<'a> {
    NoCorpus,
    Published {
        generation: &'a UserMemoryGeneration,
        memories: &'a [Memory],
    },
}
pub(crate) async fn read(
    original: &Arc<SnapshotContext>,
    admission: &UserReadAdmission<'_>,
    policy: &TaskPolicy,
    query: Query,
) -> Result<Vec<u8>, MemoryStoreError> {
    let scope = admission.user()?.live;
    let StoreOwner::User {
        installation_id,
        identity,
    } = scope.owner()
    else {
        return Err(MemoryStoreError::WrongAdmission);
    };
    let owner = UserMemoryOwner {
        installation_id: installation_id.clone(),
        identity: identity.clone(),
    };
    let scope_id = UserMemoryScopeId::from_owner(&owner)?;
    let limits = foreground_memory::limits(policy)?;
    let maximum = policy.content.max_output_bytes;
    let output = match query {
        Query::List(filter) => {
            let read =
                NativeUserMemoryStore::list(original, admission, filter.into_native(), limits)
                    .await?;
            let native = match &read {
                UserMemoryListRead::NoCorpus => Catalogue::NoCorpus,
                UserMemoryListRead::Published {
                    generation,
                    memories,
                } => Catalogue::Published {
                    generation,
                    memories,
                },
            };
            codec::encode_response(
                &Response {
                    scope_id: &scope_id,
                    current_owner: &owner,
                    native: &native,
                },
                maximum,
            )?
        }
        Query::Get(record) => codec::encode_response(
            &Response {
                scope_id: &scope_id,
                current_owner: &owner,
                native: &NativeUserMemoryStore::get(original, admission, record.id, limits).await?,
            },
            maximum,
        )?,
        Query::History(record) => codec::encode_response(
            &Response {
                scope_id: &scope_id,
                current_owner: &owner,
                native: &NativeUserMemoryStore::history(original, admission, record.id, limits)
                    .await?,
            },
            maximum,
        )?,
        Query::Mutation(_) => return Err(MemoryStoreError::WrongAdmission),
        Query::Search(_) => return Err(MemoryStoreError::WrongAdmission),
    };
    admission.recheck()?;
    original.check()?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> ContentLimits {
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 1,
            max_nodes: 1,
            max_scalar_bytes: 32,
            max_collection_entries: 32,
        }
    }
    #[test]
    fn input_only_single_decode_rejects_identity_selection_duplicate_and_types() {
        assert!(parse(br#"{"input":{}}"#, limits(), "user_memory_list").is_ok());
        for body in [
            br#"{"input":{},"issuer":"fake"}"#.as_slice(),
            br#"{"input":{},"scope_id":"fake"}"#,
            br#"{"input":{},"selection":{}}"#,
            br#"{"input":{},"input":{}}"#,
            br#"{"input":{"important_only":null}}"#,
            br#"{"input":{"tag":"a","tag":"b"}}"#,
            br#"{"input":null}"#,
            br#"{"input":{}} trailing"#,
        ] {
            assert!(parse(body, limits(), "user_memory_list").is_err());
        }
        assert!(parse(br#"{"input":{"id":"bad"}}"#, limits(), "user_memory_get").is_err());
        let mut small = limits();
        small.max_input_bytes = 1;
        assert!(parse(br#"{"input":{}}"#, small, "user_memory_list").is_err());
        small = limits();
        small.max_scalar_bytes = 2;
        assert!(
            parse(
                br#"{"input":{"tag":"\u03b2\u03b2"}}"#,
                small,
                "user_memory_list"
            )
            .is_err()
        );
    }
}
