//! Browser search consumes native rankings and generations without a local corpus filter.
use super::{Filter, limits};
use crate::embedding_service::EmbeddingServiceError;
use crate::foreground_memory_jobs::MemoryJobResources;
use crate::task_server::{TaskPolicy, browser::codec};
use graphit_auth::{ProjectReadAdmission, ServicePurpose, UserReadAdmission};
use graphit_core::artifact_store::StoreOwner;
use graphit_daemon::model_service::EmbedInput;
use graphit_hub::native::ContentLimits;
use graphit_hub::policy::RequestContext;
use graphit_records::{memory_store::*, user_memory_store::*};
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    #[default]
    Fts,
    Semantic,
    Hybrid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Input {
    pub(crate) text: String,
    pub(crate) top_k: NonZeroUsize,
    #[serde(default)]
    pub(crate) mode: Mode,
    #[serde(default)]
    pub(crate) filter: Filter,
    #[serde(default)]
    pub(crate) include_deleted_history: bool,
}
impl Input {
    pub(crate) fn validate(&self, limits: ContentLimits) -> Result<(), &'static str> {
        if self.text.trim().is_empty()
            || self.text.contains('\0')
            || self.text.len() > limits.max_scalar_bytes
            || self.top_k.get() > limits.max_collection_entries
        {
            return Err("invalid_arguments");
        }
        self.filter.validate(limits)
    }
    fn lexical(self) -> MemoryLexicalQuery {
        MemoryLexicalQuery {
            text: self.text,
            top_k: self.top_k,
            filter: self.filter.into_native(),
            include_deleted_history: self.include_deleted_history,
        }
    }
}
#[derive(Serialize)]
struct Response<'a, T> {
    mode: Mode,
    model: Option<Model>,
    #[serde(flatten)]
    result: &'a T,
}
#[derive(Serialize)]
struct Unavailable {
    state: &'static str,
    mode: Mode,
    reason: &'static str,
}

