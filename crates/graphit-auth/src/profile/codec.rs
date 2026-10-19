use super::*;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{
    borrow::Cow,
    cell::Cell,
    io::{self, Write},
    time::Instant,
};

pub(super) fn check(
    limits: ProfileLimits,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    limits.validate()?;
    if context.cancellation().is_cancelled() {
        Err(ProfileError::Cancelled)
    } else if context
        .deadline()
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        Err(ProfileError::Deadline)
    } else {
        Ok(())
    }
}
struct State<'a, 'c> {
    limits: ProfileLimits,
    context: &'a RequestContext<'c>,
    nodes: Cell<usize>,
    failure: Cell<Option<ProfileError>>,
}
impl<'a, 'c> State<'a, 'c> {
    fn new(limits: ProfileLimits, context: &'a RequestContext<'c>) -> Result<Self, ProfileError> {
        check(limits, context)?;
        Ok(Self {
            limits,
            context,
            nodes: Cell::new(0),
            failure: Cell::new(None),
        })
    }
    fn check(&self) -> Result<(), ProfileError> {
        check(self.limits, self.context)
    }
    fn node(&self) -> Result<(), ProfileError> {
        self.check()?;
        let n = self
            .nodes
            .get()
            .checked_add(1)
            .filter(|n| *n <= self.limits.nodes)
            .ok_or(ProfileError::Limit)?;
        self.nodes.set(n);
        Ok(())
    }
    fn scalar(&self, value: &str) -> Result<(), ProfileError> {
        self.check()?;
        if value.len() > self.limits.scalar_bytes {
            Err(ProfileError::Limit)
        } else {
            Ok(())
        }
    }
    fn collection(&self, length: usize, cap: usize) -> Result<(), ProfileError> {
        self.check()?;
        if length > cap.min(self.limits.collection_entries) {
            Err(ProfileError::Limit)
        } else {
            Ok(())
        }
    }
    fn error<E: de::Error>(&self, error: ProfileError) -> E {
        if self.failure.get().is_none() {
            self.failure.set(Some(error));
        }
        E::custom("auth profile document rejected")
    }
    fn result<T, E: de::Error>(&self, result: Result<T, ProfileError>) -> Result<T, E> {
        result.map_err(|e| self.error(e))
    }
    fn invalid<E: de::Error>(&self) -> E {
        self.error(ProfileError::InvalidDocument)
    }
}
struct Scalar<'s, 'a, 'c, T> {
    state: &'s State<'a, 'c>,
    convert: fn(&str) -> Result<T, ProfileError>,
}
impl<T> Copy for Scalar<'_, '_, '_, T> {}
impl<T> Clone for Scalar<'_, '_, '_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'de, T> DeserializeSeed<'de> for Scalar<'_, '_, '_, T> {
    type Value = T;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        self.state.result(self.state.node())?;
        d.deserialize_str(self)
    }
}
impl<T> Visitor<'_> for Scalar<'_, '_, '_, T> {
    type Value = T;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded auth profile string")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<T, E> {
        self.state.result(self.state.scalar(v))?;
        self.state.result((self.convert)(v))
    }
}
fn text(v: &str) -> Result<String, ProfileError> {
    valid_text(v)?;
    Ok(v.to_owned())
}
fn parse_name(v: &str) -> Result<String, ProfileError> {
    valid_name(v)?;
    Ok(v.to_owned())
}
fn secret(v: &str) -> Result<Secret, ProfileError> {
    valid_text(v)?;
    Ok(Secret::new(SecretString::from(v)))
}
fn literal(v: &str) -> Result<String, ProfileError> {
    Ok(v.to_owned())
}
fn string<'s, 'a, 'c, T>(
    s: &'s State<'a, 'c>,
    convert: fn(&str) -> Result<T, ProfileError>,
) -> Scalar<'s, 'a, 'c, T> {
    Scalar { state: s, convert }
}
struct Key<'s, 'a, 'c>(&'s State<'a, 'c>);
impl<'de> DeserializeSeed<'de> for Key<'_, '_, '_> {
    type Value = Cow<'de, str>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.result(self.0.node())?;
        d.deserialize_str(self)
    }
}
impl<'de> Visitor<'de> for Key<'_, '_, '_> {
    type Value = Cow<'de, str>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an auth profile field")
    }
    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        self.0.result(self.0.scalar(v))?;
        Ok(Cow::Borrowed(v))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.0.result(self.0.scalar(v))?;
        Ok(Cow::Owned(v.to_owned()))
    }
}
#[derive(Clone, Copy)]
struct Unsigned<'s, 'a, 'c>(&'s State<'a, 'c>);
impl<'de> DeserializeSeed<'de> for Unsigned<'_, '_, '_> {
    type Value = u64;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<u64, D::Error> {
        self.0.result(self.0.node())?;
        d.deserialize_u64(self)
    }
}
impl Visitor<'_> for Unsigned<'_, '_, '_> {
    type Value = u64;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an unsigned integer")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
        self.0.result(self.0.check())?;
        Ok(v)
    }
}
#[derive(Clone, Copy)]
struct Timestamp<'s, 'a, 'c>(&'s State<'a, 'c>);
impl<'de> DeserializeSeed<'de> for Timestamp<'_, '_, '_> {
    type Value = i64;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<i64, D::Error> {
        self.0.result(self.0.node())?;
        d.deserialize_i64(self)
    }
}
impl Visitor<'_> for Timestamp<'_, '_, '_> {
    type Value = i64;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("nonnegative UTC microseconds")
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
        self.0.result(valid_time(v))?;
        Ok(v)
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
        i64::try_from(v).map_err(|_| self.0.invalid())
    }
}
#[derive(Clone, Copy)]
struct Boolean<'s, 'a, 'c>(&'s State<'a, 'c>);
impl<'de> DeserializeSeed<'de> for Boolean<'_, '_, '_> {
    type Value = bool;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
        self.0.result(self.0.node())?;
        d.deserialize_bool(self)
    }
}
impl Visitor<'_> for Boolean<'_, '_, '_> {
    type Value = bool;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a boolean")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
        self.0.result(self.0.check())?;
        Ok(v)
    }
}
struct Refuse<'s, 'a, 'c>(&'s State<'a, 'c>);
impl<'de> DeserializeSeed<'de> for Refuse<'_, '_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(self.0.error(ProfileError::Limit))
    }
}
struct List<'s, 'a, 'c, S> {
    state: &'s State<'a, 'c>,
    item: S,
    cap: usize,
}
impl<S: Copy> Copy for List<'_, '_, '_, S> {}
impl<S: Copy> Clone for List<'_, '_, '_, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'de, S: DeserializeSeed<'de> + Copy> DeserializeSeed<'de> for List<'_, '_, '_, S> {
    type Value = Vec<S::Value>;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.state.result(self.state.node())?;
        d.deserialize_seq(self)
    }
}
impl<'de, S: DeserializeSeed<'de> + Copy> Visitor<'de> for List<'_, '_, '_, S> {
    type Value = Vec<S::Value>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded auth profile array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let cap = self.cap.min(self.state.limits.collection_entries);
        let mut values = Vec::new();
        loop {
            self.state.result(self.state.check())?;
            if values.len() == cap {
                a.next_element_seed(Refuse(self.state))?;
                break;
            }
            let Some(value) = a.next_element_seed(self.item)? else {
                break;
            };
            self.state.result(reserve(&mut values, cap))?;
            values.push(value);
        }
        Ok(values)
    }
}
macro_rules! req {
    ($state:expr, $v:expr) => {
        $v.ok_or_else(|| $state.invalid())?
    };
}
macro_rules! object {
    ($seed:ident, $output:ty, $s:ident, $($field:ident => $parser:expr),+ ; $build:expr) => {
        #[derive(Clone, Copy)] struct $seed<'s, 'a, 'c>(&'s State<'a, 'c>);
        impl<'de> DeserializeSeed<'de> for $seed<'_, '_, '_> {
            type Value = $output;
            fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<$output, D::Error> { self.0.result(self.0.node())?; d.deserialize_map(self) }
        }
        impl<'de> Visitor<'de> for $seed<'_, '_, '_> {
            type Value = $output;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("a typed auth profile object") }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<$output, A::Error> {
                let $s = self.0; $(let mut $field = None;)+ let mut fields = 0;
                loop {
                    $s.result($s.check())?;
                    let key = if fields == $s.limits.collection_entries { a.next_key_seed(Refuse($s))?.map(|_| Cow::Borrowed("")) } else { a.next_key_seed(Key($s))? };
                    let Some(key) = key else { break; }; fields += 1;
                    match key.as_ref() { $(stringify!($field) => { if $field.is_some() { return Err($s.invalid()); } $field = Some(a.next_value_seed($parser)?); },)+ _ => return Err($s.invalid()), }
                }
                $build
            }
        }
    }
}
object!(ExecutionSeed, LocalExecution, s, device => string(s, literal), device_id => Unsigned(s); {
    let device = match req!(s, device).as_str() { "auto" => LocalDevice::Auto, "cpu" => LocalDevice::Cpu, "cuda" => LocalDevice::Cuda, "metal" => LocalDevice::Metal, _ => return Err(s.invalid()) };
    Ok(LocalExecution { device, device_id: u32::try_from(req!(s, device_id)).map_err(|_| s.invalid())? })
});
object!(AiSeed, AiService, s, mode => string(s, literal), execution => ExecutionSeed(s), protocol => string(s, literal), endpoint => string(s, text), model => string(s, text), model_revision => string(s, text), recipe => string(s, text), dimensions => Unsigned(s), request_dimensions => Unsigned(s), batch_endpoint => string(s, text); {
    let mode = req!(s, mode);
    let direct_fields = protocol.is_some() || endpoint.is_some() || model.is_some() || model_revision.is_some() || recipe.is_some() || dimensions.is_some() || request_dimensions.is_some() || batch_endpoint.is_some();
    match mode.as_str() {
        "local" if !direct_fields => Ok(AiService::Local(req!(s, execution))),
        "disabled" if !direct_fields && execution.is_none() => Ok(AiService::Disabled),
        "broker" if !direct_fields && execution.is_none() => Ok(AiService::Broker),
        "direct" if execution.is_none() => Ok(AiService::Direct { protocol: s.result(parse_protocol(&req!(s, protocol)))?, endpoint: req!(s, endpoint), model: req!(s, model), model_revision: req!(s, model_revision), recipe: req!(s, recipe), dimensions: u32::try_from(req!(s, dimensions)).map_err(|_| s.invalid())?, request_dimensions: request_dimensions.map(u32::try_from).transpose().map_err(|_| s.invalid())?, batch_endpoint }),
        _ => Err(s.invalid()),
    }
});
fn parse_protocol(v: &str) -> Result<DirectProtocol, ProfileError> {
    Ok(match v {
        "openai" => DirectProtocol::OpenAi,
        "openai-compatible" => DirectProtocol::OpenAiCompatible,
        "cohere" => DirectProtocol::Cohere,
        "cohere-v2" => DirectProtocol::CohereV2,
        "voyage" => DirectProtocol::Voyage,
        "voyage-v1" => DirectProtocol::VoyageV1,
        "google-gemini-1" => DirectProtocol::GoogleGemini1,
        "google-gemini-2" => DirectProtocol::GoogleGemini2,
        "jina" => DirectProtocol::Jina,
        "jina-v1" => DirectProtocol::JinaV1,
        _ => return Err(ProfileError::InvalidDocument),
    })
}
fn protocol(v: DirectProtocol) -> &'static str {
    match v {
        DirectProtocol::OpenAi => "openai",
        DirectProtocol::OpenAiCompatible => "openai-compatible",
        DirectProtocol::Cohere => "cohere",
        DirectProtocol::CohereV2 => "cohere-v2",
        DirectProtocol::Voyage => "voyage",
        DirectProtocol::VoyageV1 => "voyage-v1",
        DirectProtocol::GoogleGemini1 => "google-gemini-1",
        DirectProtocol::GoogleGemini2 => "google-gemini-2",
        DirectProtocol::Jina => "jina",
        DirectProtocol::JinaV1 => "jina-v1",
    }
}
object!(BrokerSeed, BrokerService, s, endpoint => string(s, text), mcp_resource => string(s, text), allow_anonymous => Boolean(s); Ok(BrokerService { endpoint: req!(s, endpoint), mcp_resource, allow_anonymous: req!(s, allow_anonymous) }));
object!(KindSeed, ProviderKind, s, kind => string(s, literal), allow_aws_credential_chain => Boolean(s), broker => BrokerSeed(s); {
    match req!(s, kind).as_str() {
        "local" => Ok(ProviderKind::Local { allow_aws_credential_chain: req!(s, allow_aws_credential_chain), broker }),
        "broker" if allow_aws_credential_chain.is_none() => Ok(ProviderKind::Broker(req!(s, broker))), _ => Err(s.invalid()),
    }
});
object!(S3Seed, S3Location, s, bucket => string(s, text), region => string(s, text), endpoint => string(s, text), prefix => string(s, text), source => string(s, literal); {
    let source = match req!(s, source).as_str() { "local-credentials" => S3Source::LocalCredentials, "aws-chain" => S3Source::AwsChain, "broker-scope" => S3Source::BrokerScope, _ => return Err(s.invalid()) };
    Ok(S3Location { bucket, region, endpoint, prefix, source })
});
object!(ConfigSeed, ProviderConfig, s, kind => KindSeed(s), embedding => AiSeed(s), rerank => AiSeed(s), s3 => S3Seed(s); {
    let config = ProviderConfig { kind: req!(s, kind), embedding: req!(s, embedding), rerank: req!(s, rerank), s3: req!(s, s3) };
    s.result(validate_config(&config))?; Ok(config)
});
object!(ProviderSeed, ProviderRecord, s, name => string(s, parse_name), revision => Unsigned(s), created_at => Timestamp(s), updated_at => Timestamp(s), config => ConfigSeed(s); {
    let record = ProviderRecord { name: req!(s, name), revision: req!(s, revision), created_at: req!(s, created_at), updated_at: req!(s, updated_at), config: req!(s, config) };
    if record.revision == 0 || record.updated_at < record.created_at { return Err(s.invalid()); } Ok(record)
});
object!(LocalS3Seed, LocalS3, s, access_key_id => string(s, secret), secret_access_key => string(s, secret), session_token => string(s, secret), expires_at => Timestamp(s), aws_profile => string(s, text); Ok(LocalS3 { access_key_id, secret_access_key, session_token, expires_at, aws_profile }));
object!(LocalSeed, LocalCredentials, s, anonymous => Boolean(s), mcp_key => string(s, secret), broker_key => string(s, secret), embedding_api_key => string(s, secret), rerank_api_key => string(s, secret), s3 => LocalS3Seed(s); Ok(LocalCredentials { anonymous: req!(s, anonymous), mcp_key, broker_key, embedding_api_key, rerank_api_key, s3 }));
struct StoredTokens {
    access_token: Secret,
    refresh_token: Option<Secret>,
    id_token: Option<Secret>,
    expires_at: i64,
}
enum StoredCredentials {
    Named(ProfileCredentials),
    BrokerDevice {
        issuer: String,
        subject: String,
        tokens: BrokerDeviceTokens,
        s3_disabled: bool,
    },
}
object!(TokensSeed, StoredTokens, s, access_token => string(s, secret), refresh_token => string(s, secret), id_token => string(s, secret), token_type => string(s, literal), expires_at => Timestamp(s); {
    if req!(s, token_type) != "Bearer" { return Err(s.invalid()); }
    Ok(StoredTokens { access_token: req!(s, access_token), refresh_token, id_token, expires_at: req!(s, expires_at) })
});
object!(CredentialsSeed, StoredCredentials, s, kind => string(s, literal), local => LocalSeed(s), issuer => string(s, text), subject => string(s, text), tokens => TokensSeed(s), s3_disabled => Boolean(s); {
    match req!(s, kind).as_str() {
        "local" if issuer.is_none() && subject.is_none() && tokens.is_none() && s3_disabled.is_none() => Ok(StoredCredentials::Named(ProfileCredentials::Local(req!(s, local)))),
        "broker" if local.is_none() => {
            let tokens = req!(s, tokens);
            let id_token = tokens.id_token.ok_or_else(|| s.invalid())?;
            Ok(StoredCredentials::Named(ProfileCredentials::Broker {
                issuer: req!(s, issuer), subject: req!(s, subject),
                tokens: OidcTokens { access_token: tokens.access_token, refresh_token: tokens.refresh_token, id_token, expires_at: tokens.expires_at },
                s3_disabled: req!(s, s3_disabled),
            }))
        }
        "broker_device" if local.is_none() => {
            let tokens = req!(s, tokens);
            if tokens.id_token.is_some() || tokens.refresh_token.is_some() { return Err(s.invalid()); }
            Ok(StoredCredentials::BrokerDevice {
                issuer: req!(s, issuer), subject: req!(s, subject),
                tokens: BrokerDeviceTokens { access_token: tokens.access_token, expires_at: tokens.expires_at },
                s3_disabled: req!(s, s3_disabled),
            })
        }
        _ => Err(s.invalid()),
    }
});
object!(DataSeed, ProfileBody, s, name => string(s, parse_name), provider => string(s, parse_name), username => string(s, text), organization => string(s, text), teams => List { state: s, item: string(s, text), cap: s.limits.collection_entries }, credentials => CredentialsSeed(s); {
    let name = req!(s, name);
    let provider = req!(s, provider);
    let data = match req!(s, credentials) {
        StoredCredentials::Named(credentials) => ProfileBody::Named(ProfileData {
            name, provider, username: req!(s, username), organization, teams: req!(s, teams), credentials,
        }),
        StoredCredentials::BrokerDevice { issuer, subject, tokens, s3_disabled } => ProfileBody::BrokerDevice(BrokerDeviceProfileData {
            name, provider, issuer, subject, tokens, s3_disabled,
            metadata: BrokerDeviceMetadata { username, organization, teams },
        }),
    };
    s.result(validate_body(&data, None))?;
    Ok(data)
});
object!(ProfileSeed, ProfileRecord, s, revision => Unsigned(s), token_revision => Unsigned(s), provider_revision => Unsigned(s), created_at => Timestamp(s), updated_at => Timestamp(s), data => DataSeed(s); {
    let p = ProfileRecord { revision: req!(s, revision), token_revision: req!(s, token_revision), provider_revision: req!(s, provider_revision), created_at: req!(s, created_at), updated_at: req!(s, updated_at), data: req!(s, data) };
    if p.revision == 0 || p.token_revision == 0 || p.provider_revision == 0 || p.updated_at < p.created_at { return Err(s.invalid()); } Ok(p)
});
fn web_hub_scope(value: &str) -> Result<WebHubScope, ProfileError> {
    match value {
        "project" => Ok(WebHubScope::Project),
        "global" => Ok(WebHubScope::Global),
        _ => Err(ProfileError::InvalidDocument),
    }
}
object!(WebHubAdministratorSeed, WebHubAdministrator, s, provider => string(s, parse_name), issuer => string(s, text), subject => string(s, text), scopes => List { state: s, item: string(s, web_hub_scope), cap: 2 }; {
    let entry = WebHubAdministrator { provider: req!(s, provider), issuer: req!(s, issuer), subject: req!(s, subject), scopes: req!(s, scopes) };
    s.result(valid_url(&entry.issuer, false))?;
    if entry.scopes.is_empty() || (entry.scopes.len() == 2 && entry.scopes[0] == entry.scopes[1]) { return Err(s.invalid()); }
    Ok(entry)
});
object!(DocumentSeed, AuthDocument, s, format => string(s, literal), epoch => Unsigned(s), document_revision => Unsigned(s), active_profile => string(s, parse_name), providers => List { state: s, item: ProviderSeed(s), cap: s.limits.providers }, profiles => List { state: s, item: ProfileSeed(s), cap: s.limits.profiles }, web_hub_administrators => List { state: s, item: WebHubAdministratorSeed(s), cap: s.limits.collection_entries }; {
    if req!(s, format) != FORMAT || req!(s, epoch) != EPOCH { return Err(s.invalid()); }
    let revision = req!(s, document_revision); if revision == 0 { return Err(s.invalid()); }
    let mut providers = req!(s, providers); let mut profiles = req!(s, profiles);
    providers.sort_unstable_by(|a, b| a.name.cmp(&b.name)); profiles.sort_unstable_by(|a, b| a.name().cmp(b.name()));
    if providers.windows(2).any(|v| v[0].name == v[1].name) || profiles.windows(2).any(|v| v[0].name() == v[1].name()) { return Err(s.invalid()); }
    let web_hub_administrators = web_hub_administrators.unwrap_or_default();
    let mut principals = std::collections::BTreeSet::new();
    for entry in &web_hub_administrators {
        s.result(s.check())?;
        if !principals.insert((&entry.provider, &entry.issuer, &entry.subject)) { return Err(s.invalid()); }
    }
    drop(principals);
    let document = AuthDocument { limits: s.limits, revision, active_profile, providers, profiles, web_hub_administrators };
    for provider in &document.providers {
        s.result(s.check())?;
        s.result(check_revision(provider.revision, revision))?;
    }
    for p in &document.profiles {
        s.result(s.check())?;
        for own in [p.revision, p.token_revision, p.provider_revision] {
            s.result(check_revision(own, revision))?;
        }
        if let Some(provider) = document.provider(p.provider()).filter(|v| v.revision == p.provider_revision) { s.result(validate_body(&p.data, Some(&provider.config)))?; }
    }
    Ok(document)
});

