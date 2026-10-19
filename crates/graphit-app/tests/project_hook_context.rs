use graphit_auth::*;
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeBindingRecord, ScopeLimits, SnapshotPolicy,
        SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::NativePath,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, ProjectionLimits, SourceRef,
    native::{ContentLimits, Role, RoleCredentialReference},
    policy::RequestContext,
};
use graphit_records::memory_store::{MemoryStoreReadLimits, MemoryStoreWriteLimits};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(120))
}
fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 1048576,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 65536,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 16,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 4096,
    }
}
fn read_limits() -> MemoryStoreReadLimits {
    MemoryStoreReadLimits {
        max_query_bytes: NonZeroUsize::new(4 * 1024 * 1024).unwrap(),
        native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8 * 1024 * 1024).unwrap(),
        batch_rows: NonZeroU32::new(16).unwrap(),
    }
}
fn write_limits() -> MemoryStoreWriteLimits {
    MemoryStoreWriteLimits {
        max_input_bytes: NonZeroUsize::new(1024 * 1024).unwrap(),
        max_row_bytes: NonZeroUsize::new(2 * 1024 * 1024).unwrap(),
        read: read_limits(),
        artifacts: ArtifactLimits::new(
            1048576,
            65536,
            CopyLimits::new(1048576, 128, 65536, 16).unwrap(),
        )
        .unwrap(),
    }
}
struct Fixture {
    pub installation: CapturedPublicationScope,
    pub project: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project_record: ScopeBindingRecord,
    pub version: VersionContext,
    pub root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    _temp: tempfile::TempDir,
}
impl Fixture {
    pub fn new(ctx: &RequestContext<'_>, version: &str) -> Self {
        Self::new_for(ctx, version, "01KSH1CRFFG8Z74B5ZS78WW808")
    }
    pub fn new_for(ctx: &RequestContext<'_>, version: &str, project_id: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let install =
            InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
        let private = installation_bootstrap::prepare(
            &install,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                ctx.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let sl = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("project-hook-context-test").unwrap(),
            [root.clone()],
            sl,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            project_id.parse().unwrap(),
            sl,
            ctx.snapshot(),
        )
        .unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, version, None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            installation,
            project,
            installation_record,
            project_record,
            version,
            root,
            runtime,
            _temp: temp,
        }
    }
    pub fn scope(&self) -> ScopeInput<'_> {
        ScopeInput {
            installation: ScopeCapture {
                live: &self.installation,
                record: &self.installation_record,
            },
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    pub fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        self.issuer_with_ttl(ctx, Duration::from_secs(900))
    }
    fn issuer_with_ttl(&self, ctx: &RequestContext<'_>, ttl: Duration) -> RoleIssuer {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("project-hook-context-test").unwrap(),
            [self.installation.owner_root().to_owned()],
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let store = Arc::new(
            store::AuthStore::new(
                physical,
                profile::ProfileLimits {
                    bytes: 4096,
                    nodes: 4096,
                    scalar_bytes: 256,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        );
        let permissions = [
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::WriteFencedMemory,
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ];
        let sessions = AdminSessionPolicy {
            lifetime: Duration::from_secs(3600),
            maximum: Duration::from_secs(3600),
        };
        let authority = IssuerAuthorityPolicy::new(
            SourceAuthorityPolicy {
                ordinary: &permissions,
                administrative: &permissions,
                sessions,
            },
            SourceAuthorityPolicy {
                ordinary: &permissions,
                administrative: &permissions,
                sessions,
            },
            limits(),
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 65536,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        RoleIssuer::new(
            self.scope().installation,
            Arc::clone(&self.runtime),
            store,
            authority,
            TtlPolicy {
                ttl,
                maximum: Duration::from_secs(3600),
                refresh_margin: ttl / 4,
            },
            ctx,
        )
        .unwrap()
    }
    fn issue(
        &self,
        issuer: &RoleIssuer,
        ctx: &mut RequestContext<'_>,
    ) -> (
        RoleBearer,
        RoleCredentialReference,
        RunHandle,
        AdminSessionHandle,
    ) {
        // Observe and parse a real physical Agent before authenticated fixture administration.
        let directory = self.root.join("roles/coordinator");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("AGENT.md"), "---\nname: coordinator\ndescription: PROJECT hook reader\nread_only: false\n---\n\nRead project context.\n").unwrap();
        let retention = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
        let policy = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(1048576, 128, 65536, 8).unwrap(),
            RetainedFileSet::new([], retention).unwrap(),
            retention,
        )
        .unwrap();
        let captured = CapturedSourceReader::capture_project(
            &self.project,
            Path::new("roles/coordinator/AGENT.md"),
            graphit_fs::NodeKind::File,
            &policy,
            SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let file = captured.open_file(Path::new("")).unwrap();
        let observed = file
            .read_range(0, usize::try_from(file.len()).unwrap())
            .unwrap();
        assert_eq!(
            ctx.parse_agent(observed.as_ref()).unwrap().name(),
            "coordinator"
        );
        captured.revalidate_content().unwrap();
        let graphit_core::artifact_store::StoreOwner::Project { project_id } = self.project.owner()
        else {
            panic!("PROJECT owner")
        };
        let projection = ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap();
        let source = SourceRef::local_link(
            NativePath::new(directory).unwrap(),
            *project_id,
            ArtifactType::Agent,
            ArtifactId::new("roles/coordinator", projection).unwrap(),
            ConcreteVersion::new("1.0.0", projection).unwrap(),
            projection,
        )
        .unwrap();
        let permissions = [
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::Read(Module::Task),
        ];
        let definition = RoleDefinition::new(
            Role::Coordinator,
            &source,
            NativeBinding::KiroV3,
            &permissions,
            sha2::Sha256::digest(observed.as_ref()).into(),
            limits(),
        )
        .unwrap();
        let reference = definition.reference().clone();
        let secret =
            std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &self.scope(), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(&parent, &run, &reference, &permissions, &self.scope())
            .unwrap();
        (bearer, reference, run, admin)
    }
}

use graphit_app::project_hook_context::{HookContextError, ProjectHookInput, render_project_hook};
use graphit_core::config::{ConfigDocument, ConfigSnapshot, ConfigSource};
use graphit_core::module_settings::ModuleSettings;
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{CreateMemory, MemoryStoreError, NativeMemoryStore},
};
use graphit_runtime::{
    HookEvent, OutputError, OutputLimits,
    instructions::{
        InstructionErrorKind, InstructionLimits, InstructionValue, InstructionVariable,
    },
    policy_instructions::MandateAudience,
};
use serde_json::{Value, json};
use sha2::Digest;
use std::{collections::BTreeMap, error::Error, num::NonZeroU64};

