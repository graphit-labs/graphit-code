#![cfg(target_os = "linux")]
//! Actual PROJECT/FULL16 and native Process/Codex; no vendor authentication claim.
use crate::agent_fixture as native_agent_fixture;
use graphit_ai::{
    cli_codex::{CodexReceipt, CodexSink, CodexStreamLimits, run_codex},
    cli_codex_wire::{CodexEventView, CodexWireLimits},
    codex_completion::{AttemptPurpose, CompletionLimits, CompletionOutcome, UsageEvidence},
};
use graphit_app::knowledge_question::*;

#[path = "support/knowledge_ask.rs"]
mod authenticated_ask;
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use graphit_process::{
    Cleanup, HostLimits, HostReadiness, HostShutdown, HostShutdownObservation, Limits, ProcessHost,
    RepairObservation, Request, SinkFailure,
};
use std::{
    error::Error,
    future::Future,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn request(cancel: &Cancellation, timeout: Duration) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        cancel,
        Instant::now() + timeout,
    )
    .unwrap()
}
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>) -> Self {
        Self::with_version(ctx, "")
    }
    fn with_version(ctx: &RequestContext<'_>, version_name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&home).unwrap();
        let install = InstallationPaths::resolve(
            Brand::new("knowledge-test").unwrap(),
            &root,
            Some(&home),
            None,
        )
        .unwrap();
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
        let limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [root.clone()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let version =
            VersionContext::select(&paths, Checkout::NonGit, version_name, None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            _dir: dir,
            root,
            runtime,
            installation,
            installation_record,
            project,
            project_record,
            version,
        }
    }
    fn realm(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.installation_record,
        }
    }
    fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.realm(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    fn issuer(&self, ctx: &RequestContext<'_>, perms: &[Permission]) -> RoleIssuer {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("local-installation").unwrap(),
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
        let local = Arc::new(
            AuthStore::new(
                physical,
                ProfileLimits {
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
        let source = SourceAuthorityPolicy {
            ordinary: perms,
            administrative: perms,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(3600),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source,
            source,
            issuer_limits(),
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
            self.realm(),
            self.runtime.clone(),
            local,
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
}

fn perms() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
        Permission::Read(Module::Memory),
    ]
}
fn content() -> KnowledgeIndexContentLimits {
    KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    }
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn read_limits() -> KnowledgeIndexReadLimits {
    KnowledgeIndexReadLimits {
        content: content(),
        native: native(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn write_limits() -> KnowledgeIndexWriteLimits {
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    KnowledgeIndexWriteLimits {
        content: content(),
        native: native(),
        artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
    }
}
fn page<'a>(slug: &'a str, title: &'a str, body: &'a str) -> h::AuthoredPageFields<'a> {
    h::AuthoredPageFields {
        slug: Some(slug),
        title: Some(title),
        body: Some(body),
        kind: Some("page"),
        summary: Some("literal"),
        description: None,
        summary_truncated: false,
        source_path: "docs/fixture.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([2; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(16),
            }),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }
}
const BODY: &str = "---\ntitle: body metadata stays literal\n---\nParis é a capital.\n";
fn pages() -> [h::AuthoredPageFields<'static>; 2] {
    [
        page("Paris", "Capital \"β\"", BODY),
        page("Database", "Database", "UNSELECTED DATABASE BODY"),
    ]
}
async fn publish(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    pages: &[h::AuthoredPageFields<'_>],
) {
    let rows = h::prepare(
        ctx.snapshot(),
        pages,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(8),
            max_observations: n(8),
            max_rows: n(64),
            max_field_bytes: n(65536),
            max_text_bytes: n(131072),
        },
    )
    .unwrap();
    publish_native_authored(ctx.snapshot(), write, &rows, write_limits())
        .await
        .unwrap();
}
fn wire() -> CodexWireLimits {
    CodexWireLimits {
        frame_bytes: 8192,
        depth: 16,
        nodes: 1000,
        fields: 1000,
        array_items: 1000,
        string_bytes: 8192,
        decoded_string_bytes: 8192,
        id_bytes: 256,
    }
}
fn completion() -> CompletionLimits {
    CompletionLimits {
        max_graphit_attempts: 1,
        max_response_bytes: 8192,
        max_model_id_bytes: 256,
    }
}
fn model(w: &CodexWireLimits) -> KnowledgeQuestionModel<'_> {
    KnowledgeQuestionModel {
        requested_model: "fixture-model",
        wire_limits: w,
        completion_limits: completion(),
    }
}
fn limits() -> KnowledgeQuestionLimits {
    KnowledgeQuestionLimits {
        read: read_limits(),
        max_query_bytes: n(4096),
        max_selected_pages: n(8),
        max_prompt_bytes: n(65536),
    }
}
fn native_test(f: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .name("knowledge-question-native".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(f)
        })
        .unwrap()
        .join()
        .unwrap();
}
fn process(directory: &Path, prompt: String, mode: &str) -> Request {
    let program = directory.join(format!("question-native-{mode}"));
    if !program.exists() {
        native_agent_fixture::install(
            &program,
            serde_json::json!({
                "kind":"question", "mode":mode, "expected_body":BODY
            }),
        );
    }
    // Authenticated callers construct an unfilled Request before admission.
    // Only the real child reads the prompt after the pipeline supplies stdin.
    Request {
        program,
        args: Vec::new(),
        cwd: directory.to_owned(),
        environment: vec![],
        stdin: Some(prompt.into_bytes().into()),
        limits: Limits {
            stdin_bytes: 65536,
            stdout_bytes: 32768,
            stderr_bytes: 32768,
            termination_grace: Duration::from_millis(15),
            cleanup: Duration::from_secs(1),
        },
    }
}
struct Sink {
    cancel: bool,
    seen: Arc<AtomicUsize>,
}
impl CodexSink for Sink {
    async fn push<'a>(
        &'a mut self,
        event: CodexEventView<'a>,
        original: &'a graphit_core::file_io::SnapshotContext,
    ) -> Result<(), SinkFailure> {
        original.check().unwrap();
        self.seen.fetch_add(1, Ordering::SeqCst);
        if self.cancel && matches!(event, CodexEventView::TurnStarted) {
            original.cancellation().cancel();
        }
        Ok(())
    }
}
async fn host() -> ProcessHost {
    let h = ProcessHost::install(&Cancellation::default(), HostLimits::new(1, 1).unwrap()).unwrap();
    assert!(matches!(
        h.ready(Instant::now() + Duration::from_secs(5)).await,
        HostReadiness::Ready
    ));
    h
}
fn receipt_mut<'a, 'n>(
    r: &'a mut Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>>,
) -> Option<&'a mut CodexReceipt<'n>> {
    match r {
        Ok(KnowledgeQuestionResult::Executed { receipt, .. }) => Some(receipt.as_mut()),
        Err(e) => match e.execution_mut()? {
            Ok(r) => Some(r),
            Err(e) => Some(e.receipt_mut()),
        },
        _ => None,
    }
}
async fn repair<'n>(r: &mut Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>>) {
    if let Some(receipt) = receipt_mut(r) {
        let remaining = match receipt.process_mut() {
            Some(Ok(r)) => r.remaining.as_mut(),
            Some(Err(e)) => e.remaining.as_mut(),
            None => None,
        };
        if let Some(r) = remaining {
            assert!(matches!(
                r.repair(Instant::now() + Duration::from_secs(5)).await,
                RepairObservation::Observed {
                    cleanup: Cleanup::Complete,
                    ..
                }
            ));
        }
    }
}
async fn shutdown(h: ProcessHost) {
    let report = match h.shutdown(Instant::now() + Duration::from_secs(5)).await {
        HostShutdown::Joined(r) => r,
        HostShutdown::Pending(mut r) => {
            match r.repair(Instant::now() + Duration::from_secs(5)).await {
                HostShutdownObservation::Joined(r) => r,
                _ => panic!("host pending"),
            }
        }
    };
    assert!(report.failure().is_none());
}
struct Transport<'a> {
    host: &'a ProcessHost,
    dir: &'a Path,
    mode: &'a str,
    late: bool,
    seen: Arc<AtomicUsize>,
    prompt_len: &'a AtomicUsize,
}
async fn invoke<'n>(
    ctx: &'n RequestContext<'_>,
    read: &ProjectReadAdmission<'_>,
    w: &'n CodexWireLimits,
    query: &str,
    selection: &[&str],
    l: KnowledgeQuestionLimits,
    transport: Transport<'_>,
) -> Result<KnowledgeQuestionResult<'n>, KnowledgeQuestionFailure<'n>> {
    let Transport {
        host: h,
        dir,
        mode,
        late,
        seen,
        prompt_len,
    } = transport;
    ask_knowledge_codex(
        ctx.snapshot(),
        read,
        query,
        selection,
        model(w),
        l,
        |mut run, prompt| async move {
            let original = run.original();
            assert!(Arc::ptr_eq(original, ctx.snapshot()));
            assert_eq!(run.admitted_attempts(), 0);
            let p: serde_json::Value = serde_json::from_str(&prompt).unwrap();
            assert_eq!(p["question"], query);
            assert_eq!(p["pages"].as_array().unwrap().len(), selection.len());
            for p in p["pages"].as_array().unwrap() {
                assert!(selection.contains(&p["slug"].as_str().unwrap()));
                if p["slug"] == "Paris" {
                    assert_eq!(p["body"], BODY);
                }
            }
            prompt_len.store(prompt.len(), Ordering::SeqCst);
            let mut sink = Sink {
                cancel: mode == "cancel",
                seen,
            };
            let r = run_codex(
                &mut run,
                h,
                process(dir, prompt, mode),
                AttemptPurpose::Request,
                None,
                CodexStreamLimits {
                    stdout_bytes: 32768,
                    stderr_bytes: 32768,
                },
                &mut sink,
            )
            .await;
            assert_eq!(run.admitted_attempts(), 1);
            if late {
                original.cancellation().cancel();
            }
            r
        },
    )
    .await
}
#[test]
fn corpus_selection_literal_prompt_and_completion_states() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(180));
        let w = wire();
        let empty = Fixture::new(&ctx);
        let issuer = empty.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&empty.secret(), &ctx).unwrap();
        let auth = ProjectRecordAuthority::new(&issuer);
        let read = auth
            .admit_authenticated_read(&caller, &ctx, empty.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let write = auth
            .admit_authenticated_write(
                &caller,
                &ctx,
                empty.scope(),
                ProjectWriteOperation::Knowledge,
            )
            .unwrap();
        let r = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("no corpus must not invoke") },
        )
        .await
        .unwrap();
        assert!(matches!(r, KnowledgeQuestionResult::NoCorpus));
        publish(&ctx, &write, &[]).await;
        let r = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("empty corpus must not invoke") },
        )
        .await
        .unwrap();
        assert!(matches!(r, KnowledgeQuestionResult::PublishedEmpty { .. }));
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        let read = auth
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        publish(&ctx, &write, &pages()).await;
        let expected = match read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
        {
            KnowledgeIndexRead::Published { generation, .. } => generation,
            _ => panic!("published"),
        };
        let r = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &[],
            model(&w),
            limits(),
            |_, _| async { panic!("empty selection must not invoke") },
        )
        .await
        .unwrap();
        assert!(
            matches!(r,KnowledgeQuestionResult::EmptySelection{generation}if generation==expected)
        );
        let h = host().await;
        let prompt_len = AtomicUsize::new(0);
        let query = "  Question\n\"β\"  ";
        for mode in [
            "answer",
            "missing",
            "empty",
            "malformed",
            "nonzero",
            "provider",
        ] {
            let mut r = invoke(
                &ctx,
                &read,
                &w,
                query,
                &["Paris"],
                limits(),
                Transport {
                    host: &h,
                    dir: &f.root,
                    mode,
                    late: false,
                    seen: Arc::new(AtomicUsize::new(0)),
                    prompt_len: &prompt_len,
                },
            )
            .await;
            if matches!(mode, "answer" | "missing" | "empty") {
                let (generation, provided, receipt, disposition) = match r.as_ref().unwrap() {
                    KnowledgeQuestionResult::Executed {
                        generation,
                        provided_pages,
                        receipt,
                        disposition,
                    } => (generation, provided_pages, receipt, disposition),
                    _ => panic!("executed"),
                };
                assert_eq!(generation, &expected);
                assert_eq!(provided.len(), 1);
                assert_eq!(provided[0].slug, "Paris");
                match mode {
                    "answer" => {
                        assert_eq!(*disposition, KnowledgeQuestionDisposition::Answer);
                        let a = match receipt.completion().unwrap() {
                            CompletionOutcome::Answer(a) => a,
                            _ => panic!("answer"),
                        };
                        assert_eq!(
                            a.text(),
                            format!("{query}\nCapital \"β\"|{BODY}\n[[outside-selection]]")
                        );
                        assert!(!a.text().contains("UNSELECTED"));
                        assert_eq!(a.end().ordinal(), 1);
                        assert_eq!(a.end().purpose(), AttemptPurpose::Request);
                        assert!(
                            matches!(a.end().usage(),UsageEvidence::Observed(u)if u.input_tokens==8&&u.output_tokens==3)
                        );
                    }
                    "missing" => {
                        assert_eq!(
                            *disposition,
                            KnowledgeQuestionDisposition::EvidenceUnavailable
                        );
                        assert!(matches!(
                            receipt.completion(),
                            Some(CompletionOutcome::EvidenceUnavailable(_))
                        ));
                    }
                    "empty" => {
                        assert_eq!(*disposition, KnowledgeQuestionDisposition::EmptyAnswer);
                        assert!(
                            matches!(receipt.completion(),Some(CompletionOutcome::Answer(a))if a.text()==" \n\t")
                        );
                    }
                    _ => unreachable!(),
                }
            } else {
                let e = r.as_ref().err().unwrap();
                assert!(matches!(e.error(), KnowledgeQuestionError::Codex));
                let native = e.execution().unwrap().as_ref().err().unwrap();
                assert!(native.receipt().process().is_some());
                let diagnostic = format!("{e:?} {e}");
                assert!(
                    !diagnostic.contains(query)
                        && !diagnostic.contains("fixture-model")
                        && !diagnostic.contains("UNSELECTED")
                );
            }
            repair(&mut r).await;
        }
        let exact = prompt_len.load(Ordering::SeqCst);
        let mut l = limits();
        l.max_prompt_bytes = n(exact);
        l.max_query_bytes = n(query.len());
        l.max_selected_pages = n(1);
        let mut r = invoke(
            &ctx,
            &read,
            &w,
            query,
            &["Paris"],
            l,
            Transport {
                host: &h,
                dir: &f.root,
                mode: "answer",
                late: false,
                seen: Arc::new(AtomicUsize::new(0)),
                prompt_len: &prompt_len,
            },
        )
        .await;
        assert!(r.is_ok());
        repair(&mut r).await;
        l.max_prompt_bytes = n(exact - 1);
        let e = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            query,
            &["Paris"],
            model(&w),
            l,
            |_, _| async { panic!("prompt cap must not invoke") },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(
            e.error(),
            KnowledgeQuestionError::Limit("prompt bytes")
        ));
        assert!(e.execution().is_none());
        let mut l = limits();
        l.max_selected_pages = n(2);
        let mut r = invoke(
            &ctx,
            &read,
            &w,
            query,
            &["Paris", "Database"],
            l,
            Transport {
                host: &h,
                dir: &f.root,
                mode: "answer",
                late: false,
                seen: Arc::new(AtomicUsize::new(0)),
                prompt_len: &prompt_len,
            },
        )
        .await;
        match r.as_ref().unwrap() {
            KnowledgeQuestionResult::Executed {
                generation,
                provided_pages,
                ..
            } => {
                assert_eq!(generation, &expected);
                assert_eq!(
                    provided_pages
                        .iter()
                        .map(|p| p.slug.as_str())
                        .collect::<Vec<_>>(),
                    ["Database", "Paris"]
                );
            }
            _ => panic!("both selected pages must execute in corpus order"),
        }
        repair(&mut r).await;
        shutdown(h).await;
    });
}
#[test]
fn authority_selection_and_store_limits_fail_before_completion() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(180));
        let f = Fixture::new(&ctx);
        let w = wire();
        let issuer = f.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        let read = auth
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        publish(&ctx, &write, &pages()).await;
        let mut cases = vec![
            ("", vec!["Paris"], limits()),
            ("q", vec!["Paris", "Paris"], limits()),
            ("q", vec!["Missing"], limits()),
            ("q", vec![""], limits()),
        ];
        let mut l = limits();
        l.max_query_bytes = n(1);
        cases.push(("qq", vec!["Paris"], l));
        let mut l = limits();
        l.max_selected_pages = n(1);
        cases.push(("q", vec!["Paris", "Database"], l));
        let mut l = limits();
        l.read.content.max_identifier_bytes = n(1);
        cases.push(("q", vec!["Paris"], l));
        for (case, (q, s, l)) in cases.into_iter().enumerate() {
            let e = ask_knowledge_codex(ctx.snapshot(), &read, q, &s, model(&w), l, |_, _| async {
                panic!("invalid input/read limit invoked")
            })
            .await
            .err()
            .unwrap();
            match case {
                0 => assert!(matches!(e.error(), KnowledgeQuestionError::InvalidQuestion)),
                1..=3 => assert!(matches!(
                    e.error(),
                    KnowledgeQuestionError::InvalidSelection
                )),
                4 => assert!(matches!(
                    e.error(),
                    KnowledgeQuestionError::Limit("query bytes")
                )),
                5 => assert!(matches!(
                    e.error(),
                    KnowledgeQuestionError::Limit("selected pages")
                )),
                6 => assert!(matches!(e.error(), KnowledgeQuestionError::Data(_))),
                _ => unreachable!(),
            }
            assert!(e.execution().is_none());
        }
        let other = Arc::new(graphit_core::file_io::SnapshotContext::new(
            ctx.snapshot().required_deadline().unwrap(),
            ctx.snapshot().cancellation().clone(),
        ));
        let e = ask_knowledge_codex(
            &other,
            &read,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("foreign invoked") },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(e.error(), KnowledgeQuestionError::ForeignRequest));
        let memory = auth
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
            .unwrap();
        let e = ask_knowledge_codex(
            ctx.snapshot(),
            &memory,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("wrong module invoked") },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(
            e.error(),
            KnowledgeQuestionError::InvalidAdmission
        ));
        let mut m = model(&w);
        m.completion_limits.max_graphit_attempts = 2;
        let e = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &["Paris"],
            m,
            limits(),
            |_, _| async { panic!("multiple calls invoked") },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(
            e.error(),
            KnowledgeQuestionError::InvalidAttemptLimit
        ));
        let mut m = model(&w);
        m.completion_limits.max_response_bytes = 0;
        let e = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &["Paris"],
            m,
            limits(),
            |_, _| async { panic!("AI invalid policy invoked") },
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(e.error(), KnowledgeQuestionError::Completion(_)));
        assert!(e.source().is_some());
        let directory = f.project.runtime_root().join("knowledge/index.lance");
        let backup = directory.with_extension("saved");
        std::fs::rename(&directory, &backup).unwrap();
        std::fs::write(&directory, b"invalid corpus shape").unwrap();
        let r = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("store failure invoked") },
        )
        .await;
        std::fs::remove_file(&directory).unwrap();
        std::fs::rename(&backup, &directory).unwrap();
        let e = r.err().unwrap();
        assert!(matches!(e.error(), KnowledgeQuestionError::Data(_)));
        assert!(e.source().is_some());
        assert!(e.execution().is_none());
        let denied = f.issuer(&ctx, &[Permission::Read(Module::Memory)]);
        let deniedcaller = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let deniedauth = ProjectRecordAuthority::new(&denied);
        assert!(
            deniedauth
                .admit_authenticated_read(
                    &deniedcaller,
                    &ctx,
                    f.scope(),
                    ProjectReadModule::Knowledge
                )
                .is_err()
        );
        cancel.cancel();
        let e = ask_knowledge_codex(
            ctx.snapshot(),
            &read,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("cancelled invoked") },
        )
        .await
        .err()
        .unwrap();
        assert!(
            matches!(e.error(),KnowledgeQuestionError::Request(e)if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(e.execution().is_none());
        let expired = Arc::new(graphit_core::file_io::SnapshotContext::new(
            Instant::now(),
            Cancellation::default(),
        ));
        let e = ask_knowledge_codex(
            &expired,
            &read,
            "q",
            &["Paris"],
            model(&w),
            limits(),
            |_, _| async { panic!("expired invoked") },
        )
        .await
        .err()
        .unwrap();
        assert!(
            matches!(e.error(),KnowledgeQuestionError::Request(e)if e.kind()==std::io::ErrorKind::TimedOut)
        );
    });
}
#[test]
fn cancellation_deadline_and_late_recheck_preserve_actual_execution() {
    native_test(async {
        let base = Cancellation::default();
        let ctx = request(&base, Duration::from_secs(180));
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let auth = ProjectRecordAuthority::new(&issuer);
        let write = auth
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        publish(&ctx, &write, &pages()).await;
        let h = host().await;
        let w = wire();
        let prompt_len = AtomicUsize::new(0);
        for mode in ["cancel", "timeout", "late"] {
            let cancel = Cancellation::default();
            let timeout = if mode == "timeout" {
                Duration::from_secs(5)
            } else {
                Duration::from_secs(30)
            };
            let ctx = request(&cancel, timeout);
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let read = auth
                .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
                .unwrap();
            let seen = Arc::new(AtomicUsize::new(0));
            let mut r = invoke(
                &ctx,
                &read,
                &w,
                "q",
                &["Paris"],
                limits(),
                Transport {
                    host: &h,
                    dir: &f.root,
                    mode: if mode == "late" { "answer" } else { mode },
                    late: mode == "late",
                    seen: seen.clone(),
                    prompt_len: &prompt_len,
                },
            )
            .await;
            assert!(
                seen.load(Ordering::SeqCst) > 0,
                "a real child callback must precede interruption"
            );
            let e = r.as_ref().err().unwrap();
            assert!(
                matches!(e.error(),KnowledgeQuestionError::Request(e)if e.kind()==if mode=="timeout"{std::io::ErrorKind::TimedOut}else{std::io::ErrorKind::Interrupted})
            );
            if mode == "late" {
                assert!(e.execution().unwrap().is_ok());
            } else {
                assert!(e.execution().unwrap().is_err());
            }
            assert!(receipt_mut(&mut r).unwrap().process().is_some());
            repair(&mut r).await;
        }
        shutdown(h).await;
    });
}

#[test]
fn prepared_question_rechecks_captured_original_before_invoking_transport() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Duration::from_secs(60));
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, &perms());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
            .unwrap();
        publish(&ctx, &write, &pages()).await;
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        let wire = wire();
        let prepared = prepare_knowledge_codex(
            ctx.snapshot(),
            &read,
            "literal",
            &["Paris"],
            model(&wire),
            limits(),
        )
        .await
        .unwrap();
        assert_eq!(prepared.documents().unwrap()[0].slug, "Paris");
        assert_eq!(prepared.documents().unwrap()[0].body, BODY);
        let calls = AtomicUsize::new(0);
        cancel.cancel();
        let result = prepared
            .execute(|_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { panic!("canceled captured Original cannot invoke transport") }
            })
            .await;
        let failure = result.err().expect("actual Original cancellation");
        assert!(
            matches!(failure.error(),KnowledgeQuestionError::Request(e)if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(failure.execution().is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    });
}