pub(super) fn decode(
    bytes: &[u8],
    limits: ProfileLimits,
    context: &RequestContext<'_>,
) -> Result<AuthDocument, ProfileError> {
    let state = State::new(limits, context)?;
    if bytes.len() > limits.bytes {
        return Err(ProfileError::Limit);
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let document = DocumentSeed(&state)
        .deserialize(&mut decoder)
        .map_err(|_| state.failure.get().unwrap_or(ProfileError::InvalidDocument))?;
    decoder.end().map_err(|_| ProfileError::InvalidDocument)?;
    state.check()?;
    Ok(document)
}

// The same traversal writes bytes or counts the prospective document before mutation.
// Other is deliberate: Interrupted would make serde_json retry an unwritable sink forever.
struct Writer<'s, 'a, 'c> {
    state: &'s State<'a, 'c>,
    bytes: usize,
    output: Option<Zeroizing<Vec<u8>>>,
}
impl Write for Writer<'_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            self.state.check()?;
            let next = self
                .bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= self.state.limits.bytes)
                .ok_or(ProfileError::Limit)?;
            if let Some(output) = &mut self.output {
                if next > output.capacity() {
                    return Err(ProfileError::Limit);
                }
                output.extend_from_slice(bytes);
            }
            self.bytes = next;
            Ok(())
        })();
        if let Err(error) = result {
            self.state.failure.set(Some(error));
            return Err(io::Error::other("auth profile output rejected"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.state.check().map_err(|e| {
            self.state.failure.set(Some(e));
            io::Error::other("auth profile output rejected")
        })
    }
}
impl Writer<'_, '_, '_> {
    fn raw(&mut self, value: &[u8]) -> Result<(), ProfileError> {
        self.write_all(value).map_err(|_| {
            self.state
                .failure
                .get()
                .unwrap_or(ProfileError::InvalidDocument)
        })
    }
    fn value<T: serde::Serialize + ?Sized>(&mut self, v: &T) -> Result<(), ProfileError> {
        serde_json::to_writer(&mut *self, v).map_err(|_| {
            self.state
                .failure
                .get()
                .unwrap_or(ProfileError::InvalidDocument)
        })
    }
    fn object(&mut self, fields: usize) -> Result<(), ProfileError> {
        self.state.node()?;
        self.state
            .collection(fields, self.state.limits.collection_entries)?;
        self.raw(b"{")
    }
    fn key(&mut self, key: &str, first: &mut bool) -> Result<(), ProfileError> {
        if !*first {
            self.raw(b",")?;
        }
        *first = false;
        self.state.node()?;
        self.state.scalar(key)?;
        self.value(key)?;
        self.raw(b":")
    }
    fn text(&mut self, v: &str) -> Result<(), ProfileError> {
        self.state.node()?;
        self.state.scalar(v)?;
        self.value(v)
    }
    fn num<T: serde::Serialize>(&mut self, v: T) -> Result<(), ProfileError> {
        self.state.node()?;
        self.value(&v)
    }
    fn array(&mut self, length: usize, cap: usize) -> Result<(), ProfileError> {
        self.state.node()?;
        self.state.collection(length, cap)?;
        self.raw(b"[")
    }
}
macro_rules! key {
    ($w:expr, $first:expr, $name:expr, $call:expr) => {{
        $w.key($name, &mut $first)?;
        $call?;
    }};
}
macro_rules! optional_text {
    ($w:expr, $first:expr, $name:expr, $v:expr) => {
        if let Some(v) = $v {
            key!($w, $first, $name, $w.text(v));
        }
    };
}
fn write_execution(w: &mut Writer<'_, '_, '_>, o: &LocalExecution) -> Result<(), ProfileError> {
    w.object(2)?;
    let mut first = true;
    key!(
        w,
        first,
        "device",
        w.text(match o.device {
            LocalDevice::Auto => "auto",
            LocalDevice::Cpu => "cpu",
            LocalDevice::Cuda => "cuda",
            LocalDevice::Metal => "metal",
        })
    );
    key!(w, first, "device_id", w.num(o.device_id));
    w.raw(b"}")
}
fn write_ai(w: &mut Writer<'_, '_, '_>, a: &AiService) -> Result<(), ProfileError> {
    w.object(match a {
        AiService::Local(_) => 2,
        AiService::Direct {
            request_dimensions,
            batch_endpoint,
            ..
        } => 7 + usize::from(request_dimensions.is_some()) + usize::from(batch_endpoint.is_some()),
        _ => 1,
    })?;
    let mut first = true;
    key!(
        w,
        first,
        "mode",
        w.text(match a {
            AiService::Local(_) => "local",
            AiService::Disabled => "disabled",
            AiService::Broker => "broker",
            AiService::Direct { .. } => "direct",
        })
    );
    match a {
        AiService::Local(o) => key!(w, first, "execution", write_execution(w, o)),
        AiService::Direct {
            protocol: p,
            endpoint,
            model,
            model_revision,
            recipe,
            dimensions,
            request_dimensions,
            batch_endpoint,
        } => {
            key!(w, first, "protocol", w.text(protocol(*p)));
            key!(w, first, "endpoint", w.text(endpoint));
            key!(w, first, "model", w.text(model));
            key!(w, first, "model_revision", w.text(model_revision));
            key!(w, first, "recipe", w.text(recipe));
            key!(w, first, "dimensions", w.num(*dimensions));
            if let Some(width) = request_dimensions {
                key!(w, first, "request_dimensions", w.num(*width));
            }
            if let Some(endpoint) = batch_endpoint {
                key!(w, first, "batch_endpoint", w.text(endpoint));
            }
        }
        _ => {}
    }
    w.raw(b"}")
}
fn write_broker(w: &mut Writer<'_, '_, '_>, b: &BrokerService) -> Result<(), ProfileError> {
    w.object(2 + usize::from(b.mcp_resource.is_some()))?;
    let mut first = true;
    key!(w, first, "endpoint", w.text(&b.endpoint));
    optional_text!(w, first, "mcp_resource", &b.mcp_resource);
    key!(w, first, "allow_anonymous", w.num(b.allow_anonymous));
    w.raw(b"}")
}
fn bound_config(state: &State<'_, '_>, c: &ProviderConfig) -> Result<(), ProfileError> {
    state.check()?;
    let broker = match &c.kind {
        ProviderKind::Local { broker, .. } => broker.as_ref(),
        ProviderKind::Broker(b) => Some(b),
    };
    if let Some(b) = broker {
        state.scalar(&b.endpoint)?;
        if let Some(v) = &b.mcp_resource {
            state.scalar(v)?;
        }
    }
    for a in [&c.embedding, &c.rerank] {
        if let AiService::Direct {
            endpoint,
            model,
            model_revision,
            recipe,
            batch_endpoint,
            ..
        } = a
        {
            state.scalar(endpoint)?;
            state.scalar(model)?;
            state.scalar(model_revision)?;
            state.scalar(recipe)?;
            if let Some(endpoint) = batch_endpoint {
                state.scalar(endpoint)?;
            }
        }
    }
    for v in [&c.s3.bucket, &c.s3.region, &c.s3.endpoint, &c.s3.prefix]
        .into_iter()
        .flatten()
    {
        state.scalar(v)?;
    }
    Ok(())
}
fn bound_data(state: &State<'_, '_>, d: &ProfileData) -> Result<(), ProfileError> {
    state.scalar(&d.name)?;
    state.scalar(&d.provider)?;
    bound_profile_claims(state, &d.username, d.organization.as_deref(), &d.teams)?;
    match &d.credentials {
        ProfileCredentials::Local(c) => {
            for v in [
                &c.mcp_key,
                &c.broker_key,
                &c.embedding_api_key,
                &c.rerank_api_key,
            ]
            .into_iter()
            .flatten()
            {
                state.scalar(v.expose())?;
            }
            if let Some(s) = &c.s3 {
                for v in [&s.access_key_id, &s.secret_access_key, &s.session_token]
                    .into_iter()
                    .flatten()
                {
                    state.scalar(v.expose())?;
                }
                if let Some(v) = &s.aws_profile {
                    state.scalar(v)?;
                }
            }
        }
        ProfileCredentials::Broker {
            issuer,
            subject,
            tokens,
            ..
        } => {
            state.scalar(issuer)?;
            state.scalar(subject)?;
            state.scalar(tokens.access_token.expose())?;
            state.scalar(tokens.id_token.expose())?;
            if let Some(v) = &tokens.refresh_token {
                state.scalar(v.expose())?;
            }
        }
    }
    Ok(())
}
fn validate_body(data: &ProfileBody, config: Option<&ProviderConfig>) -> Result<(), ProfileError> {
    match data {
        ProfileBody::Named(data) => validate_data(data, config),
        ProfileBody::BrokerDevice(data) => validate_device_data(data, config),
    }
}
fn bound_device_data(
    state: &State<'_, '_>,
    data: &BrokerDeviceProfileData,
) -> Result<(), ProfileError> {
    state.check()?;
    for value in [&data.name, &data.provider, &data.issuer, &data.subject] {
        state.scalar(value)?;
    }
    state.scalar(data.tokens.access_token.expose())?;
    for value in [&data.metadata.username, &data.metadata.organization]
        .into_iter()
        .flatten()
    {
        state.scalar(value)?;
    }
    if let Some(teams) = &data.metadata.teams {
        if teams.len() > state.limits.collection_entries {
            return Err(ProfileError::Limit);
        }
        for team in teams {
            state.scalar(team)?;
        }
    }
    Ok(())
}
fn write_body(
    w: &mut Writer<'_, '_, '_>,
    data: &ProfileBody,
    update: Option<&TokenUpdate>,
) -> Result<(), ProfileError> {
    match data {
        ProfileBody::Named(data) => write_data(w, data, update),
        ProfileBody::BrokerDevice(data) => {
            if update.is_some() {
                return Err(ProfileError::InvalidDocument);
            }
            write_device_data(w, data)
        }
    }
}
fn write_device_data(
    w: &mut Writer<'_, '_, '_>,
    data: &BrokerDeviceProfileData,
) -> Result<(), ProfileError> {
    bound_device_data(w.state, data)?;
    validate_device_data(data, None)?;
    w.object(
        3 + usize::from(data.metadata.username.is_some())
            + usize::from(data.metadata.organization.is_some())
            + usize::from(data.metadata.teams.is_some()),
    )?;
    let mut first = true;
    key!(w, first, "name", w.text(&data.name));
    key!(w, first, "provider", w.text(&data.provider));
    optional_text!(w, first, "username", &data.metadata.username);
    optional_text!(w, first, "organization", &data.metadata.organization);
    if let Some(teams) = &data.metadata.teams {
        w.key("teams", &mut first)?;
        w.array(teams.len(), w.state.limits.collection_entries)?;
        for (index, team) in teams.iter().enumerate() {
            if index != 0 {
                w.raw(b",")?;
            }
            w.text(team)?;
        }
        w.raw(b"]")?;
    }
    w.key("credentials", &mut first)?;
    w.object(5)?;
    let mut credential = true;
    key!(w, credential, "kind", w.text("broker_device"));
    key!(w, credential, "issuer", w.text(&data.issuer));
    key!(w, credential, "subject", w.text(&data.subject));
    w.key("tokens", &mut credential)?;
    w.object(3)?;
    let mut token = true;
    key!(
        w,
        token,
        "access_token",
        w.text(data.tokens.access_token.expose())
    );
    key!(w, token, "token_type", w.text("Bearer"));
    key!(w, token, "expires_at", w.num(data.tokens.expires_at));
    w.raw(b"}")?;
    key!(w, credential, "s3_disabled", w.num(data.s3_disabled));
    w.raw(b"}")?;
    w.raw(b"}")
}