fn modules(enabled: &[Module]) -> ModuleSettings {
    let b = Brand::default();
    let doc = ConfigDocument::new(
        &b,
        ConfigSource::Command,
        Module::ALL
            .into_iter()
            .filter(|m| *m != Module::DaemonUi)
            .map(|m| (m.key(), json!(enabled.contains(&m)))),
    )
    .unwrap();
    ModuleSettings::resolve(&ConfigSnapshot::new(&b, [doc]).unwrap()).unwrap()
}
fn instruction_limits() -> InstructionLimits {
    InstructionLimits {
        max_templates: NonZeroUsize::new(32).unwrap(),
        max_template_bytes: NonZeroUsize::new(65536).unwrap(),
        max_values: NonZeroUsize::new(256).unwrap(),
        max_variable_bytes: NonZeroUsize::new(262144).unwrap(),
        max_output_bytes: NonZeroUsize::new(262144).unwrap(),
        fuel: NonZeroU64::new(1000000).unwrap(),
        recursion: NonZeroUsize::new(32).unwrap(),
    }
}
fn data() -> BTreeMap<String, String> {
    // Alias text is renderer input only; no fixture alias or template supplies authority.
    let all: Value = serde_json::from_str(include_str!(
        "../../graphit-runtime/tests/fixtures/policy-instructions/variables.json"
    ))
    .unwrap();
    let mut values: BTreeMap<_, _> = all["graphit"]
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| {
            if k == "module_mandates" {
                None
            } else {
                v.as_str().map(|s| (k.clone(), s.to_owned()))
            }
        })
        .collect();
    values.insert("project_version".into(), "main".into());
    values
}
fn vars(data: &BTreeMap<String, String>) -> Vec<InstructionVariable<'_>> {
    data.iter()
        .map(|(name, text)| InstructionVariable {
            name,
            value: InstructionValue::Text(text),
        })
        .collect()
}
fn input<'a>(
    modules: &'a ModuleSettings,
    variables: &'a [InstructionVariable<'a>],
) -> ProjectHookInput<'a> {
    ProjectHookInput {
        task_context: None,
        binding: NativeBinding::KiroV3,
        event: HookEvent::SessionStart,
        modules,
        variables,
        audience: MandateAudience::Coordinator,
        instruction_limits: instruction_limits(),
        output_limits: OutputLimits {
            max_context_bytes: 262144,
            max_output_bytes: 524288,
        },
    }
}
fn memory(key: &str, mandatory: bool) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: format!("PROJECT {key}"),
        body: "literal {{ untrusted }} ação <&>\n".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Convention,
            mandatory,
            important: true,
        },
        tags: vec!["excluded-tag".into()],
        references: None,
    }
}
fn body(output: &graphit_runtime::EncodedOutput) -> String {
    String::from_utf8(output.bytes().to_vec()).unwrap()
}

