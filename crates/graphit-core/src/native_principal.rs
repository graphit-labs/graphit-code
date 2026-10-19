//! Request-bound observations of the effective process principal.
//!
//! This proof grants no permissions and performs no filesystem I/O.
//! Native SDK temporary token-information buffers remain owned by the SDK.
use crate::file_io::{RequestIdentity, SnapshotContext};
use graphit_fs::{NativePrincipalError, NativePrincipalObservation};
use std::io;

/// Closed diagnostics without native identity or SDK payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalError {
    Cancelled,
    Deadline,
    WrongRequest,
    Unavailable,
    InvalidObservation,
    Changed,
    Unsupported,
}
impl std::fmt::Display for PrincipalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native principal: {self:?}")
    }
}
impl std::error::Error for PrincipalError {}

/// Actual SDK facts bound to the original request, never a grant or login.
///
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// fn duplicate(p: NativeProcessPrincipal) { let _ = p.clone(); }
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// fn expose(p: NativeProcessPrincipal) { println!("{p:?}"); }
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// fn serialize<T: serde::Serialize>() {}
/// serialize::<NativeProcessPrincipal>();
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// let _ = NativeProcessPrincipal::from_parts(0, 0, 0);
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// fn raw_ids(p: NativeProcessPrincipal) { let _ = (p.pid(), p.uid(), p.gid(), p.sid()); }
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// fn fields(p: NativeProcessPrincipal) { let _ = p.observation; }
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// fn handle(p: NativeProcessPrincipal) { let _ = p.handle(); }
/// ```
/// ```compile_fail
/// use graphit_core::native_principal::NativeProcessPrincipal;
/// let _ = NativeProcessPrincipal::restore(&[]);
/// ```
pub struct NativeProcessPrincipal {
    observation: NativePrincipalObservation,
    request: RequestIdentity,
}
impl NativeProcessPrincipal {
    pub fn capture(context: &SnapshotContext) -> Result<Self, PrincipalError> {
        Self::capture_using(context, NativePrincipalObservation::capture)
    }

    fn capture_using(
        context: &SnapshotContext,
        observe: impl FnOnce() -> Result<NativePrincipalObservation, NativePrincipalError>,
    ) -> Result<Self, PrincipalError> {
        check(context)?;
        let request = context.request_identity();
        let observation = observe();
        check(context)?;
        Ok(Self {
            observation: observation.map_err(principal_error)?,
            request,
        })
    }

    /// Re-observes the current effective principal without renewing this proof.
    /// A shared original request is valid; a fresh request is not.
    /// Future consumers must place the final recheck immediately before their
    /// synchronous effect: this observation cannot make a later effect atomic.
    pub fn recheck(&self, context: &SnapshotContext) -> Result<(), PrincipalError> {
        self.recheck_using(context, NativePrincipalObservation::recheck)
    }

    fn recheck_using(
        &self,
        context: &SnapshotContext,
        observe: impl FnOnce(&NativePrincipalObservation) -> Result<(), NativePrincipalError>,
    ) -> Result<(), PrincipalError> {
        check(context)?;
        if !self.request.matches(context) {
            return Err(PrincipalError::WrongRequest);
        }
        let observed = observe(&self.observation);
        check(context)?;
        observed.map_err(principal_error)
    }
}

fn check(context: &SnapshotContext) -> Result<(), PrincipalError> {
    context.check().map_err(|error| match error.kind() {
        io::ErrorKind::TimedOut => PrincipalError::Deadline,
        _ => PrincipalError::Cancelled,
    })
}
fn principal_error(error: NativePrincipalError) -> PrincipalError {
    match error {
        NativePrincipalError::Unavailable => PrincipalError::Unavailable,
        NativePrincipalError::InvalidObservation => PrincipalError::InvalidObservation,
        NativePrincipalError::Changed => PrincipalError::Changed,
        NativePrincipalError::Unsupported => PrincipalError::Unsupported,
    }
}

#[cfg(test)]
mod tests;
