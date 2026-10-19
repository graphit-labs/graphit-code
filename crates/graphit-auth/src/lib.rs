//! Bounded authentication and role authority. Selectors remain data.
pub mod broker;
mod issuer;
mod policy;
pub mod profile;
pub mod profile_manager;
mod scope;
pub mod store;
pub use issuer::{
    Actor, AdminParent, AdminSessionHandle, AuthenticatedCaller, CredentialSource,
    ImportedAstReadAdmission, ImportedKnowledgeReadAdmission, IncomingPrincipalInput,
    InstalledHubReadAdmission, IssuedRole, LocalServiceLease, NativeProfileRevision,
    ProjectReadAdmission, ProjectReadModule, ProjectRecordAuthority, ProjectWriteAdmission,
    ProjectWriteOperation, RecordStorageAdmission, RecordStorageResolver, RecordStorageScope,
    RemoteRecordStorage, RemoteServiceBindingInput, RoleBearer, RoleIssuer, RunHandle,
    S3CredentialView, ServiceAdmissionLimits, ServiceAuthorityInput, ServiceBindingView,
    ServiceCredentialLease, ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput,
    ServiceSelection, ServiceUseError, StandaloneS3View, SubjectSelector,
    UserKnowledgeReadAdmission, UserKnowledgeWriteAdmission, UserMemoryWriteOperation,
    UserReadAdmission, UserWriteAdmission, VerifiedRoleGrant, VerifiedUserOrigin,
    VerifiedUserSubject, WebHubMutationAdmission, WebHubProjectMutationInput,
    WebHubPublicationAdmission, WebHubPublicationInput,
};
pub use policy::{
    AdminSessionPolicy, AuthError, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits, Obligation,
    Permission, RoleDefinition, Setting, SourceAuthorityPolicy, TtlPolicy,
};
pub use scope::{ProjectCapture, ScopeCapture, ScopeInput};
