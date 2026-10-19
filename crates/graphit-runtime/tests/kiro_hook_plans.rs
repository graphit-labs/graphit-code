use graphit_core::file_lock::Cancellation;
use graphit_hub::{
    native::{
        ContentError, ContentLimits, DocumentError, DocumentFormat, FieldValue, NativeBinding,
        NativeDocument,
        kiro_hooks::{
            KiroHookAction, KiroHookError, KiroHookInput, KiroHookPlan, KiroHookTrigger,
            plan_kiro_hook,
        },
    },
    policy::{RequestContext, RequestError, SharedWriteLimits},
};
use graphit_runtime::{
    instructions::{
        InstructionErrorKind, InstructionLimit, InstructionLimits, InstructionOrigin,
        InstructionValue, InstructionVariable, RenderedInstruction, render_instruction,
    },
    policy_instructions::{PolicyPrompt, policy_prompt},
};
use serde_json::Value;
use std::{
    error::Error,
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, Instant},
};

// Finite packets already exercised by the renderer and hook planner producers.
fn instruction_limits() -> InstructionLimits {
    InstructionLimits {
        max_templates: NonZeroUsize::new(32).unwrap(),
        max_template_bytes: NonZeroUsize::new(65536).unwrap(),
        max_values: NonZeroUsize::new(128).unwrap(),
        max_variable_bytes: NonZeroUsize::new(16384).unwrap(),
        max_output_bytes: NonZeroUsize::new(65536).unwrap(),
        fuel: NonZeroU64::new(100000).unwrap(),
        recursion: NonZeroUsize::new(32).unwrap(),
    }
}

fn shared_limits() -> SharedWriteLimits {
    SharedWriteLimits {
        input_bytes: 8192,
        output_bytes: 8192,
        journal_bytes: 8192,
        nodes: 128,
        depth: 16,
        scalar_bytes: 1024,
        entries: 64,
        hook_tuples: 1,
        references: 1,
        documents: 1,
    }
}

fn content_limits() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 8192,
        max_output_bytes: 8192,
        max_depth: 16,
        max_nodes: 128,
        max_scalar_bytes: 1024,
        max_collection_entries: 64,
    }
}

fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        content_limits(),
        cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}

fn variables(brand: &str) -> [InstructionVariable<'_>; 1] {
    [InstructionVariable {
        name: "brand_display",
        value: InstructionValue::Text(brand),
    }]
}

fn rendered(
    vars: &[InstructionVariable<'_>],
    limits: InstructionLimits,
    original: &RequestContext<'_>,
) -> Result<RenderedInstruction, graphit_runtime::instructions::InstructionError> {
    let set = policy_prompt(PolicyPrompt::CoreInvariant);
    render_instruction(
        set.entry(),
        set.candidates(),
        vars,
        limits,
        original.snapshot(),
    )
}

fn prompt(body: &str) -> KiroHookInput<'_> {
    KiroHookInput {
        identifier: "routing-invariant",
        trigger: KiroHookTrigger::Stop,
        matcher: None,
        action: KiroHookAction::AgentPrompt(body),
        timeout: None,
    }
}

fn encoded(
    plan: &KiroHookPlan,
    limits: SharedWriteLimits,
    original: &RequestContext<'_>,
) -> String {
    let mut document =
        NativeDocument::parse(DocumentFormat::Json, b"{}", limits, original).unwrap();
    {
        let version = plan.version();
        document
            .set(&["version"], FieldValue::String(version.into()))
            .unwrap();
    }
    document
        .set(
            plan.array_path(),
            FieldValue::Array(vec![plan.tuple().clone()]),
        )
        .unwrap();
    document.render().unwrap()
}

#[test]
fn canonical_policy_bodies_roundtrip_with_effective_brand_and_literal_values() {
    for brand in [
        "Graphit",
        "Órbita \"literal\" ' $HOME %PATH% `text`\n{{ authored }} ${native}",
    ] {
        let cancel = Cancellation::default();
        let original = request(&cancel);
        // These are supplied text values, not proof of catalog existence or authority.
        let vars = variables(brand);
        let body = rendered(&vars, instruction_limits(), &original).unwrap();
        assert_eq!(body.origin(), InstructionOrigin::CompiledDefault);
        assert!(body.body().contains(brand));
        assert!(body.body().ends_with('\n'));

        // The active body needs its scalar budget; the outer packet stays capped.
        let mut shared = shared_limits();
        shared.scalar_bytes = body.body().len();
        let plan = plan_kiro_hook(
            NativeBinding::KiroV3,
            prompt(body.body()),
            shared,
            &original,
        )
        .unwrap();
        assert_eq!(plan.binding(), NativeBinding::KiroV3);
        assert_eq!(plan.identifier(), "routing-invariant");
        assert_eq!(plan.trigger(), KiroHookTrigger::Stop);
        assert_eq!(plan.native_trigger(), "Stop");
        assert_eq!(plan.array_path(), &["hooks"]);
        assert_eq!(plan.version(), "v1");

        let json: Value = serde_json::from_str(&encoded(&plan, shared, &original)).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 2);
        assert_eq!(json["version"], "v1");
        let hooks = json["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        let hook = &hooks[0];
        assert_eq!(hook.as_object().unwrap().len(), 3);
        assert_eq!(hook["name"], plan.identifier());
        assert_eq!(hook["trigger"], plan.native_trigger());
        assert_eq!(hook["action"]["type"], "agent");
        assert_eq!(
            hook["action"]["prompt"].as_str().unwrap().as_bytes(),
            body.body().as_bytes()
        );
        assert!(hook.get("matcher").is_none());
        assert!(hook.get("timeout").is_none());
        assert!(!format!("{plan:?}").contains(body.body()));
    }
}

