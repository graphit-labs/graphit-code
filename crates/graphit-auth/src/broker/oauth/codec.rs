use super::{BrokerRequestContext, NativeError, NativeErrorKind, OAuthLimits};
use secrecy::zeroize::Zeroizing;
use serde::de::{self, DeserializeSeed, MapAccess, Visitor};
use std::fmt;

pub(super) struct TokenResponse {
    pub access: Zeroizing<String>,
    pub refresh: Option<Zeroizing<String>>,
    pub id: Option<Zeroizing<String>>,
    pub scope: Option<Zeroizing<String>>,
    pub expires_in: u64,
}
struct Budget<'a> {
    limits: OAuthLimits,
    context: BrokerRequestContext<'a>,
    nodes: usize,
}
impl Budget<'_> {
    fn charge<E: de::Error>(&mut self) -> Result<(), E> {
        self.context.check().map_err(|_| E::custom("context"))?;
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| E::custom("nodes"))?;
        if self.nodes > self.limits.response_nodes {
            return Err(E::custom("nodes"));
        }
        Ok(())
    }
    fn scalar<E: de::Error>(&self, value: &str, token: bool) -> Result<(), E> {
        self.context.check().map_err(|_| E::custom("context"))?;
        if value.len() > self.limits.response_scalar_bytes
            || (token && value.len() > self.limits.token_bytes)
        {
            return Err(E::custom("scalar"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Field {
    Access,
    Refresh,
    Id,
    Type,
    Expires,
    Scope,
}
impl Field {
    fn bit(self) -> u8 {
        match self {
            Self::Access => 1,
            Self::Refresh => 2,
            Self::Id => 4,
            Self::Type => 8,
            Self::Expires => 16,
            Self::Scope => 32,
        }
    }
}
struct Key<'a, 'c>(&'a mut Budget<'c>);
impl<'de> DeserializeSeed<'de> for Key<'_, '_> {
    type Value = Field;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Field, D::Error> {
        deserializer.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for Key<'_, '_> {
    type Value = Field;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("token field")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
        self.0.charge()?;
        self.0.scalar(value, false)?;
        match value {
            "access_token" => Ok(Field::Access),
            "refresh_token" => Ok(Field::Refresh),
            "id_token" => Ok(Field::Id),
            "token_type" => Ok(Field::Type),
            "expires_in" => Ok(Field::Expires),
            "scope" => Ok(Field::Scope),
            _ => Err(E::custom("unknown field")),
        }
    }
}
struct Scalar<'a, 'c> {
    budget: &'a mut Budget<'c>,
    token: bool,
    bearer: bool,
}
impl<'de> DeserializeSeed<'de> for Scalar<'_, '_> {
    type Value = Zeroizing<String>;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        self.budget.charge()?;
        deserializer.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for Scalar<'_, '_> {
    type Value = Zeroizing<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("token scalar")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.budget.scalar(value, self.token)?;
        if value.is_empty() || (self.bearer && !value.as_bytes().eq_ignore_ascii_case(b"Bearer")) {
            return Err(E::custom("invalid scalar"));
        }
        Ok(Zeroizing::new(value.to_owned()))
    }
}
struct Lifetime<'a, 'c>(&'a mut Budget<'c>);
impl<'de> DeserializeSeed<'de> for Lifetime<'_, '_> {
    type Value = u64;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<u64, D::Error> {
        self.0.charge()?;
        deserializer.deserialize_u64(self)
    }
}
impl<'de> Visitor<'de> for Lifetime<'_, '_> {
    type Value = u64;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("positive integer lifetime")
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
        self.0.context.check().map_err(|_| E::custom("context"))?;
        if value == 0 {
            Err(E::custom("zero lifetime"))
        } else {
            Ok(value)
        }
    }
}
struct Response<'a, 'c>(&'a mut Budget<'c>);
impl<'de> DeserializeSeed<'de> for Response<'_, '_> {
    type Value = TokenResponse;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<TokenResponse, D::Error> {
        self.0.charge()?;
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Response<'_, '_> {
    type Value = TokenResponse;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("closed token response")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<TokenResponse, M::Error> {
        let mut seen = 0u8;
        let (mut access, mut refresh, mut id, mut scope, mut expires) =
            (None, None, None, None, None);
        while let Some(field) = map.next_key_seed(Key(self.0))? {
            let bit = field.bit();
            if seen & bit != 0 {
                return Err(de::Error::custom("duplicate field"));
            }
            seen |= bit;
            match field {
                Field::Expires => {
                    expires = Some(map.next_value_seed(Lifetime(self.0))?);
                }
                Field::Access | Field::Refresh | Field::Id => {
                    let value = map.next_value_seed(Scalar {
                        budget: self.0,
                        token: true,
                        bearer: false,
                    })?;
                    match field {
                        Field::Access => access = Some(value),
                        Field::Refresh => refresh = Some(value),
                        Field::Id => id = Some(value),
                        _ => return Err(de::Error::custom("invalid field")),
                    }
                }
                Field::Scope => {
                    scope = Some(map.next_value_seed(Scalar {
                        budget: self.0,
                        token: false,
                        bearer: false,
                    })?);
                }
                Field::Type => {
                    let _ = map.next_value_seed(Scalar {
                        budget: self.0,
                        token: false,
                        bearer: true,
                    })?;
                }
            }
        }
        if seen & (1 | 8 | 16) != 1 | 8 | 16 {
            return Err(de::Error::custom("missing field"));
        }
        Ok(TokenResponse {
            access: access.ok_or_else(|| de::Error::custom("missing access"))?,
            refresh,
            id,
            scope,
            expires_in: expires.ok_or_else(|| de::Error::custom("missing lifetime"))?,
        })
    }
}
pub(super) fn parse(
    bytes: &[u8],
    limits: OAuthLimits,
    context: BrokerRequestContext<'_>,
) -> Result<TokenResponse, NativeError> {
    context.check()?;
    let mut budget = Budget {
        limits,
        context,
        nodes: 0,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = Response(&mut budget).deserialize(&mut deserializer);
    context.check()?;
    let result = result.map_err(|_| NativeError::local(NativeErrorKind::InvalidResponse))?;
    deserializer
        .end()
        .map_err(|_| NativeError::local(NativeErrorKind::InvalidResponse))?;
    context.check()?;
    Ok(result)
}