fn error(result: Result<graphit_runtime::EncodedOutput, HookContextError>) -> HookContextError {
    result
        .err()
        .expect("expected causal failure without encoded output")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_project_memory_states_projection_version_and_readonly_channels() {
    let cancel = Cancellation::default();
    let mut ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run, admin) = f.issue(&issuer, &mut ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let settings = modules(&[
        Module::Hooks,
        Module::Memory,
        Module::Task,
        Module::Ast,
        Module::Hub,
        Module::Knowledge,
    ]);
    let mut values = data();
    let variables = vars(&values);
    let output = render_project_hook(
        &ctx,
        input(&settings, &variables),
        Some((&read, read_limits())),
        None,
    )
    .await
    .unwrap();
    assert!(body(&output).contains("PROJECT Memory state: no_corpus."));
    assert!(!f.project.runtime_root().join("memory").exists());
    NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        memory("optional", false),
        write_limits(),
    )
    .await
    .unwrap();
    let output = render_project_hook(
        &ctx,
        input(&settings, &variables),
        Some((&read, read_limits())),
        None,
    )
    .await
    .unwrap();
    assert!(body(&output).contains("PROJECT Memory state: empty."));
    assert!(!body(&output).contains("PROJECT optional"));
    let mandatory = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        memory("mandatory", true),
        write_limits(),
    )
    .await
    .unwrap();
    let tail =
        NativeMemoryStore::create(ctx.snapshot(), &write, memory("tail", true), write_limits())
            .await
            .unwrap();
    // A real alternate-version corpus shares this project and installation.
    let paths = ProjectPaths::at(
        &f.root,
        &InstallationPaths::resolve(
            Brand::default(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    let alternate =
        VersionContext::select(&paths, Checkout::NonGit, "alternate", None, "").unwrap();
    let mut alternate_scope = f.scope();
    alternate_scope.project.as_mut().unwrap().version = &alternate;
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let alternate_run = issuer
        .start_run(&parent, &alternate_scope, Duration::from_secs(3600))
        .unwrap();
    let permissions = [
        Permission::Read(Module::Memory),
        Permission::WriteMemory,
        Permission::Read(Module::Task),
    ];
    let alternate_bearer = issuer
        .issue(
            &parent,
            &alternate_run,
            &reference,
            &permissions,
            &alternate_scope,
        )
        .unwrap();
    let alternate_grant = issuer
        .verify(
            alternate_bearer.expose_for_transport(),
            &reference,
            alternate_scope,
            &ctx,
        )
        .unwrap();
    let alternate_write = authority
        .admit_write(&alternate_grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    NativeMemoryStore::create(
        ctx.snapshot(),
        &alternate_write,
        memory("foreign-version", true),
        write_limits(),
    )
    .await
    .unwrap();
    // A different physical PROJECT has its own actual grant and mandatory corpus.
    let foreign_cancel = Cancellation::default();
    let mut foreign_original = request(&foreign_cancel);
    let foreign_project = Fixture::new_for(&foreign_original, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    let foreign_issuer = foreign_project.issuer(&foreign_original);
    let (foreign_bearer, foreign_reference, _foreign_run, _foreign_admin) =
        foreign_project.issue(&foreign_issuer, &mut foreign_original);
    let foreign_grant = foreign_issuer
        .verify(
            foreign_bearer.expose_for_transport(),
            &foreign_reference,
            foreign_project.scope(),
            &foreign_original,
        )
        .unwrap();
    let foreign_authority = ProjectRecordAuthority::new(&foreign_issuer);
    let foreign_write = foreign_authority
        .admit_write(
            &foreign_grant,
            &foreign_original,
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    NativeMemoryStore::create(
        foreign_original.snapshot(),
        &foreign_write,
        memory("foreign-project", true),
        write_limits(),
    )
    .await
    .unwrap();
    let before = NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
        .await
        .unwrap();
    for (binding, event) in [
        (NativeBinding::KiroV3, HookEvent::SessionStart),
        (NativeBinding::KiroV3, HookEvent::UserPromptSubmit),
    ] {
        let mut selected = input(&settings, &variables);
        selected.binding = binding;
        selected.event = event;
        let mut one_row_batches = read_limits();
        one_row_batches.batch_rows = NonZeroU32::new(1).unwrap();
        let output = render_project_hook(&ctx, selected, Some((&read, one_row_batches)), None)
            .await
            .unwrap();
        assert_eq!(output.binding(), binding);
        assert_eq!(output.event(), event);
        let text = body(&output);
        assert!(text.contains("PROJECT Memory state: loaded."));
        assert!(text.contains("USER Memory was not loaded by this hook."));
        assert!(text.contains("Task sessions and claims were not restored or changed"));
        for tag in ["task_rule", "mem_rule", "ast_rule", "hub_rule", "doc_rule"] {
            assert!(text.contains(&format!("<{tag}>")));
        }
        assert!(text.find("<task_rule>").unwrap() < text.find("<mem_rule>").unwrap());
        assert!(text.contains("catalog_g__task_get"));
        assert!(text.contains("literal {{ untrusted }} ação <&>"));
        assert!(!text.contains("foreign-version"));
        assert!(!text.contains("foreign-project"));
        let json_text = text
            .split("Mandatory PROJECT Memory JSON (data):\n")
            .nth(1)
            .unwrap()
            .trim();
        let rows: Value = serde_json::from_str(json_text).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2);
        let ids: std::collections::BTreeSet<_> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        let expected_ids = [mandatory.id.to_string(), tail.id.to_string()];
        assert_eq!(ids, expected_ids.iter().map(String::as_str).collect());
        assert_eq!(rows[0]["revision"], 1);
        assert_eq!(rows[0]["classification"], "convention");
        assert_eq!(rows[0].as_object().unwrap().len(), 7);
        for absent in [
            "created_by",
            "updated_by",
            "tags",
            "references",
            "created_at",
            "updated_at",
        ] {
            assert!(rows[0].get(absent).is_none());
        }
    }
    let mut delegate = input(&settings, &variables);
    delegate.audience = MandateAudience::Delegate;
    let text = body(
        &render_project_hook(&ctx, delegate, Some((&read, read_limits())), None)
            .await
            .unwrap(),
    );
    assert!(text.contains("Do not acquire a claim, including your own Task"));
    assert!(text.contains("catalog_g__task_search"));
    assert_eq!(
        before,
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap()
    );
    values.insert("project_version".into(), "other".into());
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                input(&settings, &vars(&values)),
                Some((&read, read_limits())),
                None
            )
            .await
        ),
        HookContextError::ProjectVersionMismatch
    ));
}