#[test]
fn foreign_binding_rejects_a_canonical_agent_prompt() {
    let cancel = Cancellation::default();
    let original = request(&cancel);
    let vars = variables("Graphit");
    let body = rendered(&vars, instruction_limits(), &original).unwrap();
    let error = plan_kiro_hook(
        NativeBinding::Codex,
        prompt(body.body()),
        shared_limits(),
        &original,
    )
    .unwrap_err();
    assert_eq!(error, KiroHookError::UnsupportedBinding);
    assert!(!error.to_string().contains(body.body()));
    original.snapshot().check().unwrap();
}

#[test]
fn cancellation_after_render_reaches_the_original_planner_request() {
    let cancel = Cancellation::default();
    let original = request(&cancel);
    let vars = variables("Graphit");
    let body = rendered(&vars, instruction_limits(), &original).unwrap();
    cancel.cancel();
    let error = plan_kiro_hook(
        NativeBinding::KiroV3,
        prompt(body.body()),
        shared_limits(),
        &original,
    )
    .unwrap_err();
    assert_eq!(error, KiroHookError::Document(DocumentError::Interrupted));
    assert_eq!(
        error.source().unwrap().downcast_ref::<DocumentError>(),
        Some(&DocumentError::Interrupted)
    );
    assert_eq!(
        original.snapshot().check().unwrap_err().kind(),
        std::io::ErrorKind::Interrupted
    );
}

#[test]
fn expired_request_and_missing_required_brand_do_not_produce_a_hook_body() {
    let cancel = Cancellation::default();
    let expired = RequestContext::new(
        content_limits(),
        &cancel,
        Instant::now() - Duration::from_secs(1),
    );
    assert!(matches!(
        expired,
        Err(RequestError::Content(ContentError::TimedOut))
    ));

    let original = request(&cancel);
    let vars = variables("Authored display");
    let error = rendered(&vars[..0], instruction_limits(), &original).unwrap_err();
    assert!(matches!(error.kind(), InstructionErrorKind::Template(_)));
    assert!(error.source().is_some());
    assert!(!error.to_string().contains("Authored display"));
}

#[test]
fn utf8_body_and_native_output_limits_have_exact_boundaries() {
    let cancel = Cancellation::default();
    let original = request(&cancel);
    let vars = variables("Órbita");
    let body = rendered(&vars, instruction_limits(), &original).unwrap();
    assert!(body.body().len() > body.body().chars().count());

    let mut instruction = instruction_limits();
    instruction.max_output_bytes = NonZeroUsize::new(body.body().len()).unwrap();
    assert_eq!(
        rendered(&vars, instruction, &original).unwrap().body(),
        body.body()
    );
    instruction.max_output_bytes = NonZeroUsize::new(body.body().len() - 1).unwrap();
    assert_eq!(
        rendered(&vars, instruction, &original).unwrap_err().kind(),
        InstructionErrorKind::LimitExceeded(InstructionLimit::OutputBytes)
    );

    let mut shared = shared_limits();
    shared.scalar_bytes = body.body().len();
    let plan = plan_kiro_hook(
        NativeBinding::KiroV3,
        prompt(body.body()),
        shared,
        &original,
    )
    .unwrap();
    let expected = encoded(&plan, shared, &original);
    shared.output_bytes = expected.len();
    let exact = plan_kiro_hook(
        NativeBinding::KiroV3,
        prompt(body.body()),
        shared,
        &original,
    )
    .unwrap();
    assert_eq!(encoded(&exact, shared, &original), expected);

    shared.output_bytes -= 1;
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            prompt(body.body()),
            shared,
            &original,
        )
        .unwrap_err(),
        KiroHookError::Document(DocumentError::LimitExceeded)
    );
    shared = shared_limits();
    shared.scalar_bytes = body.body().len() - 1;
    assert_eq!(
        plan_kiro_hook(
            NativeBinding::KiroV3,
            prompt(body.body()),
            shared,
            &original,
        )
        .unwrap_err(),
        KiroHookError::Document(DocumentError::LimitExceeded)
    );
}
