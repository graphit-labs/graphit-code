use super::{NativePrincipalError, unchanged};

const MAX_SID_BYTES: usize = 68;
const MAX_SUBAUTHORITIES: usize = (MAX_SID_BYTES - 8) / 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    Process,
    Thread,
}

/// Only ERROR_NO_TOKEN permits falling back to the process token.
pub(super) fn select_origin(error: Option<u32>) -> Result<Origin, NativePrincipalError> {
    match error {
        None => Ok(Origin::Thread),
        Some(1008) => Ok(Origin::Process),
        Some(_) => Err(NativePrincipalError::Unavailable),
    }
}

pub(super) fn valid_sid(status: Result<bool, ()>) -> Result<(), NativePrincipalError> {
    match status {
        Ok(true) => Ok(()),
        Ok(false) | Err(()) => Err(NativePrincipalError::InvalidObservation),
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Sid([u8; MAX_SID_BYTES]);

/// Called only with SDK-validated SID views; also checks their bounded shape.
pub(super) fn normalize_sid(
    revision: u8,
    authority: [u8; 6],
    count: usize,
    sdk_length: u32,
    mut subauthorities: impl Iterator<Item = u32>,
) -> Result<Sid, NativePrincipalError> {
    let length = count
        .checked_mul(4)
        .and_then(|n| n.checked_add(8))
        .ok_or(NativePrincipalError::InvalidObservation)?;
    if revision != 1
        || count > MAX_SUBAUTHORITIES
        || length > MAX_SID_BYTES
        || usize::try_from(sdk_length).ok() != Some(length)
    {
        return Err(NativePrincipalError::InvalidObservation);
    }
    let mut bytes = [0; MAX_SID_BYTES];
    bytes[0] = revision;
    bytes[1] = count as u8;
    bytes[2..8].copy_from_slice(&authority);
    for slot in bytes[8..length].as_chunks_mut::<4>().0 {
        slot.copy_from_slice(
            &subauthorities
                .next()
                .ok_or(NativePrincipalError::InvalidObservation)?
                .to_le_bytes(),
        );
    }
    if subauthorities.next().is_some() {
        return Err(NativePrincipalError::InvalidObservation);
    }
    Ok(Sid(bytes))
}

#[derive(PartialEq, Eq)]
pub(super) struct Statistics {
    token: [u32; 2],
    authentication: [u32; 2],
    modified: [u32; 2],
    token_type: u32,
    level: Option<u32>,
}
impl Statistics {
    pub(super) fn normalize(
        token: [u32; 2],
        authentication: [u32; 2],
        modified: [u32; 2],
        token_type: u32,
        level: u32,
    ) -> Result<Self, NativePrincipalError> {
        let level = match token_type {
            1 => None,
            2 if level <= 3 => Some(level),
            _ => return Err(NativePrincipalError::InvalidObservation),
        };
        Ok(Self {
            token,
            authentication,
            modified,
            token_type,
            level,
        })
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Facts {
    pub(super) pid: u32,
    pub(super) origin: Origin,
    pub(super) sid: Sid,
    pub(super) statistics: Statistics,
}

pub(super) fn stable_statistics(
    first: &Statistics,
    second: &Statistics,
    third: &Statistics,
    origin: Origin,
    reopened_origin: Origin,
    pid: u32,
    reopened_pid: u32,
) -> Result<(), NativePrincipalError> {
    unchanged(first, second)?;
    unchanged(first, third)?;
    unchanged(&origin, &reopened_origin)?;
    unchanged(&pid, &reopened_pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stats(value: u32) -> Statistics {
        Statistics::normalize([value, 0], [2, 0], [3, 0], 1, 0).unwrap()
    }
    #[test]
    fn selection_falls_back_only_for_no_token() {
        assert!(matches!(select_origin(None), Ok(Origin::Thread)));
        assert!(matches!(select_origin(Some(1008)), Ok(Origin::Process)));
        for error in [5, 1347, 0, 6, 1314, u32::MAX] {
            assert!(matches!(
                select_origin(Some(error)),
                Err(NativePrincipalError::Unavailable)
            ));
        }
    }
    #[test]
    fn invalid_or_unavailable_sdk_sid_status_denies() {
        assert_eq!(valid_sid(Ok(true)), Ok(()));
        assert_eq!(
            valid_sid(Ok(false)),
            Err(NativePrincipalError::InvalidObservation)
        );
        assert_eq!(
            valid_sid(Err(())),
            Err(NativePrincipalError::InvalidObservation)
        );
    }
    #[test]
    fn full_original_facts_detect_sid_changes() {
        let facts = |sid_value| Facts {
            pid: 7,
            origin: Origin::Process,
            sid: normalize_sid(1, [0; 6], 1, 12, [sid_value].into_iter()).unwrap(),
            statistics: stats(1),
        };
        assert_eq!(unchanged(&facts(1), &facts(1)), Ok(()));
        assert_eq!(
            unchanged(&facts(1), &facts(2)),
            Err(NativePrincipalError::Changed)
        );
    }
    #[test]
    fn sid_fixed_shape_and_zero_suffix() {
        let Sid(bytes) = normalize_sid(1, [0, 0, 0, 0, 0, 5], 2, 16, [21, 42].into_iter()).unwrap();
        assert_eq!(&bytes[8..16], &[21, 0, 0, 0, 42, 0, 0, 0]);
        assert!(bytes[16..].iter().all(|b| *b == 0));
        assert!(normalize_sid(1, [0; 6], 15, 68, [0; 15].into_iter()).is_ok());
        assert!(normalize_sid(1, [0; 6], 0, 8, [].into_iter()).is_ok());
    }
    #[test]
    fn sid_denies_revision_count_length_and_iterator_mismatch() {
        for (revision, count, length, values) in [
            (0, 1, 12, vec![1]),
            (2, 1, 12, vec![1]),
            (1, 16, 72, vec![0; 16]),
            (1, usize::MAX, 8, vec![]),
            (1, 1, 11, vec![1]),
            (1, 1, 69, vec![1]),
            (1, 2, 16, vec![1]),
            (1, 0, 8, vec![1]),
        ] {
            assert!(normalize_sid(revision, [0; 6], count, length, values.into_iter()).is_err());
        }
    }
    #[test]
    fn statistics_normalize_applicable_level_and_deny_bad_type() {
        assert!(Statistics::normalize([0; 2], [0; 2], [0; 2], 1, u32::MAX).is_ok());
        for level in 0..=3 {
            assert!(Statistics::normalize([0; 2], [0; 2], [0; 2], 2, level).is_ok());
        }
        assert!(Statistics::normalize([0; 2], [0; 2], [0; 2], 2, 4).is_err());
        assert!(Statistics::normalize([0; 2], [0; 2], [0; 2], 0, 0).is_err());
    }
    #[test]
    fn statistics_a_b_c_origin_and_process_changes_deny() {
        let a = stats(1);
        let changed = stats(4);
        assert_eq!(
            stable_statistics(&a, &a, &a, Origin::Process, Origin::Process, 7, 7),
            Ok(())
        );
        for (b, c, o, p) in [
            (&changed, &a, Origin::Process, 7),
            (&a, &changed, Origin::Process, 7),
            (&a, &a, Origin::Thread, 7),
            (&a, &a, Origin::Process, 8),
        ] {
            assert_eq!(
                stable_statistics(&a, b, c, Origin::Process, o, 7, p),
                Err(NativePrincipalError::Changed)
            );
        }
        for field in 0..5 {
            let mut b = stats(1);
            match field {
                0 => b.authentication[0] += 1,
                1 => b.modified[0] += 1,
                2 => b.token_type = 2,
                3 => b.level = Some(1),
                _ => b.token[1] += 1,
            };
            assert_eq!(
                stable_statistics(&a, &b, &a, Origin::Thread, Origin::Thread, 7, 7),
                Err(NativePrincipalError::Changed)
            );
        }
    }
}
