use super::NativePrincipalError;
use super::windows_facts::{
    Facts, Origin, Statistics, normalize_sid, select_origin, stable_statistics, valid_sid,
};
use winsafe::{self as w, co};

fn effective_token()
-> Result<(Origin, w::guard::CloseHandleGuard<w::HACCESSTOKEN>), NativePrincipalError> {
    match w::HTHREAD::GetCurrentThread().OpenThreadToken(co::TOKEN::QUERY, true) {
        Ok(token) => Ok((select_origin(None)?, token)),
        Err(error) => {
            let origin = select_origin(Some(error.raw()))?;
            let token = w::HPROCESS::GetCurrentProcess()
                .OpenProcessToken(co::TOKEN::QUERY)
                .map_err(|_| NativePrincipalError::Unavailable)?;
            Ok((origin, token))
        }
    }
}

fn statistics(token: &w::HACCESSTOKEN) -> Result<Statistics, NativePrincipalError> {
    let info = token
        .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::Statistics)
        .map_err(|_| NativePrincipalError::Unavailable)?;
    let w::TokenInfo::Statistics(stats) = info else {
        return Err(NativePrincipalError::InvalidObservation);
    };
    let luid = |value: w::LUID| [value.low_part(), value.high_part() as u32];
    Statistics::normalize(
        luid(stats.TokenId),
        luid(stats.AuthenticationId),
        luid(stats.ModifiedId),
        stats.TokenType.raw(),
        stats.ImpersonationLevel.raw(),
    )
}

pub(super) fn capture() -> Result<Facts, NativePrincipalError> {
    let pid = w::GetCurrentProcessId();
    let (origin, token) = effective_token()?;
    let first = statistics(&token)?;
    let sid = {
        let info = token
            .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::User)
            .map_err(|_| NativePrincipalError::Unavailable)?;
        let w::TokenInfo::User(user) = info else {
            return Err(NativePrincipalError::InvalidObservation);
        };
        let sid = user
            .User
            .Sid()
            .ok_or(NativePrincipalError::InvalidObservation)?;
        valid_sid(w::IsValidSid(sid).map_err(|_| ()))?;
        normalize_sid(
            sid.Revision,
            sid.IdentifierAuthority.Value,
            usize::from(sid.SubAuthorityCount()),
            w::GetLengthSid(sid),
            sid.SubAuthority().iter().map(|rid| rid.raw()),
        )?
    };
    let second = statistics(&token)?;
    let (reopened_origin, reopened) = effective_token()?;
    let third = statistics(&reopened)?;
    stable_statistics(
        &first,
        &second,
        &third,
        origin,
        reopened_origin,
        pid,
        w::GetCurrentProcessId(),
    )?;
    Ok(Facts {
        pid,
        origin,
        sid,
        statistics: first,
    })
}
