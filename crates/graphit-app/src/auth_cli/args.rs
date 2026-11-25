use clap::{Args, Subcommand, ValueEnum};
use graphit_auth::profile::{
    AiService, BrokerService, DirectProtocol, LocalDevice, LocalExecution, ProviderConfig,
    ProviderKind, S3Location, S3Source,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

#[derive(Subcommand, Serialize, Deserialize)]
pub enum Command {
    Provider(Box<ProviderArguments>),
    Account(AccountArguments),
    Login(Box<LoginArguments>),
    Logout(LogoutArguments),
    Refresh(RefreshArguments),
}

#[derive(Args, Serialize, Deserialize)]
pub struct ProviderArguments {
    #[command(subcommand)]
    pub command: ProviderCommand,
}
#[derive(Subcommand, Serialize, Deserialize)]
pub enum ProviderCommand {
    List,
    Show {
        name: String,
    },
    Add {
        name: String,
        #[arg(long = "type", value_enum)]
        kind: ProviderType,
        #[command(flatten)]
        fields: ProviderFields,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    Update {
        name: String,
        #[arg(long = "type", value_enum)]
        kind: Option<ProviderType>,
        #[command(flatten)]
        fields: ProviderFields,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    Remove {
        name: String,
        #[arg(long)]
        cascade: bool,
        #[arg(long, required = true)]
        yes: bool,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
}
#[derive(Args, Serialize, Deserialize)]
pub struct AccountArguments {
    #[command(subcommand)]
    pub command: AccountCommand,
}
#[derive(Subcommand, Serialize, Deserialize)]
pub enum AccountCommand {
    List,
    Show {
        profile: Option<String>,
    },
    Use {
        profile: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
}
#[derive(Args, Serialize, Deserialize)]
pub struct LogoutArguments {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub expected_revision: Option<u64>,
}
#[derive(Args, Serialize, Deserialize)]
pub struct RefreshArguments {
    #[arg(long)]
    pub expected_revision: Option<u64>,
}
#[derive(Args, Serialize, Deserialize)]
pub struct LoginArguments {
    /// Authorize with a public user code instead of a loopback callback.
    #[arg(long)]
    #[serde(default)]
    pub device: bool,
    #[arg(long)]
    pub profile: String,
    #[arg(long)]
    pub provider: String,
    #[arg(long, conflicts_with = "anonymous")]
    pub username: Option<String>,
    #[arg(long, conflicts_with = "anonymous")]
    pub organization: Option<String>,
    #[arg(long = "team", conflicts_with = "anonymous")]
    pub teams: Vec<String>,
    #[arg(long)]
    pub anonymous: bool,
    #[arg(long)]
    pub s3_disabled: bool,
    #[arg(long, value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub mcp_key: Option<SecretString>,
    #[arg(long, conflicts_with = "anonymous", value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub broker_key: Option<SecretString>,
    #[arg(long, value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub embedding_api_key: Option<SecretString>,
    #[arg(long, value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub rerank_api_key: Option<SecretString>,
    #[arg(long, requires = "s3_secret_key", value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub s3_access_key: Option<SecretString>,
    #[arg(long, requires = "s3_access_key", value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub s3_secret_key: Option<SecretString>,
    #[arg(long, requires = "s3_access_key", value_parser = secret)]
    #[serde(default, with = "secret_wire")]
    pub s3_session_token: Option<SecretString>,
    #[arg(long, requires = "s3_access_key")]
    pub s3_expires_at: Option<i64>,
    #[arg(long)]
    pub aws_profile: Option<String>,
    #[arg(long)]
    pub expected_revision: Option<u64>,
}
fn secret(value: &str) -> Result<SecretString, String> {
    if value.is_empty() {
        Err("credential must be nonempty".into())
    } else {
        Ok(SecretString::from(value))
    }
}

#[derive(Clone, Copy, ValueEnum, Serialize, Deserialize)]
pub enum ProviderType {
    Local,
    Broker,
}
#[derive(Clone, Copy, ValueEnum, Serialize, Deserialize)]
pub enum ServiceMode {
    Local,
    Direct,
    Broker,
    Disabled,
}
#[derive(Clone, Copy, ValueEnum, Serialize, Deserialize)]
pub enum Device {
    Auto,
    Cpu,
    Cuda,
    Metal,
}
#[derive(Clone, Copy, ValueEnum, Serialize, Deserialize)]
pub enum Protocol {
    #[value(name = "openai")]
    OpenAi,
    #[value(name = "openai-compatible")]
    OpenAiCompatible,
    Cohere,
    CohereV2,
    Voyage,
    VoyageV1,
    #[value(name = "google-gemini-1")]
    GoogleGemini1,
    #[value(name = "google-gemini-2")]
    GoogleGemini2,
    Jina,
    JinaV1,
}
#[derive(Clone, Copy, ValueEnum, Serialize, Deserialize)]
pub enum CredentialSource {
    Login,
    AwsChain,
    Broker,
}

#[derive(Args, Default, Serialize, Deserialize)]
pub struct ProviderFields {
    #[arg(long, conflicts_with = "clear_broker")]
    pub broker_endpoint: Option<String>,
    #[arg(long, conflicts_with = "clear_broker")]
    pub mcp_resource: Option<String>,
    #[arg(long, conflicts_with = "clear_broker")]
    pub broker_allow_anonymous: bool,
    #[arg(long, conflicts_with = "broker_allow_anonymous")]
    pub broker_deny_anonymous: bool,
    #[arg(long)]
    pub clear_broker: bool,
    #[arg(long)]
    pub allow_aws_credential_chain: bool,
    #[arg(long, conflicts_with = "allow_aws_credential_chain")]
    pub deny_aws_credential_chain: bool,
    #[arg(long, value_enum)]
    pub embedding_mode: Option<ServiceMode>,
    #[arg(long, value_enum)]
    pub embedding_device: Option<Device>,
    #[arg(long)]
    pub embedding_device_id: Option<u32>,
    #[arg(long, value_enum)]
    pub embedding_protocol: Option<Protocol>,
    #[arg(long)]
    pub embedding_endpoint: Option<String>,
    #[arg(long)]
    pub embedding_model: Option<String>,
    /// Explicit pinned model or deployment revision supplied by the provider owner.
    #[arg(long)]
    pub embedding_model_revision: Option<String>,
    /// Explicit reproducible embedding input/output recipe identifier.
    #[arg(long)]
    pub embedding_recipe: Option<String>,
    #[arg(long)]
    pub embedding_dimensions: Option<u32>,
    #[arg(long, conflicts_with = "embedding_clear_request_dimensions")]
    pub embedding_request_dimensions: Option<u32>,
    #[arg(long)]
    pub embedding_clear_request_dimensions: bool,
    #[arg(long, conflicts_with = "embedding_clear_batch_endpoint")]
    pub embedding_batch_endpoint: Option<String>,
    #[arg(long)]
    pub embedding_clear_batch_endpoint: bool,
    #[arg(long, value_enum)]
    pub rerank_mode: Option<ServiceMode>,
    #[arg(long, value_enum)]
    pub rerank_device: Option<Device>,
    #[arg(long)]
    pub rerank_device_id: Option<u32>,
    #[arg(long, value_enum)]
    pub rerank_protocol: Option<Protocol>,
    #[arg(long)]
    pub rerank_endpoint: Option<String>,
    #[arg(long)]
    pub rerank_model: Option<String>,
    #[arg(long)]
    pub rerank_model_revision: Option<String>,
    #[arg(long)]
    pub rerank_recipe: Option<String>,
    #[arg(long)]
    pub rerank_dimensions: Option<u32>,
    #[arg(long)]
    pub s3_bucket: Option<String>,
    #[arg(long)]
    pub s3_region: Option<String>,
    #[arg(long)]
    pub s3_endpoint: Option<String>,
    #[arg(long)]
    pub s3_prefix: Option<String>,
    #[arg(long, value_enum)]
    pub s3_credential_source: Option<CredentialSource>,
    #[arg(long, conflicts_with_all = ["s3_bucket", "s3_region", "s3_endpoint", "s3_prefix", "s3_credential_source"])]
    pub clear_s3: bool,
}

impl ProviderFields {
    pub(super) fn apply(
        self,
        kind: Option<ProviderType>,
        current: Option<&ProviderConfig>,
    ) -> Result<ProviderConfig, &'static str> {
        let local = ProviderConfig {
            kind: ProviderKind::Local {
                allow_aws_credential_chain: false,
                broker: None,
            },
            embedding: AiService::Local(LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            }),
            rerank: AiService::Local(LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            }),
            s3: S3Location {
                bucket: None,
                region: None,
                endpoint: None,
                prefix: None,
                source: if current.is_none() && matches!(kind, Some(ProviderType::Broker)) {
                    S3Source::BrokerScope
                } else {
                    S3Source::LocalCredentials
                },
            },
        };
        let old = current.unwrap_or(&local);
        let broker = match &old.kind {
            ProviderKind::Local { broker, .. } => broker.clone(),
            ProviderKind::Broker(broker) => Some(broker.clone()),
        };
        let broker = if self.clear_broker {
            None
        } else if let Some(endpoint) = self.broker_endpoint {
            Some(BrokerService {
                endpoint,
                mcp_resource: self
                    .mcp_resource
                    .or_else(|| broker.as_ref().and_then(|b| b.mcp_resource.clone())),
                allow_anonymous: if self.broker_allow_anonymous {
                    true
                } else if self.broker_deny_anonymous {
                    false
                } else {
                    broker.as_ref().is_some_and(|b| b.allow_anonymous)
                },
            })
        } else if let Some(mut broker) = broker {
            if let Some(resource) = self.mcp_resource {
                broker.mcp_resource = Some(resource);
            }
            if self.broker_allow_anonymous {
                broker.allow_anonymous = true;
            }
            if self.broker_deny_anonymous {
                broker.allow_anonymous = false;
            }
            Some(broker)
        } else {
            if self.mcp_resource.is_some()
                || self.broker_allow_anonymous
                || self.broker_deny_anonymous
            {
                return Err("Broker options require a Broker endpoint");
            }
            None
        };
        let kind = match kind.unwrap_or(match old.kind {
            ProviderKind::Local { .. } => ProviderType::Local,
            ProviderKind::Broker(_) => ProviderType::Broker,
        }) {
            ProviderType::Broker => {
                ProviderKind::Broker(broker.ok_or("Broker provider requires a Broker endpoint")?)
            }
            ProviderType::Local => ProviderKind::Local {
                allow_aws_credential_chain: if self.allow_aws_credential_chain {
                    true
                } else if self.deny_aws_credential_chain {
                    false
                } else {
                    matches!(
                        old.kind,
                        ProviderKind::Local {
                            allow_aws_credential_chain: true,
                            ..
                        }
                    )
                },
                broker,
            },
        };
        if matches!(kind, ProviderKind::Broker(_))
            && (self.allow_aws_credential_chain || self.deny_aws_credential_chain)
        {
            return Err("AWS credential chain options require a local provider");
        }
        let becoming_broker =
            matches!(kind, ProviderKind::Broker(_)) && !matches!(old.kind, ProviderKind::Broker(_));
        let becoming_local = matches!(kind, ProviderKind::Local { .. })
            && matches!(old.kind, ProviderKind::Broker(_));
        let embedding = service(
            if becoming_broker && !matches!(old.embedding, AiService::Disabled) {
                &AiService::Broker
            } else if becoming_local && matches!(old.embedding, AiService::Broker) {
                &local.embedding
            } else {
                &old.embedding
            },
            ServiceFields {
                mode: self.embedding_mode,
                device: self.embedding_device,
                device_id: self.embedding_device_id,
                protocol: self.embedding_protocol,
                endpoint: self.embedding_endpoint,
                model: self.embedding_model,
                model_revision: self.embedding_model_revision,
                recipe: self.embedding_recipe,
                dimensions: self.embedding_dimensions,
                request_dimensions: if self.embedding_clear_request_dimensions {
                    Some(None)
                } else {
                    self.embedding_request_dimensions.map(Some)
                },
                batch_endpoint: if self.embedding_clear_batch_endpoint {
                    Some(None)
                } else {
                    self.embedding_batch_endpoint.map(Some)
                },
            },
        )?;
        let rerank = service(
            if becoming_broker && !matches!(old.rerank, AiService::Disabled) {
                &AiService::Broker
            } else if becoming_local && matches!(old.rerank, AiService::Broker) {
                &local.rerank
            } else {
                &old.rerank
            },
            ServiceFields {
                mode: self.rerank_mode,
                device: self.rerank_device,
                device_id: self.rerank_device_id,
                protocol: self.rerank_protocol,
                endpoint: self.rerank_endpoint,
                model: self.rerank_model,
                model_revision: self.rerank_model_revision,
                recipe: self.rerank_recipe,
                dimensions: self.rerank_dimensions,
                request_dimensions: None,
                batch_endpoint: None,
            },
        )?;
        let s3 = if self.clear_s3 {
            local.s3
        } else {
            S3Location {
                bucket: self.s3_bucket.or_else(|| old.s3.bucket.clone()),
                region: self.s3_region.or_else(|| old.s3.region.clone()),
                endpoint: self.s3_endpoint.or_else(|| old.s3.endpoint.clone()),
                prefix: self.s3_prefix.or_else(|| old.s3.prefix.clone()),
                source: self
                    .s3_credential_source
                    .map_or(old.s3.source, |source| match source {
                        CredentialSource::Login => S3Source::LocalCredentials,
                        CredentialSource::AwsChain => S3Source::AwsChain,
                        CredentialSource::Broker => S3Source::BrokerScope,
                    }),
            }
        };
        let config = ProviderConfig {
            kind,
            embedding,
            rerank,
            s3,
        };
        config.validate_ai_services().map_err(|_| {
            "invalid AI services: Broker requires broker or disabled services; Direct rerank requires Cohere, Voyage or Jina with zero dimensions and valid native service fields"
        })?;
        Ok(config)
    }
}

struct ServiceFields {
    mode: Option<ServiceMode>,
    device: Option<Device>,
    device_id: Option<u32>,
    protocol: Option<Protocol>,
    endpoint: Option<String>,
    model: Option<String>,
    model_revision: Option<String>,
    recipe: Option<String>,
    dimensions: Option<u32>,
    request_dimensions: Option<Option<u32>>,
    batch_endpoint: Option<Option<String>>,
}
fn service(old: &AiService, input: ServiceFields) -> Result<AiService, &'static str> {
    let ServiceFields {
        mode,
        device,
        device_id,
        protocol,
        endpoint,
        model,
        model_revision,
        recipe,
        dimensions,
        request_dimensions,
        batch_endpoint,
    } = input;
    let mode = mode.unwrap_or(match old {
        AiService::Local(_) => ServiceMode::Local,
        AiService::Direct { .. } => ServiceMode::Direct,
        AiService::Broker => ServiceMode::Broker,
        AiService::Disabled => ServiceMode::Disabled,
    });
    if !matches!(mode, ServiceMode::Local) && (device.is_some() || device_id.is_some()) {
        return Err("device options require a local service");
    }
    if !matches!(mode, ServiceMode::Direct)
        && (protocol.is_some()
            || endpoint.is_some()
            || model.is_some()
            || model_revision.is_some()
            || recipe.is_some()
            || dimensions.is_some()
            || request_dimensions.is_some()
            || batch_endpoint.is_some())
    {
        return Err(
            "protocol/endpoint/model/revision/recipe/dimensions and options require a direct service",
        );
    }
    Ok(match mode {
        ServiceMode::Disabled => AiService::Disabled,
        ServiceMode::Broker => AiService::Broker,
        ServiceMode::Local => {
            let old = match old {
                AiService::Local(v) => *v,
                _ => LocalExecution {
                    device: LocalDevice::Cpu,
                    device_id: 0,
                },
            };
            AiService::Local(LocalExecution {
                device: device.map_or(old.device, |d| match d {
                    Device::Auto => LocalDevice::Auto,
                    Device::Cpu => LocalDevice::Cpu,
                    Device::Cuda => LocalDevice::Cuda,
                    Device::Metal => LocalDevice::Metal,
                }),
                device_id: device_id.unwrap_or(old.device_id),
            })
        }
        ServiceMode::Direct => {
            let prior = match old {
                AiService::Direct {
                    protocol,
                    endpoint,
                    model,
                    model_revision,
                    recipe,
                    dimensions,
                    request_dimensions,
                    batch_endpoint,
                } => Some((
                    *protocol,
                    endpoint,
                    model,
                    *dimensions,
                    *request_dimensions,
                    batch_endpoint,
                    model_revision,
                    recipe,
                )),
                _ => None,
            };
            let protocol = protocol
                .map(|p| match p {
                    Protocol::OpenAi => DirectProtocol::OpenAi,
                    Protocol::OpenAiCompatible => DirectProtocol::OpenAiCompatible,
                    Protocol::Cohere => DirectProtocol::Cohere,
                    Protocol::CohereV2 => DirectProtocol::CohereV2,
                    Protocol::Voyage => DirectProtocol::Voyage,
                    Protocol::VoyageV1 => DirectProtocol::VoyageV1,
                    Protocol::GoogleGemini1 => DirectProtocol::GoogleGemini1,
                    Protocol::GoogleGemini2 => DirectProtocol::GoogleGemini2,
                    Protocol::Jina => DirectProtocol::Jina,
                    Protocol::JinaV1 => DirectProtocol::JinaV1,
                })
                .or(prior.map(|p| p.0))
                .ok_or("direct service requires a protocol")?;
            AiService::Direct {
                protocol,
                endpoint: endpoint
                    .or_else(|| prior.map(|p| p.1.clone()))
                    .ok_or("direct service requires an endpoint")?,
                model: model
                    .or_else(|| prior.map(|p| p.2.clone()))
                    .ok_or("direct service requires a model")?,
                model_revision: model_revision
                    .or_else(|| prior.map(|p| p.6.clone()))
                    .ok_or("direct service requires a pinned model/deployment revision")?,
                recipe: recipe
                    .or_else(|| prior.map(|p| p.7.clone()))
                    .ok_or("direct service requires a recipe")?,
                dimensions: dimensions
                    .or(prior.map(|p| p.3))
                    .ok_or("direct service requires dimensions")?,
                request_dimensions: request_dimensions.unwrap_or_else(|| prior.and_then(|p| p.4)),
                batch_endpoint: batch_endpoint.unwrap_or_else(|| prior.and_then(|p| p.5.clone())),
            }
        }
    })
}

// This explicit transport codec is used only for private authenticated requests.
// Command remains non-Debug and response views never expose these credentials.
mod secret_wire {
    use super::*;
    use secrecy::ExposeSecret;
    pub fn serialize<S: serde::Serializer>(
        value: &Option<SecretString>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .map(|secret| secret.expose_secret())
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<SecretString>, D::Error> {
        let value = Option::<String>::deserialize(deserializer)?;
        if value.as_ref().is_some_and(String::is_empty) {
            return Err(serde::de::Error::custom("credential must be nonempty"));
        }
        Ok(value.map(SecretString::from))
    }
}
