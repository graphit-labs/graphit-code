//! Fixed, opaque observations of the current effective native principal.
//! No observation is a permission or a filesystem ownership assertion.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod windows_facts;

/// Static failures; native identities and SDK error payloads stay private.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePrincipalError {
    Unavailable,
    InvalidObservation,
    Changed,
    Unsupported,
}

/// Owned identity facts only. No token handle, pointer, or serialized identity.
pub struct NativePrincipalObservation {
    #[cfg(unix)]
    facts: unix::Facts,
    #[cfg(windows)]
    facts: windows_facts::Facts,
}
impl NativePrincipalObservation {
    pub fn capture() -> Result<Self, NativePrincipalError> {
        #[cfg(unix)]
        {
            Ok(Self {
                facts: unix::capture()?,
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                facts: windows::capture()?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(NativePrincipalError::Unsupported)
        }
    }

    pub fn recheck(&self) -> Result<(), NativePrincipalError> {
        #[cfg(unix)]
        {
            unchanged(&self.facts, &unix::capture()?)
        }
        #[cfg(windows)]
        {
            unchanged(&self.facts, &windows::capture()?)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(NativePrincipalError::Unsupported)
        }
    }
}

fn unchanged<T: PartialEq>(original: &T, current: &T) -> Result<(), NativePrincipalError> {
    if original == current {
        Ok(())
    } else {
        Err(NativePrincipalError::Changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_facts_are_not_rebased() {
        assert_eq!(unchanged(&1, &2), Err(NativePrincipalError::Changed));
        assert_eq!(unchanged(&1, &1), Ok(()));
    }
    #[test]
    fn owned_observation_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NativePrincipalObservation>();
    }
    #[cfg(any(unix, windows))]
    #[test]
    fn real_sdk_capture_recheck_and_thread_transfer() {
        let observation = NativePrincipalObservation::capture().expect("real SDK capture");
        observation.recheck().expect("same effective principal");
        std::thread::spawn(move || observation.recheck())
            .join()
            .unwrap()
            .unwrap();
    }
}