fn bound_profile_claims(
    state: &State<'_, '_>,
    username: &str,
    organization: Option<&str>,
    teams: &[String],
) -> Result<(), ProfileError> {
    state.scalar(username)?;
    if let Some(organization) = organization {
        state.scalar(organization)?;
    }
    state.collection(teams.len(), state.limits.collection_entries)?;
    for team in teams {
        state.scalar(team)?;
    }
    Ok(())
}
fn write_config(w: &mut Writer<'_, '_, '_>, c: &ProviderConfig) -> Result<(), ProfileError> {
    bound_config(w.state, c)?;
    validate_config(c)?;
    w.object(4)?;
    let mut first = true;
    w.key("kind", &mut first)?;
    match &c.kind {
        ProviderKind::Local {
            allow_aws_credential_chain,
            broker,
        } => {
            w.object(2 + usize::from(broker.is_some()))?;
            let mut inner = true;
            key!(w, inner, "kind", w.text("local"));
            key!(
                w,
                inner,
                "allow_aws_credential_chain",
                w.num(*allow_aws_credential_chain)
            );
            if let Some(b) = broker {
                key!(w, inner, "broker", write_broker(w, b));
            }
            w.raw(b"}")?;
        }
        ProviderKind::Broker(b) => {
            w.object(2)?;
            let mut inner = true;
            key!(w, inner, "kind", w.text("broker"));
            key!(w, inner, "broker", write_broker(w, b));
            w.raw(b"}")?;
        }
    }
    key!(w, first, "embedding", write_ai(w, &c.embedding));
    key!(w, first, "rerank", write_ai(w, &c.rerank));
    w.key("s3", &mut first)?;
    let s3 = &c.s3;
    w.object(
        1 + [&s3.bucket, &s3.region, &s3.endpoint, &s3.prefix]
            .iter()
            .filter(|v| v.is_some())
            .count(),
    )?;
    let mut inner = true;
    key!(
        w,
        inner,
        "source",
        w.text(match s3.source {
            S3Source::LocalCredentials => "local-credentials",
            S3Source::AwsChain => "aws-chain",
            S3Source::BrokerScope => "broker-scope",
        })
    );
    optional_text!(w, inner, "bucket", &s3.bucket);
    optional_text!(w, inner, "region", &s3.region);
    optional_text!(w, inner, "endpoint", &s3.endpoint);
    optional_text!(w, inner, "prefix", &s3.prefix);
    w.raw(b"}")?;
    w.raw(b"}")
}
fn check_revision(revision: u64, document_revision: u64) -> Result<(), ProfileError> {
    if revision == 0 || revision > document_revision {
        Err(ProfileError::InvalidDocument)
    } else {
        Ok(())
    }
}
fn write_provider(
    w: &mut Writer<'_, '_, '_>,
    p: &ProviderRecord,
    document_revision: u64,
) -> Result<(), ProfileError> {
    valid_name(&p.name)?;
    check_revision(p.revision, document_revision)?;
    valid_time(p.created_at)?;
    next_time(p.updated_at, p.created_at)?;
    w.object(5)?;
    let mut first = true;
    key!(w, first, "name", w.text(&p.name));
    key!(w, first, "revision", w.num(p.revision));
    key!(w, first, "created_at", w.num(p.created_at));
    key!(w, first, "updated_at", w.num(p.updated_at));
    key!(w, first, "config", write_config(w, &p.config));
    w.raw(b"}")
}
fn write_local_s3(w: &mut Writer<'_, '_, '_>, s: &LocalS3) -> Result<(), ProfileError> {
    w.object(
        usize::from(s.access_key_id.is_some())
            + usize::from(s.secret_access_key.is_some())
            + usize::from(s.session_token.is_some())
            + usize::from(s.expires_at.is_some())
            + usize::from(s.aws_profile.is_some()),
    )?;
    let mut first = true;
    optional_text!(
        w,
        first,
        "access_key_id",
        s.access_key_id.as_ref().map(Secret::expose)
    );
    optional_text!(
        w,
        first,
        "secret_access_key",
        s.secret_access_key.as_ref().map(Secret::expose)
    );
    optional_text!(
        w,
        first,
        "session_token",
        s.session_token.as_ref().map(Secret::expose)
    );
    if let Some(t) = s.expires_at {
        key!(w, first, "expires_at", w.num(t));
    }
    optional_text!(w, first, "aws_profile", &s.aws_profile);
    w.raw(b"}")
}
fn write_tokens(
    w: &mut Writer<'_, '_, '_>,
    t: &OidcTokens,
    update: Option<&TokenUpdate>,
) -> Result<(), ProfileError> {
    let access = update.map(|u| &u.access_token).unwrap_or(&t.access_token);
    let refresh = update
        .map(|u| Some(&u.refresh_token))
        .unwrap_or(t.refresh_token.as_ref());
    let id = match update.map(|u| &u.identity) {
        Some(ProfileIdentityUpdate::Replace { id_token, .. }) => id_token,
        _ => &t.id_token,
    };
    let expires = update.map(|u| u.expires_at).unwrap_or(t.expires_at);
    w.object(4 + usize::from(refresh.is_some()))?;
    let mut first = true;
    key!(w, first, "access_token", w.text(access.expose()));
    optional_text!(w, first, "refresh_token", refresh.map(Secret::expose));
    key!(w, first, "id_token", w.text(id.expose()));
    key!(w, first, "token_type", w.text("Bearer"));
    key!(w, first, "expires_at", w.num(expires));
    w.raw(b"}")
}
fn write_data(
    w: &mut Writer<'_, '_, '_>,
    d: &ProfileData,
    update: Option<&TokenUpdate>,
) -> Result<(), ProfileError> {
    bound_data(w.state, d)?;
    validate_data(d, None)?;
    let (username, organization, teams) = match update.map(|u| &u.identity) {
        Some(ProfileIdentityUpdate::Replace { claims, .. }) => (
            &claims.username,
            &claims.organization,
            claims.teams.as_slice(),
        ),
        _ => (&d.username, &d.organization, d.teams.as_slice()),
    };
    w.object(5 + usize::from(organization.is_some()))?;
    let mut first = true;
    key!(w, first, "name", w.text(&d.name));
    key!(w, first, "provider", w.text(&d.provider));
    key!(w, first, "username", w.text(username));
    optional_text!(w, first, "organization", organization);
    w.key("teams", &mut first)?;
    w.array(teams.len(), w.state.limits.collection_entries)?;
    for (i, team) in teams.iter().enumerate() {
        if i != 0 {
            w.raw(b",")?;
        }
        w.text(team)?;
    }
    w.raw(b"]")?;
    w.key("credentials", &mut first)?;
    match &d.credentials {
        ProfileCredentials::Local(c) => {
            w.object(2)?;
            let mut inner = true;
            key!(w, inner, "kind", w.text("local"));
            w.key("local", &mut inner)?;
            w.object(
                [
                    &c.mcp_key,
                    &c.broker_key,
                    &c.embedding_api_key,
                    &c.rerank_api_key,
                ]
                .iter()
                .filter(|v| v.is_some())
                .count()
                    + usize::from(c.s3.is_some())
                    + 1,
            )?;
            let mut keys = true;
            key!(w, keys, "anonymous", w.num(c.anonymous));
            optional_text!(w, keys, "mcp_key", c.mcp_key.as_ref().map(Secret::expose));
            optional_text!(
                w,
                keys,
                "broker_key",
                c.broker_key.as_ref().map(Secret::expose)
            );
            optional_text!(
                w,
                keys,
                "embedding_api_key",
                c.embedding_api_key.as_ref().map(Secret::expose)
            );
            optional_text!(
                w,
                keys,
                "rerank_api_key",
                c.rerank_api_key.as_ref().map(Secret::expose)
            );
            if let Some(s) = &c.s3 {
                key!(w, keys, "s3", write_local_s3(w, s));
            }
            w.raw(b"}")?;
            w.raw(b"}")?;
        }
        ProfileCredentials::Broker {
            issuer,
            subject,
            tokens,
            s3_disabled,
        } => {
            w.object(5)?;
            let mut inner = true;
            key!(w, inner, "kind", w.text("broker"));
            key!(w, inner, "issuer", w.text(issuer));
            key!(w, inner, "subject", w.text(subject));
            key!(w, inner, "tokens", write_tokens(w, tokens, update));
            key!(w, inner, "s3_disabled", w.num(*s3_disabled));
            w.raw(b"}")?;
        }
    }
    w.raw(b"}")
}
#[derive(Clone, Copy)]
enum ProfileProjection<'a> {
    Unchanged,
    Activation,
    Tokens(&'a TokenUpdate, i64),
}
fn write_profile(
    w: &mut Writer<'_, '_, '_>,
    p: &ProfileRecord,
    projection: ProfileProjection<'_>,
    document_revision: u64,
) -> Result<(), ProfileError> {
    let update = match projection {
        ProfileProjection::Tokens(update, now) => Some((update, now)),
        _ => None,
    };
    let revision = p
        .revision
        .checked_add(u64::from(!matches!(
            projection,
            ProfileProjection::Unchanged
        )))
        .ok_or(ProfileError::Limit)?;
    let token_revision = p
        .token_revision
        .checked_add(u64::from(update.is_some()))
        .ok_or(ProfileError::Limit)?;
    for own in [revision, token_revision, p.provider_revision] {
        check_revision(own, document_revision)?;
    }
    valid_time(p.created_at)?;
    next_time(p.updated_at, p.created_at)?;
    w.object(6)?;
    let mut first = true;
    key!(w, first, "revision", w.num(revision));
    key!(w, first, "token_revision", w.num(token_revision));
    key!(w, first, "provider_revision", w.num(p.provider_revision));
    key!(w, first, "created_at", w.num(p.created_at));
    key!(
        w,
        first,
        "updated_at",
        w.num(update.map(|u| u.1).unwrap_or(p.updated_at))
    );
    key!(
        w,
        first,
        "data",
        write_body(w, &p.data, update.map(|u| u.0))
    );
    w.raw(b"}")
}

