//! Temporary installations, signed Broker and canonical native datasets for compiled journeys.
#[path = "native_install_auth.rs"]
pub mod auth;
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use graphit_core::{
    artifact_store::{CapturedSourceReader, CopyPolicy, SourceReadLimits},
    file_lock::Cancellation,
    project_paths::InstallationPaths,
};
use graphit_hub::{
    native_package::{self, PackageKind},
    policy::{HubResourcePolicy, RequestContext},
    registry::HubRegistry,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub fn original(cancel: &Cancellation) -> RequestContext<'_> {
    let config =
        graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
            .unwrap();
    RequestContext::new(
        HubResourcePolicy::resolve(&config).unwrap().content(),
        cancel,
        Instant::now() + Duration::from_secs(180),
    )
    .unwrap()
}
pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub paths: InstallationPaths,
    pub policy: HubResourcePolicy,
    pub registry: HubRegistry,
    store: Arc<graphit_auth::store::AuthStore>,
}
impl Fixture {
    pub fn with_owner(
        broker: &auth::Broker,
        ctx: &RequestContext<'_>,
    ) -> (
        Self,
        Arc<graphit_daemon::auth_startup::DaemonAuth>,
        graphit_core::config::ConfigSnapshot,
    ) {
        let f = auth::Fixture::new(ctx, true);
        f.profile(broker, auth::now() + 600, "broker-api", false, ctx);
        let auth::Fixture {
            temp,
            paths,
            auth,
            store,
            policy,
            config,
            registry,
        } = f;
        (
            Self {
                temp,
                paths,
                policy,
                registry,
                store,
            },
            auth,
            config,
        )
    }
    pub fn new(broker: &auth::Broker, ctx: &RequestContext<'_>, local: bool) -> Self {
        let f = auth::Fixture::new(ctx, true);
        f.profile(broker, auth::now() + 600, "broker-api", local, ctx);
        let cleared = f
            .store
            .mutate(f.store.load(ctx).unwrap(), ctx, |doc, ctx| {
                doc.logout(Some("native"), doc.revision(), ctx)
            })
            .unwrap();
        assert!(cleared.failure().is_none() && cleared.cleanup_failure().is_none());
        assert_eq!(f.key(ctx).len(), 64);
        assert!(
            f.config
                .layer(graphit_core::config::ConfigSource::Distribution)
                .is_some()
        );
        f.remove(ctx);
        let auth::Fixture {
            temp,
            paths,
            auth,
            store,
            policy,
            config,
            registry,
        } = f;
        drop(auth);
        drop(config);
        Self {
            temp,
            paths,
            policy,
            registry,
            store,
        }
    }
    pub fn activate(
        &self,
        broker: &auth::Broker,
        ctx: &RequestContext<'_>,
        audience: &str,
        local: bool,
    ) {
        use graphit_auth::profile::{
            LocalCredentials, OidcTokens, ProfileCredentials, ProfileData, ProfileExpectation,
            Secret,
        };
        let data = ProfileData {
            name: "native".into(),
            provider: "service".into(),
            username: "display-only".into(),
            organization: None,
            teams: vec![],
            credentials: if local {
                ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: Some(Secret::new("fixture-local-key".into())),
                    broker_key: None,
                    embedding_api_key: None,
                    rerank_api_key: None,
                    s3: None,
                })
            } else {
                ProfileCredentials::Broker {
                    issuer: broker.origin.clone(),
                    subject: "native-fixture-sub".into(),
                    s3_disabled: true,
                    tokens: OidcTokens {
                        access_token: Secret::new(broker.token(auth::now() + 600, audience).into()),
                        refresh_token: None,
                        id_token: Secret::new("historical-fixture-id-token".into()),
                        expires_at: (auth::now() + 600).try_into().unwrap(),
                    },
                }
            },
        };
        let published = self
            .store
            .mutate(self.store.load(ctx).unwrap(), ctx, |doc, ctx| {
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("service").unwrap().revision(),
                    profile_revision: doc.profile("native").map(|p| p.revision()),
                };
                let updated = doc.profile("native").map_or(1, |p| p.updated_at() + 1);
                doc.replace_profile_data(data, expected, updated, ctx)?;
                doc.activate("native", doc.revision(), ctx)
            })
            .unwrap();
        assert_eq!(
            published.effect(),
            graphit_core::auth_document::publication::AuthPublicationEffect::Applied
        );
        assert!(published.failure().is_none() && published.cleanup_failure().is_none());
    }
    pub async fn package(
        &self,
        name: &str,
        kind: PackageKind,
        wrong_schema: bool,
        ctx: &RequestContext<'_>,
    ) -> PathBuf {
        let path = self.temp.path().join(name);
        fs::create_dir(&path).unwrap();
        if kind == PackageKind::Ast {
            fs::create_dir(path.join("graph.icebug")).unwrap();
            fs::write(path.join("graph.icebug/icebug.json"), serde_json::to_vec(&serde_json::json!({
                "manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,
                "add_reverse_edges":false,"schema_file":"schema.cypher","node_tables":[],"relationships":[],
                "relation_uids":true,"generation":"native-executable-fixture","n_edges":0,"repaired_strings":0,
                "invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}
            })).unwrap()).unwrap();
            fs::write(
                path.join("graph.icebug/schema.cypher"),
                "opaque schema metadata",
            )
            .unwrap();
            let schema = if wrong_schema {
                graphit_data::knowledge_index::physical_schema()
            } else {
                graphit_ast::indexed_source::source_schema()
            };
            let batch = if wrong_schema {
                RecordBatch::new_empty(Arc::clone(&schema))
            } else {
                let columns: Vec<ArrayRef> = vec![
                    Arc::new(StringArray::from(vec!["file.any"])),
                    Arc::new(StringArray::from(vec![format!(
                        "{name} compiled installed source\nsecond line"
                    )])),
                ];
                RecordBatch::try_new(Arc::clone(&schema), columns).unwrap()
            };
            let store = graphit_data::lance_module_store::LanceModuleStore::create(
                path.join("graph.icebug/source.lance").to_str().unwrap(),
                schema,
                vec![batch],
            )
            .await
            .unwrap();
            drop(store);
        } else {
            let schema = if wrong_schema {
                graphit_ast::indexed_source::source_schema()
            } else {
                graphit_data::knowledge_index::physical_schema()
            };
            let store = graphit_data::lance_module_store::LanceModuleStore::create(
                path.join("index.lance").to_str().unwrap(),
                Arc::clone(&schema),
                vec![RecordBatch::new_empty(schema)],
            )
            .await
            .unwrap();
            drop(store);
        }
        let snapshot = self
            .policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .unwrap();
        let source = CapturedSourceReader::capture(
            &path,
            graphit_fs::NodeKind::Directory,
            &snapshot,
            SourceReadLimits::new(65536, 4096, 536870912).unwrap(),
            Arc::clone(ctx.snapshot()),
        )
        .unwrap();
        let zip = self.temp.path().join(format!("{name}.zip"));
        native_package::export(&source, kind, &zip, ctx.snapshot()).unwrap();
        zip
    }
    pub fn command(&self, cwd: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(cwd)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", self.temp.path())
            .env("RUST_BACKTRACE", "0")
            .arg("--global-dir")
            .arg(self.paths.root());
        command
    }
}