#[derive(Debug)]
pub(crate) enum Error {
    Store(MemoryStoreError),
    Operation(std::io::Error),
    Service(graphit_auth::ServiceUseError),
    Auth(graphit_auth::AuthError),
    Model(EmbeddingServiceError),
    Json(serde_json::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Memory search: {self:?}")
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(e) => Some(e),
            Self::Operation(e) => Some(e),
            Self::Service(e) => Some(e),
            Self::Auth(e) => Some(e),
            Self::Model(e) => Some(e),
            Self::Json(e) => Some(e),
        }
    }
}
macro_rules! from_error {
    ($variant:ident,$ty:ty) => {
        impl From<$ty> for Error {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(Store, MemoryStoreError);
from_error!(Operation, std::io::Error);
from_error!(Service, graphit_auth::ServiceUseError);
from_error!(Auth, graphit_auth::AuthError);
from_error!(Model, EmbeddingServiceError);
from_error!(Json, serde_json::Error);
#[derive(Serialize)]
struct Model {
    vector_id: [u8; 32],
    dimensions: u32,
    projection_recipe: &'static str,
}
impl Model {
    fn from_identity(identity: &MemoryEmbeddingIdentity) -> Self {
        Self {
            vector_id: *identity.vector_id(),
            dimensions: identity.dimensions().get(),
            projection_recipe: MEMORY_DOCUMENT_RECIPE,
        }
    }
}
fn native_caller<'a>(
    resources: &'a MemoryJobResources,
    request: &'a RequestContext<'a>,
) -> Result<graphit_auth::AuthenticatedCaller<'a>, Error> {
    let key = resources
        .auth
        .runtime()
        .check(request.snapshot())
        .map_err(std::io::Error::other)?;
    let raw = std::str::from_utf8(
        key.secret(request.snapshot())
            .map_err(std::io::Error::other)?,
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(resources.auth.issuer().authenticate_runtime(raw, request)?)
}
async fn infer(
    resources: &MemoryJobResources,
    request: &RequestContext<'_>,
    service: &mut graphit_auth::ServiceSelection<'_>,
    text: String,
) -> Result<(MemoryEmbeddingIdentity, Vec<f32>), Error> {
    let original = request.snapshot();
    original.check()?;
    let metadata = crate::embedding_service::metadata(service, &resources.execution()).await?;
    let dimensions = u32::try_from(metadata.dimensions)
        .ok()
        .and_then(std::num::NonZeroU32::new)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
    let identity = MemoryEmbeddingIdentity::new(
        metadata.vector_id,
        dimensions,
        MEMORY_DOCUMENT_RECIPE.into(),
    )?;
    let mut output = resources
        .embed(
            service,
            &metadata,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec![text],
                deadline: original.required_deadline()?,
            },
        )
        .await?;
    let vector = output
        .vectors
        .pop()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
    original.check()?;
    Ok((identity, vector))
}
impl From<std::io::ErrorKind> for Error {
    fn from(kind: std::io::ErrorKind) -> Self {
        Self::Operation(kind.into())
    }
}

enum PreparedQuery {
    Semantic(MemorySemanticSearchOptions),
    Hybrid(MemoryHybridQuery),
}

pub(crate) async fn project<'a>(
    request: &'a RequestContext<'a>,
    admission: &ProjectReadAdmission<'_>,
    policy: &TaskPolicy,
    selection: &codec::AdmittedSelection,
    input: Input,
    resources: Option<&'a MemoryJobResources>,
) -> Result<Vec<u8>, Error> {
    let original = request.snapshot();
    let maximum = policy.content.max_output_bytes;
    let limits = limits(policy)?;
    let mode = input.mode;
    let bytes = if matches!(mode, Mode::Fts) {
        let native =
            NativeMemoryStore::search_lexical(original, admission, input.lexical(), limits).await?;
        codec::encode(
            selection,
            &Response {
                mode,
                model: None,
                result: &native,
            },
            maximum,
        )?
    } else if let Some(resources) = resources {
        // Service identity authorizes inference; only the Web admission authorizes corpus access.
        admission.recheck()?;
        let caller = native_caller(resources, request)?;
        let mut service = resources
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Embedding)?;
        let query = if matches!(mode, Mode::Hybrid) {
            PreparedQuery::Hybrid(MemoryHybridQuery::new(
                &input.text,
                MemoryHybridSearchOptions {
                    filter: input.filter.into_native(),
                    top_k: input.top_k,
                    include_deleted_history: input.include_deleted_history,
                },
                limits,
            )?)
        } else {
            PreparedQuery::Semantic(MemorySemanticSearchOptions {
                filter: input.filter.into_native(),
                top_k: input.top_k,
                include_deleted_history: input.include_deleted_history,
            })
        };
        let (identity, vector) = infer(resources, request, &mut service, input.text).await?;
        let model = Some(Model::from_identity(&identity));
        let bytes = match query {
            PreparedQuery::Hybrid(query) => {
                let native = NativeMemoryStore::search_hybrid(
                    original, admission, identity, query, &vector, limits,
                )
                .await?;
                codec::encode(
                    selection,
                    &Response {
                        mode,
                        model,
                        result: &native,
                    },
                    maximum,
                )?
            }
            PreparedQuery::Semantic(options) => {
                let native = NativeMemoryStore::search_semantic(
                    original, admission, identity, &vector, options, limits,
                )
                .await?;
                codec::encode(
                    selection,
                    &Response {
                        mode,
                        model,
                        result: &native,
                    },
                    maximum,
                )?
            }
        };
        service.mode()?;
        bytes
    } else {
        codec::encode(
            selection,
            &Unavailable {
                state: "unavailable",
                mode,
                reason: "model_unavailable",
            },
            maximum,
        )?
    };
    admission.recheck()?;
    original.check()?;
    Ok(bytes)
}
pub(crate) async fn user<'a>(
    request: &'a RequestContext<'a>,
    admission: &UserReadAdmission<'_>,
    policy: &TaskPolicy,
    input: Input,
    resources: Option<&'a MemoryJobResources>,
) -> Result<Vec<u8>, Error> {
    let original = request.snapshot();
    let scope = admission.user()?.live;
    let StoreOwner::User {
        installation_id,
        identity,
    } = scope.owner()
    else {
        return Err(MemoryStoreError::WrongAdmission.into());
    };
    let owner = UserMemoryOwner {
        installation_id: installation_id.clone(),
        identity: identity.clone(),
    };
    let scope_id = UserMemoryScopeId::from_owner(&owner)?;
    let maximum = policy.content.max_output_bytes;
    let limits = limits(policy)?;
    let mode = input.mode;
    let bytes = if matches!(mode, Mode::Fts) {
        let native =
            NativeUserMemoryStore::search_lexical(original, admission, input.lexical(), limits)
                .await?;
        codec::encode_response(
            &crate::foreground_user_memory::Response {
                scope_id: &scope_id,
                current_owner: &owner,
                native: &Response {
                    mode,
                    model: None,
                    result: &native,
                },
            },
            maximum,
        )?
    } else if let Some(resources) = resources {
        admission.recheck()?;
        let caller = native_caller(resources, request)?;
        let mut service = resources
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::Embedding)?;
        let query = if matches!(mode, Mode::Hybrid) {
            PreparedQuery::Hybrid(MemoryHybridQuery::new(
                &input.text,
                MemoryHybridSearchOptions {
                    filter: input.filter.into_native(),
                    top_k: input.top_k,
                    include_deleted_history: input.include_deleted_history,
                },
                limits,
            )?)
        } else {
            PreparedQuery::Semantic(MemorySemanticSearchOptions {
                filter: input.filter.into_native(),
                top_k: input.top_k,
                include_deleted_history: input.include_deleted_history,
            })
        };
        let (identity, vector) = infer(resources, request, &mut service, input.text).await?;
        let model = Some(Model::from_identity(&identity));
        let bytes = match query {
            PreparedQuery::Hybrid(query) => {
                let native = NativeUserMemoryStore::search_hybrid(
                    original, admission, identity, query, &vector, limits,
                )
                .await?;
                codec::encode_response(
                    &crate::foreground_user_memory::Response {
                        scope_id: &scope_id,
                        current_owner: &owner,
                        native: &Response {
                            mode,
                            model,
                            result: &native,
                        },
                    },
                    maximum,
                )?
            }
            PreparedQuery::Semantic(options) => {
                let native = NativeUserMemoryStore::search_semantic(
                    original, admission, identity, &vector, options, limits,
                )
                .await?;
                codec::encode_response(
                    &crate::foreground_user_memory::Response {
                        scope_id: &scope_id,
                        current_owner: &owner,
                        native: &Response {
                            mode,
                            model,
                            result: &native,
                        },
                    },
                    maximum,
                )?
            }
        };
        service.mode()?;
        bytes
    } else {
        codec::encode_response(
            &crate::foreground_user_memory::Response {
                scope_id: &scope_id,
                current_owner: &owner,
                native: &Unavailable {
                    state: "unavailable",
                    mode,
                    reason: "model_unavailable",
                },
            },
            maximum,
        )?
    };
    admission.recheck()?;
    original.check()?;
    Ok(bytes)
}
