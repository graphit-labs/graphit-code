use crate::policy::AuthError;
use graphit_core::{
    artifact_store::{PublicationScope, StoreOwner},
    installation_identity::InstallationId,
    version_context::VersionContext,
};
use graphit_hub::{NativeBinding, policy::RequestContext};

/// Logical PROJECT selection. This data grants no permission.
#[derive(Clone, Copy)]
pub struct ProjectSelection<'a> {
    pub scope: &'a PublicationScope,
    pub version: &'a VersionContext,
}
#[derive(Clone, Copy)]
pub struct ScopeInput<'a> {
    pub installation: &'a PublicationScope,
    pub project: Option<ProjectSelection<'a>>,
    pub providers: &'a [&'a PublicationScope],
    pub binding: NativeBinding,
    pub global_version: &'a str,
}
pub(crate) struct ScopeFacts {
    installation: PublicationScope,
    project: Option<PublicationScope>,
    providers: Vec<PublicationScope>,
    version: String,
    pub binding: NativeBinding,
}
impl ScopeInput<'_> {
    pub(crate) fn validate(
        &self,
        installation: &InstallationId,
        brand: &str,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        validate_owners(
            self.installation,
            self.project,
            self.providers,
            installation,
            brand,
            request,
        )?;
        if self.project.is_none() {
            let version = graphit_core::project_version::validate(self.global_version)
                .map_err(|_| AuthError::Invalid)?;
            if version != self.global_version || version.is_empty() {
                return Err(AuthError::Invalid);
            }
        }
        Ok(())
    }
}
pub(crate) fn validate_owners(
    realm: &PublicationScope,
    project: Option<ProjectSelection<'_>>,
    providers: &[&PublicationScope],
    installation: &InstallationId,
    brand: &str,
    request: &RequestContext<'_>,
) -> Result<(), AuthError> {
    request.check().map_err(|_| AuthError::Request)?;
    if !matches!(realm.owner(), StoreOwner::Installation { installation_id } if installation_id == installation)
    {
        return Err(AuthError::Denied);
    }
    if project.is_some_and(|project| {
        !matches!(project.scope.owner(), StoreOwner::Project { .. })
            || project.version.scope() != project.scope.owner_root()
    }) {
        return Err(AuthError::Denied);
    }
    for scope in std::iter::once(realm)
        .chain(project.map(|project| project.scope))
        .chain(providers.iter().copied())
    {
        request.check().map_err(|_| AuthError::Request)?;
        let owner_ok = match scope.owner() {
                StoreOwner::User { .. } => false,
                StoreOwner::Installation { installation_id } => installation_id == installation,
                StoreOwner::Project { project_id } => project.is_some_and(|project| {
                    matches!(project.scope.owner(), StoreOwner::Project { project_id: expected } if expected == project_id)
                }),
            };
        if scope.brand().key() != brand || !owner_ok {
            return Err(AuthError::Denied);
        }
    }
    request.check().map_err(|_| AuthError::Request)
}
impl ScopeFacts {
    pub(crate) fn from_input(input: &ScopeInput<'_>) -> Self {
        Self {
            installation: input.installation.clone(),
            project: input.project.map(|project| project.scope.clone()),
            providers: input
                .providers
                .iter()
                .map(|scope| (*scope).clone())
                .collect(),
            version: input
                .project
                .map(|project| project.version.version())
                .unwrap_or(input.global_version)
                .into(),
            binding: input.binding,
        }
    }
    pub(crate) fn matches(
        &self,
        input: &ScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<bool, AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        Ok(self.binding == input.binding
            && self.installation == *input.installation
            && self.project.as_ref() == input.project.map(|project| project.scope)
            && self.providers.iter().eq(input.providers.iter().copied())
            && self.version
                == input
                    .project
                    .map(|project| project.version.version())
                    .unwrap_or(input.global_version))
    }
}
