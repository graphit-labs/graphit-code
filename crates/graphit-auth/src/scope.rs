use crate::policy::AuthError;
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeContext, StoreOwner},
    installation_identity::InstallationId,
    project_identity::ProjectId,
    version_context::VersionContext,
};
use graphit_hub::{NativeBinding, policy::RequestContext};

/// Caller-held real capture and bounded data observation; this is not a grant.
#[derive(Clone, Copy)]
pub struct ScopeCapture<'a> {
    pub live: &'a CapturedPublicationScope,
    pub record: &'a ScopeBindingRecord,
}
#[derive(Clone, Copy)]
pub struct ProjectCapture<'a> {
    pub scope: ScopeCapture<'a>,
    pub version: &'a VersionContext,
}
#[derive(Clone, Copy)]
pub struct ScopeInput<'a> {
    pub installation: ScopeCapture<'a>,
    pub project: Option<ProjectCapture<'a>>,
    pub providers: &'a [ScopeCapture<'a>],
    pub binding: NativeBinding,
    pub global_version: &'a str,
}
pub(crate) struct ScopeFacts {
    installation: ScopeBindingRecord,
    project: Option<(ProjectId, ScopeBindingRecord)>,
    providers: Vec<ScopeBindingRecord>,
    installation_original: CapturedPublicationScope,
    originals: Vec<CapturedPublicationScope>,
    version: String,
    pub binding: NativeBinding,
}
/// Physical captures validated for this request; this subset carries no permission.
#[derive(Clone, Copy)]
pub(crate) struct PhysicalScopeInput<'a> {
    pub(crate) installation: ScopeCapture<'a>,
    pub(crate) project: Option<ProjectCapture<'a>>,
    pub(crate) providers: &'a [ScopeCapture<'a>],
}
impl ScopeInput<'_> {
    pub(crate) fn physical(&self) -> PhysicalScopeInput<'_> {
        PhysicalScopeInput {
            installation: self.installation,
            project: self.project,
            providers: self.providers,
        }
    }
    pub(crate) fn validate(
        &self,
        installation: &InstallationId,
        brand: &str,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        self.physical().validate(installation, brand, request)?;
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
impl PhysicalScopeInput<'_> {
    pub(crate) fn validate(
        &self,
        installation: &InstallationId,
        brand: &str,
        request: &RequestContext<'_>,
    ) -> Result<(), AuthError> {
        request.check().map_err(|_| AuthError::Request)?;
        if !matches!(self.installation.live.owner(), StoreOwner::Installation { installation_id } if installation_id == installation)
        {
            return Err(AuthError::Denied);
        }
        if self.project.is_some_and(|project| {
            !matches!(project.scope.live.owner(), StoreOwner::Project { .. })
                || project.version.scope() != project.scope.live.owner_root()
        }) {
            return Err(AuthError::Denied);
        }
        let context = ScopeContext::from_snapshot(request.snapshot());
        for (index, capture) in std::iter::once(self.installation)
            .chain(self.project.map(|project| project.scope))
            .chain(self.providers.iter().copied())
            .enumerate()
        {
            request.check().map_err(|_| AuthError::Request)?;
            if capture.live.brand().key() != brand
                || !capture
                    .record
                    .matches_scope_bounded(capture.live, &context)
                    .map_err(|_| AuthError::Denied)?
            {
                return Err(AuthError::Denied);
            }
            let owner_ok = match capture.live.owner() {
                StoreOwner::User { .. } => false,
                StoreOwner::Installation { installation_id } => installation_id == installation,
                StoreOwner::Project { project_id } => self.project.is_some_and(|project| {
                    matches!(project.scope.live.owner(), StoreOwner::Project { project_id: expected } if expected == project_id)
                }),
            };
            if !owner_ok {
                return Err(AuthError::Denied);
            }
            if index == 0 {
                capture.live.revalidate_installation(&context)
            } else {
                capture.live.revalidate_bounded(&context)
            }
            .map_err(|_| AuthError::Denied)?;
        }
        request.check().map_err(|_| AuthError::Request)
    }
}
impl ScopeFacts {
    pub(crate) fn capture(input: &ScopeInput<'_>) -> Result<Self, AuthError> {
        let originals = input
            .project
            .map(|project| project.scope)
            .into_iter()
            .chain(input.providers.iter().copied())
            .map(|capture| capture.live.clone())
            .collect();
        let providers = input
            .providers
            .iter()
            .map(|provider| provider.record.clone())
            .collect();
        Ok(Self {
            installation: input.installation.record.clone(),
            project: input.project.map(|p| {
                let StoreOwner::Project { project_id } = p.scope.live.owner() else {
                    unreachable!("validated project capture")
                };
                (*project_id, p.scope.record.clone())
            }),
            providers,
            installation_original: input.installation.live.clone(),
            originals,
            version: input
                .project
                .map(|p| p.version.version())
                .unwrap_or(input.global_version)
                .into(),
            binding: input.binding,
        })
    }
    pub(crate) fn matches(
        &self,
        input: &ScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<bool, AuthError> {
        let context = ScopeContext::from_snapshot(request.snapshot());
        if self.binding != input.binding
            || self.providers.len() != input.providers.len()
            || !self
                .installation
                .matches_scope_bounded(input.installation.live, &context)
                .map_err(|_| AuthError::Denied)?
        {
            return Ok(false);
        }
        let version = input
            .project
            .map(|p| p.version.version())
            .unwrap_or(input.global_version);
        if self.version != version {
            return Ok(false);
        }
        match (&self.project, input.project) {
            (None, None) => {}
            (Some((id, record)), Some(project)) => {
                if !matches!(project.scope.live.owner(),StoreOwner::Project{project_id}if project_id==id)
                    || !record
                        .matches_scope_bounded(project.scope.live, &context)
                        .map_err(|_| AuthError::Denied)?
                {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
        for (record, capture) in self.providers.iter().zip(input.providers) {
            if !record
                .matches_scope_bounded(capture.live, &context)
                .map_err(|_| AuthError::Denied)?
            {
                return Ok(false);
            }
        }
        request.check().map_err(|_| AuthError::Request)?;
        Ok(true)
    }
    pub(crate) fn revalidate(&self, request: &RequestContext<'_>) -> Result<(), AuthError> {
        let context = ScopeContext::from_snapshot(request.snapshot());
        request.check().map_err(|_| AuthError::Request)?;
        self.installation_original
            .revalidate_installation(&context)
            .map_err(|_| AuthError::Denied)?;
        for original in &self.originals {
            request.check().map_err(|_| AuthError::Request)?;
            original
                .revalidate_bounded(&context)
                .map_err(|_| AuthError::Denied)?;
        }
        Ok(())
    }
    pub(crate) fn matches_checked(
        &self,
        input: &ScopeInput<'_>,
        request: &RequestContext<'_>,
    ) -> Result<bool, AuthError> {
        self.revalidate(request)?;
        self.matches(input, request)
    }
}
