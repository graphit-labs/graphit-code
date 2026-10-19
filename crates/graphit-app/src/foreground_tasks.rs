//! Production Task/browser composition for the existing foreground service owner.
use crate::{
    browser_auth::{BrowserAuth, BrowserPolicy},
    task_server::{TaskPolicy, TaskServer},
};
use axum::Router;
use graphit_auth::{
    broker::{
        BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits,
        oauth::OAuthLimits,
        project_access::{BrokerProjectAccess, ProjectAccessPolicy},
    },
    profile::ProviderKind,
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::{
    artifact_store::ScopeLimits,
    config::{ConfigKey, ConfigSnapshot, ConfigSource},
    file_io::SnapshotContext,
    native_document_file::NativeDocumentFileLimits,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
};
use graphit_records::task_store::{
    TaskReferencesInputLimits, TaskStoreReadLimits, TaskStoreWriteLimits,
};
use serde::Deserialize;
use std::{
    io,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserConfig {
    #[serde(deserialize_with = "browser_providers")]
    providers: Vec<String>,
    project_endpoint: String,
    #[serde(default)]
    public_origin: Option<String>,
    #[serde(default)]
    knowledge_conversations: bool,
    #[serde(default)]
    allow_loopback_http: bool,
    #[serde(default = "login_default")]
    login_timeout_ms: u64,
    #[serde(default = "session_default")]
    session_lifetime_ms: u64,
    #[serde(default = "refresh_default")]
    refresh_margin_ms: u64,
}
fn browser_providers<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Providers {
        Csv(String),
        List(Vec<String>),
    }
    Ok(match Providers::deserialize(deserializer)? {
        Providers::Csv(text) if text.trim().is_empty() => Vec::new(),
        Providers::Csv(text) => text.split(',').map(|name| name.trim().to_owned()).collect(),
        Providers::List(names) => names,
    })
}
fn login_default() -> u64 {
    300_000
}
fn session_default() -> u64 {
    3_600_000
}
fn refresh_default() -> u64 {
    60_000
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Native Task policy is usable without configuring a browser session.
pub(crate) fn task_policy(
    hub: &HubResourcePolicy,
    scope: ScopeLimits,
    native: NativeDocumentFileLimits,
    binding: NativeBinding,
) -> io::Result<TaskPolicy> {
    let timeout = Duration::from_millis(hub.limit(ResourceSetting::RequestDeadlineMs).value);
    if timeout.is_zero()
        || Instant::now().checked_add(timeout).is_none()
        || timeout.as_secs() > i64::MAX as u64
    {
        return Err(invalid("invalid Task request duration"));
    }
    let content = hub.content();
    let read = TaskStoreReadLimits {
        native,
        batch_rows: NonZeroU32::new(8192).expect("Lance scanner batch default is positive"),
    };
    Ok(TaskPolicy {
        imported_knowledge: None,
        content,
        timeout,
        scope,
        read,
        write: TaskStoreWriteLimits {
            read,
            artifacts: hub.artifacts(),
            references: TaskReferencesInputLimits {
                max_entries: NonZeroUsize::new(content.max_collection_entries)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?,
                max_text_bytes: NonZeroUsize::new(content.max_input_bytes)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?,
            },
        },
        role_reference: hub.projection(),
        binding,
    })
}

pub(crate) struct ForegroundTaskConfig {
    config: BrowserConfig,
    browser: BrowserPolicy,
    task: TaskPolicy,
    network: NetworkLimits,
    verifier: VerifierLimits,
}
impl ForegroundTaskConfig {
    pub(crate) fn knowledge_conversations(&self) -> bool {
        self.config.knowledge_conversations
    }
    /// Providers explicitly selected by the trusted browser configuration.
    pub(crate) fn browser_providers(&self) -> &[String] {
        &self.config.providers
    }
    pub(crate) fn capture(
        config: &ConfigSnapshot,
        hub: &HubResourcePolicy,
        scope: ScopeLimits,
        native: NativeDocumentFileLimits,
        binding: NativeBinding,
        original: &SnapshotContext,
    ) -> io::Result<Option<Self>> {
        original.check()?;
        let global = ConfigSnapshot::new(
            config.brand(),
            [
                ConfigSource::Distribution,
                ConfigSource::Global,
                ConfigSource::Environment,
                ConfigSource::Command,
            ]
            .into_iter()
            .filter_map(|s| config.layer(s).cloned()),
        )?;
        let Some(config) = global
            .get::<BrowserConfig>(&ConfigKey::new("task.browser")?)
            .map_err(|_| invalid("invalid task.browser object"))?
        else {
            return Ok(None);
        };
        if config.providers.is_empty()
            || config
                .providers
                .iter()
                .enumerate()
                .any(|(i, name)| name.trim().is_empty() || config.providers[..i].contains(name))
            || config.project_endpoint.trim().is_empty()
        {
            return Err(invalid(
                "task.browser requires unique Broker providers and a PROJECT endpoint",
            ));
        }
        let duration = |ms: u64, zero_allowed: bool| {
            let value = Duration::from_millis(ms);
            if (!zero_allowed && value.is_zero())
                || Instant::now().checked_add(value).is_none()
                || value.as_secs() > i64::MAX as u64
            {
                Err(invalid("invalid task.browser duration"))
            } else {
                Ok(value)
            }
        };
        let task = task_policy(hub, scope, native, binding)?;
        let timeout = task.timeout;
        let content = task.content;
        let json = JsonLimits {
            bytes: content.max_input_bytes,
            nodes: content.max_nodes,
            depth: content.max_depth,
            scalar_bytes: content.max_scalar_bytes,
            collection_entries: content.max_collection_entries,
        };
        let browser = BrowserPolicy {
            oauth: OAuthLimits {
                authorization_url_bytes: content.max_scalar_bytes,
                callback_bytes: content.max_input_bytes,
                callback_query_entries: content.max_collection_entries,
                form_body_bytes: content.max_input_bytes,
                token_bytes: content.max_scalar_bytes,
                response_nodes: content.max_nodes,
                response_scalar_bytes: content.max_scalar_bytes,
            },
            request_timeout: timeout,
            login_timeout: duration(config.login_timeout_ms, false)?,
            session_lifetime: duration(config.session_lifetime_ms, false)?,
            refresh_margin: duration(config.refresh_margin_ms, true)?,
            allow_loopback_http: config.allow_loopback_http,
        };
        let network = NetworkLimits {
            request_body_bytes: content.max_input_bytes,
            response_body_bytes: content.max_input_bytes,
            url_bytes: content.max_scalar_bytes,
            header_bytes: content.max_input_bytes,
            header_count: content.max_collection_entries,
            max_in_flight: usize::try_from(hub.limit(ResourceSetting::TransportInflight).value)
                .map_err(io::Error::other)?,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 1,
            max_certificate_bytes: content.max_input_bytes,
        };
        let verifier = VerifierLimits {
            token_bytes: content.max_scalar_bytes,
            json,
            max_keys: content.max_collection_entries,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_secs(1),
        };
        original.check()?;
        Ok(Some(Self {
            config,
            browser,
            task,
            network,
            verifier,
        }))
    }
}

pub(crate) struct ForegroundTasks {
    ui_url: String,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    address: SocketAddr,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    executable: std::path::PathBuf,
    browser: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    conversation_providers: Option<Vec<(String, Arc<BrokerVerifier>)>>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    conversation_limits: graphit_ai::knowledge_conversation::ConversationLimits,
}
impl ForegroundTasks {
    pub(crate) fn new(
        auth: Arc<DaemonAuth>,
        tasks: TaskServer,
        captured: ForegroundTaskConfig,
        address: SocketAddr,
        stop: CancellationToken,
        original: &RequestContext<'_>,
    ) -> io::Result<Self> {
        original.snapshot().check()?;
        let (providers, backend) = {
            let key = auth
                .runtime()
                .check(original.snapshot())
                .map_err(io::Error::other)?;
            let raw =
                std::str::from_utf8(key.secret(original.snapshot()).map_err(io::Error::other)?)
                    .map_err(io::Error::other)?;
            let caller = auth
                .issuer()
                .authenticate_runtime(raw, original)
                .map_err(io::Error::other)?;
            let session = auth
                .issuer()
                .bootstrap_admin(&caller)
                .map_err(io::Error::other)?;
            let parent = auth
                .issuer()
                .admin_parent(&caller, &session)
                .map_err(io::Error::other)?;
            let manager = NativeProfileManager::new(auth.issuer());
            let snapshot = manager
                .begin(ProfileManagerAuthority::Admin(&parent))
                .map_err(io::Error::other)?;
            let records: Vec<_> = snapshot.providers().collect();
            let mut providers = Vec::with_capacity(captured.config.providers.len());
            let mut endpoint: Option<String> = None;
            for name in &captured.config.providers {
                let record = records
                    .iter()
                    .find(|r| r.name() == name)
                    .ok_or_else(|| invalid("task.browser provider is absent"))?;
                let ProviderKind::Broker(service) = &record.config().kind else {
                    return Err(invalid("task.browser provider must be Broker"));
                };
                if captured.config.knowledge_conversations
                    && service
                        .mcp_resource
                        .as_deref()
                        .is_none_or(|resource| resource.trim().is_empty())
                {
                    return Err(invalid(
                        "task.browser conversations require a configured Broker MCP resource",
                    ));
                }
                if endpoint.as_ref().is_some_and(|e| e != &service.endpoint) {
                    return Err(invalid(
                        "task.browser providers must share a Broker endpoint",
                    ));
                }
                endpoint = Some(service.endpoint.clone());
                let verifier = BrokerVerifier::new(
                    BrokerPolicy {
                        endpoint: &service.endpoint,
                        mcp_resource: service.mcp_resource.as_deref(),
                        provider_revision: record.revision(),
                        network: ClientPolicy {
                            origin: &service.endpoint,
                            allow_loopback_http: captured.config.allow_loopback_http,
                            proxy: None,
                            additional_roots: &[],
                        },
                    },
                    captured.network,
                    captured.verifier,
                )
                .map_err(io::Error::other)?;
                providers.push((name.clone(), Arc::new(verifier)));
            }
            let endpoint =
                endpoint.ok_or_else(|| invalid("task.browser requires a Broker endpoint"))?;
            let backend = Arc::new(
                BrokerProjectAccess::new(
                    ProjectAccessPolicy {
                        endpoint: &captured.config.project_endpoint,
                        network: ClientPolicy {
                            origin: &endpoint,
                            allow_loopback_http: captured.config.allow_loopback_http,
                            proxy: None,
                            additional_roots: &[],
                        },
                    },
                    captured.network,
                    captured.verifier.json,
                )
                .map_err(io::Error::other)?,
            );
            (providers, backend)
        };
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        let conversation_limits = graphit_ai::knowledge_conversation::ConversationLimits {
            artifacts: captured.task.write.artifacts,
            native: captured.task.read.native,
            record_bytes: NonZeroUsize::new(captured.task.content.max_input_bytes)
                .ok_or_else(|| invalid("conversation input limit"))?,
            field_bytes: NonZeroUsize::new(captured.task.content.max_scalar_bytes)
                .ok_or_else(|| invalid("conversation scalar limit"))?,
            sources: NonZeroUsize::new(captured.task.content.max_collection_entries)
                .ok_or_else(|| invalid("conversation sources limit"))?,
            list_records: NonZeroUsize::new(captured.task.content.max_collection_entries)
                .ok_or_else(|| invalid("conversation list limit"))?,
        };
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        let conversation_providers = captured
            .config
            .knowledge_conversations
            .then(|| providers.clone());
        let local_origin = format!("http://{address}");
        let browser = BrowserAuth::new(
            providers,
            captured
                .config
                .public_origin
                .as_deref()
                .unwrap_or(&local_origin),
            captured.browser,
            stop.clone(),
        )?;
        let origin = captured
            .config
            .public_origin
            .as_deref()
            .unwrap_or(&local_origin);
        let ui_url = url::Url::parse(origin)
            .map_err(io::Error::other)?
            .join("/task/gantt")
            .map_err(io::Error::other)?
            .to_string();
        original.snapshot().check()?;
        Ok(Self {
            ui_url,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            address,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            executable: std::env::current_exe()?,
            browser,
            tasks,
            backend,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            conversation_providers,
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            conversation_limits,
        })
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn conversation_access(
        &self,
    ) -> Option<crate::knowledge_conversation::ConversationAccess> {
        self.conversation_providers.as_ref().map(|providers| {
            crate::knowledge_conversation::ConversationAccess {
                providers: providers.clone(),
                backend: Arc::clone(&self.backend),
                limits: self.conversation_limits,
            }
        })
    }
    pub(crate) fn ui_url(&self) -> &str {
        &self.ui_url
    }
    pub(crate) fn router(
        &self,
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        sources: crate::source_server::SourceServer,
    ) -> Router {
        let router = self.browser.task_router(
            self.tasks.clone(),
            Arc::clone(&self.backend),
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            Some(sources.clone()),
        );
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        let router = router
            .merge(self.browser.daemon_router(sources.clone()))
            .merge(self.browser.now_router(sources.clone()))
            .merge(
                self.browser
                    .ast_router(sources.clone(), Arc::clone(&self.backend)),
            )
            .merge(self.browser.live_execution_router(
                self.tasks.clone(),
                Arc::clone(&self.backend),
                self.address,
                self.executable.clone(),
                Some(sources),
            ));
        router.merge(crate::foreground_ui::router())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn attach_project_configuration(
        &self,
        configuration: (
            graphit_core::project_paths::InstallationPaths,
            Arc<crate::source_server::project_hook::HookPolicy>,
        ),
    ) -> io::Result<()> {
        self.tasks.attach_project_configuration(configuration)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) fn attach_memory_model(
        &self,
        resources: Option<Arc<crate::foreground_memory_jobs::MemoryJobResources>>,
    ) -> io::Result<()> {
        self.tasks.attach_memory_model(resources)
    }
    pub(crate) async fn shutdown(&self) -> io::Result<()> {
        self.browser.shutdown().await
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod conversation_optin_tests {
    use super::*;
    #[test]
    fn conversation_optin_is_false_unless_literal_bool_true() {
        let base = serde_json::json!({"providers":["broker"],"project_endpoint":"https://broker.example/access"});
        assert!(
            !serde_json::from_value::<BrowserConfig>(base.clone())
                .unwrap()
                .knowledge_conversations
        );
        for value in [serde_json::json!(false), serde_json::json!(true)] {
            let mut input = base.clone();
            input["knowledge_conversations"] = value.clone();
            assert_eq!(
                serde_json::from_value::<BrowserConfig>(input)
                    .unwrap()
                    .knowledge_conversations,
                value.as_bool().unwrap()
            );
        }
        for value in [
            serde_json::json!(null),
            serde_json::json!("true"),
            serde_json::json!(1),
            serde_json::json!([]),
        ] {
            let mut input = base.clone();
            input["knowledge_conversations"] = value;
            assert!(serde_json::from_value::<BrowserConfig>(input).is_err());
        }
    }
}