#[tokio::test]
async fn disabled_and_validation_boundaries_do_not_require_memory() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let off = modules(&[]);
    assert!(
        render_project_hook(&ctx, input(&off, &[]), None, None)
            .await
            .unwrap()
            .bytes()
            .is_empty()
    );
    let settings = modules(&[Module::Hooks]);
    let values = data();
    let variables = vars(&values);
    let text = body(
        &render_project_hook(&ctx, input(&settings, &variables), None, None)
            .await
            .unwrap(),
    );
    assert!(text.contains("PROJECT Memory state: disabled."));
    assert!(!text.contains("<mem_rule>"));
    assert!(!text.contains("<task_rule>"));
    let mem = modules(&[Module::Hooks, Module::Memory]);
    assert!(matches!(
        error(render_project_hook(&ctx, input(&mem, &variables), None, None).await),
        HookContextError::MissingMemoryAdmission
    ));
    for (binding, event) in [
        (NativeBinding::Claude, HookEvent::BeforeAgent),
        (NativeBinding::KiroV3, HookEvent::BeforeAgent),
        (NativeBinding::Kimi, HookEvent::BeforeAgent),
    ] {
        let mut selected = input(&mem, &variables);
        selected.binding = binding;
        selected.event = event;
        assert!(matches!(
            error(render_project_hook(&ctx, selected, None, None).await),
            HookContextError::UnsupportedEvent
        ));
    }
    let mut reserved = variables;
    reserved.push(InstructionVariable {
        name: "project_memory_state",
        value: InstructionValue::Text("injected"),
    });
    assert!(matches!(
        error(render_project_hook(&ctx, input(&settings, &reserved), None, None).await),
        HookContextError::ReservedVariable
    ));
    let mut duplicate = vars(&values);
    duplicate.push(InstructionVariable {
        name: "brand_display",
        value: InstructionValue::Text("duplicate"),
    });
    assert!(
        matches!(error(render_project_hook(&ctx, input(&settings, &duplicate), None, None).await), HookContextError::Instruction(e) if e.kind() == InstructionErrorKind::DuplicateVariable)
    );
    let mut missing = values.clone();
    missing.remove("brand_display");
    assert!(matches!(
        error(render_project_hook(&ctx, input(&settings, &vars(&missing)), None, None).await),
        HookContextError::Instruction(_)
    ));
    let variables = vars(&values);
    let mut limited = input(&settings, &variables);
    limited.output_limits.max_context_bytes = 1;
    let cause = error(render_project_hook(&ctx, limited, None, None).await);
    assert!(matches!(
        cause,
        HookContextError::Output(OutputError::LimitExceeded)
    ));
    assert!(cause.source().is_some());
    assert!(!format!("{cause:?}").contains("Graphit"));
    let mut limited = input(&settings, &variables);
    limited.instruction_limits.max_variable_bytes = NonZeroUsize::new(1).unwrap();
    assert!(matches!(
        error(render_project_hook(&ctx, limited, None, None).await),
        HookContextError::Instruction(_)
    ));
    cancel.cancel();
    assert!(
        matches!(error(render_project_hook(&ctx, input(&settings, &variables), None, None).await), HookContextError::Request(e) if e.kind() == std::io::ErrorKind::Interrupted)
    );
    let cancel = Cancellation::default();
    let ctx = request_until(&cancel, Instant::now() + Duration::from_millis(1));
    std::thread::sleep(Duration::from_millis(2));
    assert!(
        matches!(error(render_project_hook(&ctx, input(&settings, &variables), None, None).await), HookContextError::Request(e) if e.kind() == std::io::ErrorKind::TimedOut)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_admissions_wrong_request_module_and_read_bounds_are_causal() {
    let cancel = Cancellation::default();
    let mut ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run, admin) = f.issue(&issuer, &mut ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let task = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let settings = modules(&[Module::Hooks, Module::Memory]);
    let values = data();
    let variables = vars(&values);
    assert!(
        matches!(error(render_project_hook(&ctx, input(&settings, &variables), Some((&task, read_limits())), None).await), HookContextError::Memory(e) if matches!(*e, MemoryStoreError::WrongAdmission))
    );
    let other = request(&cancel);
    assert!(
        matches!(error(render_project_hook(&other, input(&settings, &variables), Some((&read, read_limits())), None).await), HookContextError::Memory(e) if matches!(*e, MemoryStoreError::DifferentRequest))
    );
    NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        memory("bounded", true),
        write_limits(),
    )
    .await
    .unwrap();
    let mut bounded = read_limits();
    bounded.max_query_bytes = NonZeroUsize::MIN;
    assert!(
        render_project_hook(
            &ctx,
            input(&settings, &variables),
            Some((&read, bounded)),
            None
        )
        .await
        .is_ok()
    );
    let other_version = VersionContext::select(
        &ProjectPaths::at(
            &f.root,
            &InstallationPaths::resolve(
                Brand::default(),
                &f.root,
                None,
                Some(f.installation.owner_root()),
            )
            .unwrap(),
        )
        .unwrap(),
        Checkout::NonGit,
        "other",
        None,
        "",
    )
    .unwrap();
    let mut foreign = f.scope();
    foreign.project.as_mut().unwrap().version = &other_version;
    assert!(matches!(
        issuer.verify(bearer.expose_for_transport(), &reference, foreign, &ctx),
        Err(AuthError::Denied)
    ));
    let other_project = Fixture::new_for(&ctx, "main", "01KSH1CRFFG8Z74B5ZS78WW809");
    assert!(matches!(
        issuer.verify(
            bearer.expose_for_transport(),
            &reference,
            other_project.scope(),
            &ctx
        ),
        Err(AuthError::Denied)
    ));
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                input(&settings, &variables),
                Some((&read, read_limits())),
                None
            )
            .await
        ),
        HookContextError::Auth(AuthError::Denied)
    ));
}

