use super::{BrokerError, BrokerMetadata, Moment, oauth::web::RegisteredWebClient};
use serde::Deserialize;
use std::time::{Duration, UNIX_EPOCH};
use subtle::ConstantTimeEq;

#[derive(Clone, Copy)]
pub(super) enum TokenPurpose<'a> {
    IncomingMcp,
    NativeId,
    NativeAccess,
    WebId(&'a RegisteredWebClient),
    WebAccess(&'a RegisteredWebClient),
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    Single(String),
    Multiple(Vec<String>),
}
impl Audience {
    fn contains(&self, expected: &str) -> bool {
        match self {
            Self::Single(value) => value == expected,
            Self::Multiple(values) => values.iter().any(|value| value == expected),
        }
    }
    fn native_id(
        &self,
        client_id: &str,
        access_audience: &str,
        resource: Option<&str>,
        azp: Option<&str>,
    ) -> bool {
        if azp.is_some_and(|value| value != client_id) {
            return false;
        }
        match self {
            Self::Single(value) => value == client_id,
            Self::Multiple(values) => {
                !values.is_empty()
                    && self.contains(client_id)
                    && (values.len() == 1 || azp == Some(client_id))
                    && values.iter().all(|value| {
                        value == client_id
                            || value == access_audience
                            || resource == Some(value.as_str())
                    })
            }
        }
    }
}
#[derive(Deserialize)]
pub(super) struct Claims {
    pub(super) iss: String,
    pub(super) sub: String,
    aud: Audience,
    pub(super) iat: u64,
    pub(super) exp: u64,
    #[serde(default)]
    nbf: Option<u64>,
    #[serde(default)]
    pub(super) client_id: Option<String>,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    token_use: Option<String>,
    #[serde(default)]
    pub(super) preferred_username: Option<String>,
    #[serde(default)]
    pub(super) organization: Option<String>,
    #[serde(default)]
    pub(super) groups: Option<Vec<String>>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    azp: Option<String>,
}
impl Claims {
    pub(super) fn validate(
        &self,
        purpose: TokenPurpose,
        metadata: &BrokerMetadata,
        resource: Option<&str>,
        anchor: Moment,
    ) -> Result<(), BrokerError> {
        let now = anchor
            .utc
            .duration_since(UNIX_EPOCH)
            .map_err(|_| BrokerError::InvalidToken)?
            .as_secs();
        // Admit both clocks for each token independently, before taking the
        // pair's minimum lifetime. An overflowing ID lifetime cannot hide
        // behind a shorter, valid access token.
        UNIX_EPOCH
            .checked_add(Duration::from_secs(self.exp))
            .and_then(|expiry| expiry.duration_since(anchor.utc).ok())
            .and_then(|remaining| anchor.mono.checked_add(remaining))
            .ok_or(BrokerError::InvalidToken)?;
        if self.iss != metadata.issuer
            || self.sub.is_empty()
            || self.iat > now
            || self.exp <= now
            || self.exp <= self.iat
            || self.nbf.is_some_and(|value| value > now)
        {
            return Err(BrokerError::InvalidToken);
        }
        let valid = match purpose {
            TokenPurpose::IncomingMcp => {
                self.access() && resource.is_some_and(|value| self.aud.contains(value))
            }
            TokenPurpose::NativeAccess => {
                self.access()
                    && self.client_id.as_deref() == Some(metadata.client_id.as_str())
                    && self.aud.contains(&metadata.access_token_audience)
                    && resource.is_none_or(|value| self.aud.contains(value))
            }
            TokenPurpose::WebAccess(client) => {
                self.access()
                    && self.client_id.as_deref() == Some(client.client_id())
                    && self.aud.contains(&metadata.access_token_audience)
                    && self.aud.contains(client.resource())
            }
            TokenPurpose::WebId(client) => self.aud.native_id(
                client.client_id(),
                &metadata.access_token_audience,
                Some(client.resource()),
                self.azp.as_deref(),
            ),
            TokenPurpose::NativeId => {
                self.aud.native_id(
                    &metadata.client_id,
                    &metadata.access_token_audience,
                    resource,
                    self.azp.as_deref(),
                ) && self
                    .preferred_username
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
            }
        };
        if valid {
            Ok(())
        } else {
            Err(BrokerError::InvalidToken)
        }
    }
    fn access(&self) -> bool {
        self.token_use.as_deref() == Some("access")
            && self
                .client_id
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            && self.jti.as_ref().is_some_and(|value| !value.is_empty())
    }
    pub(super) fn check_nonce(&self, expected: &str) -> Result<(), BrokerError> {
        let actual = self.nonce.as_deref().ok_or(BrokerError::InvalidToken)?;
        if bool::from(actual.as_bytes().ct_eq(expected.as_bytes())) {
            Ok(())
        } else {
            Err(BrokerError::InvalidToken)
        }
    }
    pub(super) fn into_profile(self) -> Result<VerifiedNativeProfileClaims, BrokerError> {
        Ok(VerifiedNativeProfileClaims {
            username: self.preferred_username.ok_or(BrokerError::InvalidToken)?,
            organization: self.organization,
            teams: self.groups.unwrap_or_default(),
        })
    }
}

/// Bounded profile attributes from a verified native ID token, never permissions.
pub struct VerifiedNativeProfileClaims {
    username: String,
    organization: Option<String>,
    teams: Vec<String>,
}
impl VerifiedNativeProfileClaims {
    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn organization(&self) -> Option<&str> {
        self.organization.as_deref()
    }
    pub fn teams(&self) -> &[String] {
        &self.teams
    }
}
