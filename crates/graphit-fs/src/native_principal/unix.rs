use super::{NativePrincipalError, unchanged};

#[derive(PartialEq, Eq)]
pub(super) struct Facts {
    pid: rustix::process::Pid,
    uid: rustix::process::Uid,
    gid: rustix::process::Gid,
}

fn observe() -> Facts {
    Facts {
        pid: rustix::process::getpid(),
        uid: rustix::process::geteuid(),
        gid: rustix::process::getegid(),
    }
}

pub(super) fn capture() -> Result<Facts, NativePrincipalError> {
    capture_using(observe)
}

fn capture_using(mut observe: impl FnMut() -> Facts) -> Result<Facts, NativePrincipalError> {
    let first = observe();
    let second = observe();
    unchanged(&first, &second)?;
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_sdk_has_two_stable_observations() {
        assert!(capture().is_ok());
    }
    #[test]
    fn unstable_effective_identity_or_process_denies_without_retry() {
        for changed_field in 0..3 {
            let mut calls = 0;
            let result = capture_using(|| {
                calls += 1;
                let mut facts = observe();
                if calls == 2 {
                    match changed_field {
                        0 => {
                            facts.pid =
                                rustix::process::Pid::from_raw(if facts.pid.as_raw_pid() == 1 {
                                    2
                                } else {
                                    1
                                })
                                .unwrap()
                        }
                        1 => {
                            facts.uid =
                                rustix::process::Uid::from_raw(facts.uid.as_raw().wrapping_add(1))
                        }
                        _ => {
                            facts.gid =
                                rustix::process::Gid::from_raw(facts.gid.as_raw().wrapping_add(1))
                        }
                    }
                }
                facts
            });
            assert_eq!(result.err(), Some(NativePrincipalError::Changed));
            assert_eq!(calls, 2);
        }
    }
}
