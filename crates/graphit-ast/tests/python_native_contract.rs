use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_context::{ContextName, PreparedTreeSitterContexts},
    tree_sitter_engine::builtin,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn native_python_fields_scopes_and_queries_precede_the_semantic_model() -> TestResult {
    let original = context();
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
    ));
    let source = include_str!("fixtures/python/native_roles.py");
    with_source(source, &original, |input| {
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin(draft.profiles()[0].grammar().value())?,
            &original,
        )?;
        let document = factory.collect(input)?;
        let observations = document.observations()?;
        let nodes = &observations.nodes;
        assert!(nodes.iter().all(|n| !n.error && !n.missing));
        let field = |parent: usize, name: &str| {
            nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.parent == Some(parent) && n.field.as_deref() == Some(name))
                .map(|(i, _)| i)
        };
        let text = |index: usize| &source[nodes[index].range.clone()];
        let named = |kind: &str, name: &str| {
            nodes
                .iter()
                .enumerate()
                .find(|(i, n)| n.kind == kind && field(*i, "name").is_some_and(|i| text(i) == name))
                .map(|(i, _)| i)
                .unwrap()
        };
        let class = named("class_definition", "Child");
        let wrapper = nodes[class].parent.unwrap();
        assert_eq!(nodes[wrapper].kind, "decorated_definition");
        assert_eq!(field(wrapper, "definition"), Some(class));
        assert_eq!(nodes[field(class, "body").unwrap()].kind, "block");
        assert_eq!(
            nodes[field(class, "superclasses").unwrap()].kind,
            "argument_list"
        );
        assert!(text(wrapper).starts_with("@mark\nclass Child"));
        let run = named("function_definition", "run");
        let parameters = field(run, "parameters").unwrap();
        let body = field(run, "body").unwrap();
        assert_eq!(nodes[parameters].kind, "parameters");
        assert_eq!(nodes[body].kind, "block");
        assert_eq!(text(field(run, "return_type").unwrap()), "int");
        assert!(nodes[run].range.start < nodes[parameters].range.start);
        assert!(nodes[parameters].range.end < nodes[body].range.start);
        let parameter = |kind: &str| {
            nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.parent == Some(parameters) && n.kind == kind)
                .map(|(i, _)| i)
                .unwrap()
        };
        assert_eq!(text(parameter("identifier")), "self");
        let typed = parameter("typed_parameter");
        assert_eq!(text(field(typed, "type").unwrap()), "int");
        assert!(
            nodes
                .iter()
                .enumerate()
                .any(|(i, n)| n.parent == Some(typed) && n.kind == "identifier" && text(i) == "x")
        );
        let default = parameter("default_parameter");
        assert_eq!(text(field(default, "name").unwrap()), "y");
        assert_eq!(text(field(default, "value").unwrap()), "2");
        let typed_default = parameter("typed_default_parameter");
        assert_eq!(text(field(typed_default, "name").unwrap()), "z");
        assert_eq!(text(field(typed_default, "type").unwrap()), "str");
        assert_eq!(text(field(typed_default, "value").unwrap()), "\"a\"");
        let assignment = nodes
            .iter()
            .enumerate()
            .find(|(i, n)| {
                n.kind == "assignment" && field(*i, "left").is_some_and(|i| text(i) == "BASE")
            })
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(text(field(assignment, "right").unwrap()), "1");
        let alias = named("aliased_import", "package");
        assert_eq!(text(field(alias, "alias").unwrap()), "alias");
        let attribute = nodes
            .iter()
            .enumerate()
            .find(|(i, n)| {
                n.kind == "attribute" && field(*i, "attribute").is_some_and(|i| text(i) == "value")
            })
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(text(field(attribute, "object").unwrap()), "self");
        let lambda = nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.kind == "lambda")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(
            nodes[field(lambda, "parameters").unwrap()].kind,
            "lambda_parameters"
        );
        assert_eq!(nodes[field(lambda, "body").unwrap()].kind, "call");
        let base = named("class_definition", "Base");
        let base_body = field(base, "body").unwrap();
        let first = nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.parent == Some(base_body) && n.named)
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(nodes[first].kind, "expression_statement");
        assert!(
            nodes
                .iter()
                .enumerate()
                .any(|(i, n)| n.parent == Some(first)
                    && n.kind == "string"
                    && text(i).contains("Authored multiline"))
        );
        assert_eq!(observations.groups.len(), 16);
        assert!(
            observations
                .groups
                .iter()
                .filter(|g| !g.matches.is_empty())
                .count()
                >= 12
        );
        let contexts = document.contexts()?;
        assert!(contexts.iter().any(|c| c.label == "Class"
            && matches!(&c.name, ContextName::Found {selected, ..}
                if text(selected.node_index) == "Child")));
        for kind in [
            "for_statement",
            "except_clause",
            "with_item",
            "global_statement",
            "list_comprehension",
        ] {
            assert!(
                nodes.iter().any(|n| n.kind == kind),
                "missing actual native role {kind}"
            );
        }
        Ok(())
    })
}

fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}
fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("model-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let decoded = [decoder
        .decode(
            ProfileInput { id: &id, bytes },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            &original,
        )
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &decoded,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        &original,
    )
    .unwrap()
}
fn with_source(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = if text.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: span,
            original: span,
        }]
    };
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

#[test]
fn native_python_type_parameters_and_assignment_annotations_are_real_fields() -> TestResult {
    let original = context();
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
    ));
    let source =
        "class Box[T]: pass\ndef identity[T](value: T) -> T: return value\nitems: list[int] = []\n";
    with_source(source, &original, |input| {
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin(draft.profiles()[0].grammar().value())?,
            &original,
        )?;
        let document = factory.collect(input)?;
        let nodes = &document.observations()?.nodes;
        assert!(nodes.iter().all(|n| !n.error && !n.missing));
        for kind in ["class_definition", "function_definition"] {
            let index = nodes.iter().position(|n| n.kind == kind).unwrap();
            let field = nodes
                .iter()
                .find(|n| n.parent == Some(index) && n.field.as_deref() == Some("type_parameters"))
                .unwrap();
            assert_eq!(&source[field.range.clone()], "[T]");
        }
        let assignment = nodes.iter().position(|n| n.kind == "assignment").unwrap();
        let field = nodes
            .iter()
            .find(|n| n.parent == Some(assignment) && n.field.as_deref() == Some("type"))
            .unwrap();
        assert_eq!(&source[field.range.clone()], "list[int]");
        Ok(())
    })
}
