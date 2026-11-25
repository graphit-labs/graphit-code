//! Short content CAS and atomic replacement of auth.json.
use super::*;
pub(super) const LOCK: &str = ".auth.json.lock";
pub const AUTH_PRIVATE_NAMES: [&str; 2] = ["auth.json", LOCK];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthPublicationEffect {
    Applied,
    Unchanged,
}

impl AuthDocumentStore {
    /// This local transaction must not span HTTP or async profile-lock waiting.
    pub fn lock_latest<'a>(
        &'a self,
        original: &'a SnapshotContext,
    ) -> Result<AuthDocumentMutation<'a>, AuthDocumentError> {
        self.check(original)?;
        let lock = private_document::lock(&self.root.join(LOCK), original, false)?;
        Ok(AuthDocumentMutation {
            store: self,
            original,
            _lock: lock,
            snapshot: self.read(original)?,
        })
    }
    pub fn publish(
        &self,
        expected: AuthDocumentSnapshot,
        body: &[u8],
        original: &SnapshotContext,
    ) -> Result<AuthPublicationEffect, AuthDocumentError> {
        self.check(original)?;
        let _lock = private_document::lock(&self.root.join(LOCK), original, false)?;
        self.publish_locked(expected, body, original)
    }
    fn publish_locked(
        &self,
        expected: AuthDocumentSnapshot,
        body: &[u8],
        original: &SnapshotContext,
    ) -> Result<AuthPublicationEffect, AuthDocumentError> {
        self.check(original)?;
        if body.len() > self.limits.bytes {
            return Err(AuthDocumentError::Limit);
        }
        if self.read(original)?.bytes() != expected.bytes() {
            return Err(AuthDocumentError::Conflict);
        }
        if expected.bytes() == Some(body) {
            original.check()?;
            return Ok(AuthPublicationEffect::Unchanged);
        }
        #[cfg(feature = "auth-test-support")]
        test_support::trigger_before_replace(self, original);
        private_document::write_atomic(
            &self.root.join("auth.json"),
            expected.bytes(),
            body,
            self.limits.bytes,
            original,
        )?;
        Ok(AuthPublicationEffect::Applied)
    }
}
impl AuthDocumentMutation<'_> {
    pub fn bytes(&self) -> Option<&[u8]> {
        self.snapshot.bytes()
    }
    pub fn publish(self, body: &[u8]) -> Result<AuthPublicationEffect, AuthDocumentError> {
        self.store
            .publish_locked(self.snapshot, body, self.original)
    }
}