#[derive(Clone, Copy)]
pub(super) enum Change<'a> {
    None,
    Provider(&'a ProviderRecord),
    Profile(&'a ProfileRecord),
    RemoveProvider(&'a str, bool),
    Logout(&'a str),
    Active(&'a str),
    Tokens(&'a str, &'a TokenUpdate, i64),
}
fn write_document(
    w: &mut Writer<'_, '_, '_>,
    d: &AuthDocument,
    change: Change<'_>,
) -> Result<(), ProfileError> {
    let changed = !matches!(change, Change::None);
    check_revision(d.revision, d.revision)?;
    let document_revision = d
        .revision
        .checked_add(u64::from(changed))
        .ok_or(ProfileError::Limit)?;
    let active = match change {
        Change::Active(n) => Some(n),
        Change::Logout(n) if d.active_profile.as_deref() == Some(n) => None,
        Change::RemoveProvider(n, true)
            if d.active_profile
                .as_deref()
                .and_then(|a| d.profile(a))
                .is_some_and(|p| p.provider() == n) =>
        {
            None
        }
        _ => d.active_profile.as_deref(),
    };
    w.object(
        5 + usize::from(active.is_some()) + usize::from(!d.web_hub_administrators.is_empty()),
    )?;
    let mut first = true;
    key!(w, first, "format", w.text(FORMAT));
    key!(w, first, "epoch", w.num(EPOCH));
    key!(w, first, "document_revision", w.num(document_revision));
    optional_text!(w, first, "active_profile", active);
    if !d.web_hub_administrators.is_empty() {
        w.key("web_hub_administrators", &mut first)?;
        w.array(d.web_hub_administrators.len(), d.limits.collection_entries)?;
        for (index, entry) in d.web_hub_administrators.iter().enumerate() {
            if index != 0 {
                w.raw(b",")?;
            }
            valid_name(&entry.provider)?;
            valid_url(&entry.issuer, false)?;
            valid_text(&entry.subject)?;
            w.object(4)?;
            let mut entry_first = true;
            key!(w, entry_first, "provider", w.text(&entry.provider));
            key!(w, entry_first, "issuer", w.text(&entry.issuer));
            key!(w, entry_first, "subject", w.text(&entry.subject));
            w.key("scopes", &mut entry_first)?;
            w.array(entry.scopes.len(), 2)?;
            for (position, scope) in entry.scopes.iter().enumerate() {
                if position != 0 {
                    w.raw(b",")?;
                }
                w.text(match scope {
                    WebHubScope::Project => "project",
                    WebHubScope::Global => "global",
                })?;
            }
            w.raw(b"]}")?;
        }
        w.raw(b"]")?;
    }
    let replacement = match change {
        Change::Provider(p) => Some(p),
        _ => None,
    };
    let removed = match change {
        Change::RemoveProvider(n, _) => Some(n),
        _ => None,
    };
    let providers = d.providers.len()
        + usize::from(replacement.is_some_and(|p| d.provider(&p.name).is_none()))
        - usize::from(removed.is_some());
    w.key("providers", &mut first)?;
    w.array(providers, d.limits.providers)?;
    let mut separator = false;
    for p in d
        .providers
        .iter()
        .filter(|p| {
            removed != Some(p.name.as_str()) && !replacement.is_some_and(|r| r.name == p.name)
        })
        .chain(replacement)
    {
        if separator {
            w.raw(b",")?;
        }
        separator = true;
        write_provider(w, p, document_revision)?;
    }
    w.raw(b"]")?;
    let replacement = match change {
        Change::Profile(p) => Some(p),
        _ => None,
    };
    let remove = |p: &&ProfileRecord| match change {
        Change::Logout(n) => p.name() == n,
        Change::RemoveProvider(n, true) => p.provider() == n,
        _ => false,
    };
    let count = d.profiles.iter().filter(|p| !remove(p)).count()
        + usize::from(replacement.is_some_and(|p| d.profile(p.name()).is_none()));
    w.key("profiles", &mut first)?;
    w.array(count, d.limits.profiles)?;
    separator = false;
    for p in d
        .profiles
        .iter()
        .filter(|p| !remove(p) && !replacement.is_some_and(|r| r.name() == p.name()))
        .chain(replacement)
    {
        if separator {
            w.raw(b",")?;
        }
        separator = true;
        let projection = match change {
            Change::Tokens(n, u, now) if p.name() == n => ProfileProjection::Tokens(u, now),
            Change::Active(n) if p.name() == n && d.active_profile.as_deref() != Some(n) => {
                ProfileProjection::Activation
            }
            _ => ProfileProjection::Unchanged,
        };
        write_profile(w, p, projection, document_revision)?;
    }
    w.raw(b"]")?;
    w.raw(b"}")
}
pub(super) fn validate_change(
    d: &AuthDocument,
    change: Change<'_>,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    let state = State::new(d.limits, context)?;
    let mut writer = Writer {
        state: &state,
        bytes: 0,
        output: None,
    };
    write_document(&mut writer, d, change)?;
    state.check()
}
pub(super) fn encode(
    d: &AuthDocument,
    context: &RequestContext<'_>,
) -> Result<Zeroizing<Vec<u8>>, ProfileError> {
    let state = State::new(d.limits, context)?;
    let mut counter = Writer {
        state: &state,
        bytes: 0,
        output: None,
    };
    write_document(&mut counter, d, Change::None)?;
    let size = counter.bytes;
    state.check()?;
    let mut output = Zeroizing::new(Vec::new());
    output
        .try_reserve_exact(size)
        .map_err(|_| ProfileError::Limit)?;
    let state = State::new(d.limits, context)?;
    let mut writer = Writer {
        state: &state,
        bytes: 0,
        output: Some(output),
    };
    write_document(&mut writer, d, Change::None)?;
    writer
        .flush()
        .map_err(|_| state.failure.get().unwrap_or(ProfileError::InvalidDocument))?;
    writer.output.ok_or(ProfileError::InvalidDocument)
}
pub(super) fn validate_provider_input(
    name: &str,
    config: &ProviderConfig,
    limits: ProfileLimits,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    let state = State::new(limits, context)?;
    state.scalar(name)?;
    bound_config(&state, config)?;
    valid_name(name)?;
    validate_config(config)?;
    write_config(
        &mut Writer {
            state: &state,
            bytes: 0,
            output: None,
        },
        config,
    )
}
pub(super) fn validate_profile_body_input(
    data: &ProfileBody,
    config: &ProviderConfig,
    limits: ProfileLimits,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    match data {
        ProfileBody::Named(data) => validate_profile_input(data, config, limits, context),
        ProfileBody::BrokerDevice(data) => {
            let state = State::new(limits, context)?;
            bound_device_data(&state, data)?;
            validate_device_data(data, Some(config))
        }
    }
}
pub(super) fn validate_profile_input(
    data: &ProfileData,
    config: &ProviderConfig,
    limits: ProfileLimits,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    let state = State::new(limits, context)?;
    bound_data(&state, data)?;
    validate_data(data, Some(config))?;
    write_data(
        &mut Writer {
            state: &state,
            bytes: 0,
            output: None,
        },
        data,
        None,
    )
}
pub(super) fn validate_token_update(
    u: &TokenUpdate,
    limits: ProfileLimits,
    context: &RequestContext<'_>,
) -> Result<(), ProfileError> {
    let state = State::new(limits, context)?;
    for v in [&u.access_token, &u.refresh_token] {
        state.scalar(v.expose())?;
        valid_text(v.expose())?;
    }
    if let ProfileIdentityUpdate::Replace { id_token, claims } = &u.identity {
        state.scalar(id_token.expose())?;
        valid_text(id_token.expose())?;
        bound_profile_claims(
            &state,
            &claims.username,
            claims.organization.as_deref(),
            &claims.teams,
        )?;
        valid_profile_claims(
            &claims.username,
            claims.organization.as_deref(),
            &claims.teams,
        )?;
    }
    valid_time(u.expires_at)
}

#[cfg(test)]
pub(super) fn writer_failure_case(case: &str) {
    use graphit_core::file_lock::Cancellation;
    use graphit_hub::native::ContentLimits;
    use std::time::Duration;
    let cancellation = Cancellation::default();
    let context = RequestContext::new(
        ContentLimits {
            max_input_bytes: 1000,
            max_output_bytes: 1000,
            max_depth: 10,
            max_nodes: 1000,
            max_scalar_bytes: 1000,
            max_collection_entries: 10,
        },
        &cancellation,
        Instant::now() + Duration::from_millis(if case == "deadline" { 20 } else { 5000 }),
    )
    .unwrap();
    let limits = ProfileLimits {
        bytes: if case == "limit" { 1 } else { 1000 },
        nodes: 1000,
        scalar_bytes: 1000,
        collection_entries: 10,
        providers: 10,
        profiles: 10,
    };
    let state = State::new(limits, &context).unwrap();
    if case == "cancelled" {
        cancellation.cancel();
    }
    if case == "deadline" {
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut writer = Writer {
        state: &state,
        bytes: 0,
        output: None,
    };
    let error = writer.write(b"a string too long").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    let error = serde_json::to_writer(&mut writer, "sentinel-sensitive-string").unwrap_err();
    assert!(error.is_io());
    assert_eq!(
        state.failure.get(),
        Some(match case {
            "limit" => ProfileError::Limit,
            "cancelled" => ProfileError::Cancelled,
            "deadline" => ProfileError::Deadline,
            _ => panic!("unknown writer case"),
        })
    );
}
