use super::*;
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{
        ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput, QuerySource,
    },
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_model::ParsedJsonDocument,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::{ContextError, ContextName},
    tree_sitter_engine::{BuiltinGrammarError, EngineError},
    tree_sitter_model::{ParsedTreeSitterDocument, TsEntityCallable},
    tree_sitter_profile::TreeSitterProfileError,
};
use graphit_core::file_lock::Cancellation;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn catalog(bytes: &[u8], original: &SnapshotContext) -> CatalogDraft {
    let id = ProfileId::new("actual-resource-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let limits = ProfileLimits::new(ProfileLimitsInput {
        input_bytes: 1 << 20,
        nodes: 32768,
        depth: 64,
        text_bytes: 1 << 20,
        scalar_bytes: 32768,
        collection_entries: 4096,
    })
    .unwrap();
    let profiles = [decoder
        .decode(ProfileInput { id: &id, bytes }, &limits, original)
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &profiles,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        original,
    )
    .unwrap()
}

fn identity_limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 32,
        path_component_bytes: 4096,
        identity_text_bytes: 4096,
        owner_depth: 64,
        uid_bytes: 32768,
    })
    .unwrap()
}

fn with_input<T>(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult<T>,
) -> TestResult<T> {
    let prepared = text.to_owned();
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = [OriginSegment::Direct {
        prepared: span,
        original: span,
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

fn query_profile(data_key: &str, label: &str, pattern: &str) -> String {
    format!(
        "language: rust\ngrammar: tree-sitter-rust\ncontext_types: {{function_item: Function, struct_item: Struct}}\nqueries:\n  - data_key: {data_key}\n    graph_label: {label}\n    pattern: '{pattern}'\n"
    )
}

#[test]
fn linked_stock_rust_factory_is_reused_for_two_actual_files() -> TestResult {
    let resources = default_resources();
    let original = SnapshotContext::service(Cancellation::default());
    let draft = catalog(
        include_bytes!("../../../crates/graphit-ast/profiles/queries/rust.yaml"),
        &original,
    );
    let factory = resources.contexts(&draft.profiles()[0], &original)?;
    let files = tempfile::tempdir()?;
    for (path, name, text) in [
        ("first.rs", "alpha", "pub fn alpha() -> u32 { 1 }"),
        ("second.rs", "beta", "pub fn beta() -> u32 { 2 }"),
    ] {
        std::fs::write(files.path().join(path), text)?;
        let source = std::fs::read_to_string(files.path().join(path))?;
        with_input(&source, &original, |input| {
            let components = [path];
            let parsed =
                ParsedTreeSitterDocument::collect(&factory, input, &components, identity_limits())?;
            let native = parsed.context_document()?;
            let observations = native.observations()?;
            assert!(observations.groups.iter().any(|group| {
                group.query.value().data_key().value() == "functions"
                    && group.capture_names().contains(&"name")
                    && group.matches.iter().any(|matched| {
                        matched
                            .captures
                            .iter()
                            .any(|capture| capture.prepared.text() == name)
                    })
            }));
            assert!(native.contexts()?.iter().any(|context| {
                context.label == "Function"
                    && matches!(&context.name,
                    ContextName::Found { selected, .. } if selected.prepared.text() == name)
            }));
            let model = parsed.assemble()?;
            let entity = model
                .entities()?
                .iter()
                .find(|entity| entity.name() == name)
                .expect("actual Rust function");
            assert_eq!(entity.identity().label(), "Function");
            assert!(
                matches!(entity.callable(), TsEntityCallable::Function(details) if details.signature.syntax.is_some())
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn refreshed_query_factory_preserves_admitted_old_and_other_resource_factories() -> TestResult {
    let original = SnapshotContext::service(Cancellation::default());
    let old_yaml = query_profile(
        "old-functions",
        "Function",
        "(function_item name: (identifier) @name)",
    );
    let new_yaml = query_profile(
        "new-structs",
        "Struct",
        "(struct_item name: (type_identifier) @name)",
    );
    let old = catalog(old_yaml.as_bytes(), &original);
    let new = catalog(new_yaml.as_bytes(), &original);
    let resource_a = default_resources();
    let resource_b = default_resources();
    let old_a = resource_a.contexts(&old.profiles()[0], &original)?;
    let old_b = resource_b.contexts(&old.profiles()[0], &original)?;
    let fresh_a = resource_a.contexts(&new.profiles()[0], &original)?;
    // Each admitted factory retains its actual native language/query owners.
    drop(resource_a);
    with_input("fn old_function() {} struct Fresh;", &original, |input| {
        for factory in [&old_a, &old_b] {
            let document = factory.collect(input)?;
            let groups = &document.observations()?.groups;
            assert_eq!(groups[0].query.value().data_key().value(), "old-functions");
            assert!(groups[0].matches.iter().any(|matched| {
                matched
                    .captures
                    .iter()
                    .any(|capture| capture.prepared.text() == "old_function")
            }));
            assert!(!groups[0].matches.iter().any(|matched| {
                matched
                    .captures
                    .iter()
                    .any(|capture| capture.prepared.text() == "Fresh")
            }));
        }
        let document = fresh_a.collect(input)?;
        let groups = &document.observations()?.groups;
        assert_eq!(groups[0].query.value().data_key().value(), "new-structs");
        assert!(groups[0].matches.iter().any(|matched| {
            matched
                .captures
                .iter()
                .any(|capture| capture.prepared.text() == "Fresh")
        }));
        Ok(())
    })
}

#[test]
fn preparation_errors_and_real_request_cancellation_preserve_original_causes() -> TestResult {
    let resources = default_resources();
    let original = SnapshotContext::service(Cancellation::default());
    // Acquisition is the actual builtin loader, rather than a Rust/JSON table.
    // This proves the builtin query observations used by the syntax fallback.
    let go = catalog(b"language: go\ngrammar: tree-sitter-go\nqueries:\n  - data_key: functions\n    graph_label: Function\n    pattern: '(function_declaration name: (identifier) @name)'\n", &original);
    let acquired = resources.profile(&go.profiles()[0], &original)?;
    with_input(
        "package example\nfunc Available() {}\n",
        &original,
        |input| {
            let observations = acquired.collect_matches(input)?;
            assert!(observations.groups[0].matches.iter().any(|matched| {
                matched
                    .captures
                    .iter()
                    .any(|capture| capture.prepared.text() == "Available")
            }));
            Ok(())
        },
    )?;
    let unavailable = catalog(
        b"language: unknown\ngrammar: tree-sitter-not-distributed\n",
        &original,
    );
    let error = resources
        .contexts(&unavailable.profiles()[0], &original)
        .err()
        .expect("unacquired grammar refused");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(matches!(
        error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<BuiltinGrammarError>()),
        Some(BuiltinGrammarError::Unsupported)
    ));

    let invalid_yaml = query_profile("invalid", "Function", "(not_a_real_rust_node) @name");
    let invalid = catalog(invalid_yaml.as_bytes(), &original);
    let error = resources
        .profile(&invalid.profiles()[0], &original)
        .err()
        .expect("native query rejected");
    let cause = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<TreeSitterProfileError>())
        .expect("actual profile cause");
    assert!(matches!(
        cause,
        TreeSitterProfileError::Engine(EngineError::Query(_))
    ));
    assert!(cause.source().is_some());

    let antlr = catalog(
        b"language: rust\ngrammar: tree-sitter-rust\nparser: antlr4\n",
        &original,
    );
    let error = resources
        .contexts(&antlr.profiles()[0], &original)
        .err()
        .expect("parser mismatch refused");
    assert!(matches!(
        error
            .get_ref()
            .and_then(|error| error.downcast_ref::<ContextError>()),
        Some(ContextError::Profile(
            TreeSitterProfileError::ParserMismatch
        ))
    ));

    let source_query = catalog(b"language: rust\ngrammar: tree-sitter-rust\nqueries:\n  - data_key: functions\n    graph_label: Function\n    source: cobol-preprocessor\n    pattern: '(function_item name: (identifier) @name)'\n", &original);
    let error = resources
        .profile(&source_query.profiles()[0], &original)
        .err()
        .expect("unsupported query source refused");
    assert!(matches!(
        error
            .get_ref()
            .and_then(|error| error.downcast_ref::<TreeSitterProfileError>()),
        Some(TreeSitterProfileError::UnsupportedQuerySource {
            query_index: 0,
            source: QuerySource::CobolPreprocessor
        })
    ));

    let invalid_context = catalog(b"language: rust\ngrammar: tree-sitter-rust\ncontext_types: {not_a_native_context: Function}\n", &original);
    let error = resources
        .contexts(&invalid_context.profiles()[0], &original)
        .err()
        .expect("invalid native context refused");
    assert!(
        matches!(error.get_ref().and_then(|error| error.downcast_ref::<ContextError>()), Some(ContextError::UnknownContextKind { kind }) if kind == "not_a_native_context")
    );

    let cancel = Cancellation::default();
    let preparation = SnapshotContext::service(cancel.clone());
    let draft = catalog(
        query_profile(
            "functions",
            "Function",
            "(function_item name: (identifier) @name)",
        )
        .as_bytes(),
        &original,
    );
    let factory = resources.contexts(&draft.profiles()[0], &preparation)?;
    cancel.cancel();
    assert_eq!(
        resources
            .contexts(&draft.profiles()[0], &preparation)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    // Preparation does not retain its Original. A fresh input checks its own
    // genuine request, while cancellation of that request stops collection.
    with_input("fn fresh_input() {}", &original, |input| {
        assert!(
            factory.collect(input)?.observations()?.groups[0]
                .matches
                .iter()
                .any(|matched| matched
                    .captures
                    .iter()
                    .any(|capture| capture.prepared.text() == "fresh_input"))
        );
        Ok(())
    })?;
    let input_cancel = Cancellation::default();
    let input_original = SnapshotContext::service(input_cancel.clone());
    with_input("fn cancelled_input() {}", &input_original, |input| {
        input_cancel.cancel();
        assert!(
            matches!(factory.collect(input), Err(ContextError::Operation(error)) if error.kind() == io::ErrorKind::Interrupted)
        );
        Ok(())
    })
}

#[test]
fn linked_json_profile_collects_and_assembles_genuine_entities() -> TestResult {
    let resources = default_resources();
    let original = SnapshotContext::service(Cancellation::default());
    let draft = catalog(
        include_bytes!("../../../crates/graphit-ast/profiles/queries/json.yaml"),
        &original,
    );
    let factory = resources.contexts(&draft.profiles()[0], &original)?;
    with_input(r#"{"answer":42,"flag":true}"#, &original, |input| {
        let parsed =
            ParsedJsonDocument::collect(&factory, input, &["actual.json"], identity_limits())?;
        let model = parsed.assemble()?;
        for (name, raw_name) in [("answer", r#""answer""#), ("flag", r#""flag""#)] {
            let entity = model
                .entities()
                .iter()
                .find(|entity| entity.identity().label() == "Pair" && entity.name() == name)
                .expect("actual JSON Pair");
            assert_eq!(entity.name_raw().text(), raw_name);
            assert!(entity.conflicts().is_empty());
        }
        assert!(model.diagnostics().is_empty());
        Ok(())
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocking_thread_keeps_borrowed_rust_model_across_local_await() -> TestResult {
    let resources = default_resources();
    let handle = tokio::runtime::Handle::current();
    let worker = tokio::task::spawn_blocking(move || -> io::Result<String> {
        let original = SnapshotContext::service(Cancellation::default());
        let draft = catalog(
            include_bytes!("../../../crates/graphit-ast/profiles/queries/rust.yaml"),
            &original,
        );
        let factory = resources.contexts(&draft.profiles()[0], &original)?;
        let source = String::from("pub fn borrowed(value: u32) -> u32 { value }");
        let prepared = source.clone();
        let span = ByteSpan {
            start: 0,
            end: source.len() as u64,
        };
        let segments = [OriginSegment::Direct {
            prepared: span,
            original: span,
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            prepared.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )
        .map_err(io::Error::other)?;
        let parsed = ParsedTreeSitterDocument::collect(
            &factory,
            &input,
            &["borrowed.rs"],
            identity_limits(),
        )
        .map_err(io::Error::other)?;
        let model = parsed.assemble().map_err(io::Error::other)?;
        let borrowed_model = &model;
        handle.block_on(async {
            let entity = borrowed_model.entities().map_err(io::Error::other)?.iter().find(|entity| entity.name() == "borrowed").expect("actual borrowed Rust function");
            let TsEntityCallable::Function(details) = entity.callable() else { return Err(io::Error::other("missing genuine function details")); };
            let signature = details.signature.syntax.as_ref().expect("genuine local Syn signature");
            tokio::task::yield_now().await;
            original.check()?;
            assert_eq!(signature.ident.to_string(), "borrowed");
            assert!(borrowed_model.code_projections().map_err(io::Error::other)?.iter().any(|projection| matches!(projection, graphit_ast::tree_sitter_model::TsProjection::Available(code) if code.text == source)));
            // Only this owned Send result crosses the joined thread boundary.
            Ok(entity.name().to_owned())
        })
    });
    assert_eq!(worker.await??, "borrowed");
    Ok(())
}

fn default_resources() -> AstParserResources {
    AstParserResources::default()
}