#[tokio::test]
async fn actual_expired_admission_never_returns_context() {
    let cancel = Cancellation::default();
    let mut ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer_with_ttl(&ctx, Duration::from_secs(1));
    let (bearer, reference, _run, _admin) = f.issue(&issuer, &mut ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let settings = modules(&[Module::Hooks, Module::Memory]);
    let values = data();
    let variables = vars(&values);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(matches!(
        error(
            render_project_hook(
                &ctx,
                input(&settings, &variables),
                Some((&read, read_limits())),
                None
            )
            .await
        ),
        HookContextError::Auth(AuthError::Expired)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admitted_memory_with_escaped_json_over_cap_never_returns_context() {
    let cancel = Cancellation::default();
    let mut ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run, _admin) = f.issue(&issuer, &mut ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let mut authored = memory("escaped-limit", true);
    authored.body = "\0\"\\α\n".repeat(1024);
    NativeMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let before = NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
        .await
        .unwrap();
    let settings = modules(&[Module::Hooks, Module::Memory]);
    let mut values = data();
    // A renderer-only failure cannot masquerade as this JSON bound: this alias is missing.
    values.remove("skill_name_memory");
    let variables = vars(&values);
    let mut selected = input(&settings, &variables);
    selected.instruction_limits.max_variable_bytes = NonZeroUsize::new(4096).unwrap();
    let cause =
        error(render_project_hook(&ctx, selected, Some((&read, read_limits())), None).await);
    assert!(
        matches!(cause, HookContextError::Instruction(ref e) if e.kind() == InstructionErrorKind::LimitExceeded(graphit_runtime::instructions::InstructionLimit::VariableBytes))
    );
    assert!(cause.source().is_some());
    assert_eq!(
        before,
        NativeMemoryStore::list(ctx.snapshot(), &read, Default::default(), read_limits())
            .await
            .unwrap()
    );
}

#[path = "project_hook_context/task.rs"]
mod task_context;
