use super::service_credentials::{ServiceCredentialLease, ServiceUseError, original_check};
use super::{AuthenticatedCaller, RoleIssuer};
use crate::broker::oauth::web::AuthenticatedWebCaller;
use crate::broker::{
    AuthenticatedBrokerCaller, BrokerRequestContext, BrokerVerifier, VerifiedNativeLogin,
};
use graphit_hub::policy::RequestContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedUserOrigin {
    Native,
    Incoming,
    Web,
}

pub(super) struct SubjectIdentity<'a> {
    pub issuer: &'a str,
    pub subject: &'a str,
    pub username: Option<&'a str>,
    pub teams: &'a [String],
    pub origin: VerifiedUserOrigin,
}
/// Private, implemented only by the actual service lease; not a grant callback.
pub(super) trait ServiceSubjectProof: Sync {
    fn subject_identity(&self) -> Result<Option<SubjectIdentity<'_>>, ServiceUseError>;
}
enum Source<'a> {
    Authenticated(&'a AuthenticatedCaller<'a>),
    Native {
        proof: &'a VerifiedNativeLogin<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    },
    Incoming {
        proof: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    },
    Web {
        proof: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    },
    Service(&'a dyn ServiceSubjectProof),
}
/// Literal verified identity, without permission or USER storage authority.
/// ```compile_fail
/// use graphit_auth::VerifiedUserSubject;
/// fn debug<T: std::fmt::Debug>() {} debug::<VerifiedUserSubject<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedUserSubject;
/// fn clone<T: Clone>() {} clone::<VerifiedUserSubject<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedUserSubject;
/// fn serialize<T: serde::Serialize>() {} serialize::<VerifiedUserSubject<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedUserSubject;
/// fn restore<T: serde::de::DeserializeOwned>() {} restore::<VerifiedUserSubject<'static>>();
/// ```
/// ```compile_fail
/// use graphit_auth::{Actor, VerifiedUserSubject};
/// fn user(actor: Actor) -> VerifiedUserSubject<'static> { actor.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::{VerifiedRoleGrant, VerifiedUserSubject};
/// fn user<'a>(role: VerifiedRoleGrant<'a>) -> VerifiedUserSubject<'a> { role.into() }
/// ```
/// ```compile_fail
/// use graphit_auth::{ServiceCredentialLease, VerifiedUserSubject};
/// fn user<'a>(lease: &'a ServiceCredentialLease<'a>) -> VerifiedUserSubject<'static> {
///     lease.user_subject().unwrap().unwrap()
/// }
/// ```
/// ```compile_fail
/// use graphit_auth::VerifiedUserSubject;
/// let forged = VerifiedUserSubject { source: panic!("caller-selected source") };
/// ```
pub struct VerifiedUserSubject<'a> {
    source: Source<'a>,
}

impl RoleIssuer {
    /// Literal identity from this call's actual authenticated Native phase; no USER permission.
    pub fn user_from_authenticated<'a>(
        &'a self,
        caller: &'a AuthenticatedCaller<'a>,
    ) -> Result<VerifiedUserSubject<'a>, ServiceUseError> {
        if !std::ptr::eq(self, caller.issuer) {
            return Err(crate::AuthError::Denied.into());
        }
        let subject = VerifiedUserSubject {
            source: Source::Authenticated(caller),
        };
        subject.check()?;
        Ok(subject)
    }
    pub fn user_from_native<'a>(
        &'a self,
        proof: &'a VerifiedNativeLogin<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    ) -> Result<VerifiedUserSubject<'a>, ServiceUseError> {
        let subject = VerifiedUserSubject {
            source: Source::Native {
                proof,
                verifier,
                original,
            },
        };
        subject.check()?;
        Ok(subject)
    }
    pub fn user_from_incoming<'a>(
        &'a self,
        proof: &'a AuthenticatedBrokerCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    ) -> Result<VerifiedUserSubject<'a>, ServiceUseError> {
        let subject = VerifiedUserSubject {
            source: Source::Incoming {
                proof,
                verifier,
                original,
            },
        };
        subject.check()?;
        Ok(subject)
    }
    /// Retain literal Web identity from an existing verified Web access proof.
    /// This grants no PROJECT or USER permission.
    pub fn user_from_web<'a>(
        &'a self,
        proof: &'a AuthenticatedWebCaller<'a>,
        verifier: &'a BrokerVerifier,
        original: &'a RequestContext<'a>,
    ) -> Result<VerifiedUserSubject<'a>, ServiceUseError> {
        let subject = VerifiedUserSubject {
            source: Source::Web {
                proof,
                verifier,
                original,
            },
        };
        subject.check()?;
        Ok(subject)
    }
}
impl VerifiedUserSubject<'_> {
    pub(super) fn identity(&self) -> Result<SubjectIdentity<'_>, ServiceUseError> {
        match &self.source {
            Source::Authenticated(caller) => {
                caller.check()?;
                caller
                    .native
                    .as_ref()
                    .ok_or(crate::AuthError::Denied)?
                    .user_identity(caller)
            }
            Source::Native {
                proof,
                verifier,
                original,
            } => {
                original_check(original)?;
                proof.check(verifier, BrokerRequestContext::new(original.snapshot()))?;
                Ok(SubjectIdentity {
                    issuer: proof.issuer(),
                    subject: proof.subject(),
                    username: proof.username(),
                    teams: proof.teams(),
                    origin: VerifiedUserOrigin::Native,
                })
            }
            Source::Incoming {
                proof,
                verifier,
                original,
            } => {
                original_check(original)?;
                proof.check(verifier, BrokerRequestContext::new(original.snapshot()))?;
                Ok(SubjectIdentity {
                    issuer: proof.issuer(),
                    subject: proof.subject(),
                    username: proof.username(),
                    teams: proof.teams(),
                    origin: VerifiedUserOrigin::Incoming,
                })
            }
            Source::Web {
                proof,
                verifier,
                original,
            } => {
                original_check(original)?;
                proof.check(verifier, BrokerRequestContext::new(original.snapshot()))?;
                Ok(SubjectIdentity {
                    issuer: proof.issuer(),
                    subject: proof.subject(),
                    username: proof.username(),
                    teams: proof.teams(),
                    origin: VerifiedUserOrigin::Web,
                })
            }
            Source::Service(lease) => lease.subject_identity()?.ok_or(ServiceUseError::WrongMode),
        }
    }
    pub fn check(&self) -> Result<(), ServiceUseError> {
        self.identity().map(|_| ())
    }
    pub fn issuer(&self) -> Result<&str, ServiceUseError> {
        self.identity().map(|p| p.issuer)
    }
    pub fn subject(&self) -> Result<&str, ServiceUseError> {
        self.identity().map(|p| p.subject)
    }
    /// Access-JWT identity data; authorization must evaluate its own current ACL.
    pub fn username(&self) -> Result<Option<&str>, ServiceUseError> {
        self.identity().map(|p| p.username)
    }
    pub fn teams(&self) -> Result<&[String], ServiceUseError> {
        self.identity().map(|p| p.teams)
    }
    pub fn origin(&self) -> Result<VerifiedUserOrigin, ServiceUseError> {
        self.identity().map(|p| p.origin)
    }
}
impl ServiceCredentialLease<'_> {
    pub fn user_subject(&self) -> Result<Option<VerifiedUserSubject<'_>>, ServiceUseError> {
        if self.subject_identity()?.is_none() {
            return Ok(None);
        }
        Ok(Some(VerifiedUserSubject {
            source: Source::Service(self),
        }))
    }
}
