use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_context::{ContextName, PreparedTreeSitterContexts},
    tree_sitter_engine::builtin,
    tree_sitter_profile::CaptureBinding,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{collections::BTreeMap, error::Error};
type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn native_ecma_binding_roles_prove_fields_and_boundaries_before_local_resolution() -> TestResult {
    let original = context();
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/javascript.yaml"
    ));
    let source = include_str!("fixtures/ecma/binding_scopes.js");
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
        let declarator = |name: &str| {
            nodes
                .iter()
                .enumerate()
                .find(|(i, n)| {
                    n.kind == "variable_declarator"
                        && field(*i, "name").is_some_and(|i| text(i) == name)
                })
                .map(|(i, _)| i)
                .unwrap()
        };
        let before = declarator("before");
        assert_eq!(
            nodes[nodes[before].parent.unwrap()].kind,
            "lexical_declaration"
        );
        assert_eq!(
            nodes[field(before, "value").unwrap()].kind,
            "arrow_function"
        );
        let calls = nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                n.kind == "call_expression"
                    && field(*i, "function").is_some_and(|i| text(i) == "before")
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert!(nodes[calls[0]].range.start < nodes[before].range.start);
        assert!(nodes[calls[1]].range.start > nodes[before].range.end);
        let block = |mut index: usize| {
            loop {
                if nodes[index].kind == "statement_block" {
                    break Some(index);
                }
                match nodes[index].parent {
                    Some(parent) => index = parent,
                    None => break None,
                }
            }
        };
        assert_eq!(block(calls[0]), block(calls[1]));
        assert_eq!(block(calls[0]), block(before));
        let hoisted = declarator("hoisted");
        assert_eq!(
            nodes[nodes[hoisted].parent.unwrap()].kind,
            "variable_declaration"
        );
        let alias = nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.kind == "import_specifier")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(text(field(alias, "name").unwrap()), "outside");
        assert_eq!(text(field(alias, "alias").unwrap()), "Alias");
        let namespace = nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.kind == "namespace_import")
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            nodes
                .iter()
                .enumerate()
                .any(|(i, n)| n.parent == Some(namespace)
                    && n.kind == "identifier"
                    && text(i) == "ns")
        );
        let pattern = nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.kind == "pair_pattern")
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(text(field(pattern, "key").unwrap()), "nested");
        assert_eq!(text(field(pattern, "value").unwrap()), "binding");
        let rest = nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.kind == "rest_pattern")
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            nodes.iter().enumerate().any(|(i, n)| n.parent == Some(rest)
                && n.kind == "identifier"
                && text(i) == "rest")
        );
        let fields = nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                n.kind == "field_definition"
                    && field(*i, "property").is_some_and(|i| text(i) == "field")
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(fields.len(), 2);
        let is_static = |parent: usize| {
            nodes
                .iter()
                .any(|n| n.parent == Some(parent) && n.kind == "static" && !n.named)
        };
        assert!(!is_static(fields[0]));
        assert!(is_static(fields[1]));
        for (name, expected_static) in [("instance", false), ("staticMethod", true)] {
            let method = nodes
                .iter()
                .enumerate()
                .find(|(i, n)| {
                    n.kind == "method_definition"
                        && field(*i, "name").is_some_and(|i| text(i) == name)
                })
                .map(|(i, _)| i)
                .unwrap();
            assert_eq!(is_static(method), expected_static);
        }
        let lexical = field(declarator("lexical"), "value").unwrap();
        assert_eq!(nodes[lexical].kind, "arrow_function");
        let dynamic = nodes
            .iter()
            .enumerate()
            .find(|(i, n)| {
                n.kind == "function_declaration"
                    && field(*i, "name").is_some_and(|i| text(i) == "dynamic")
            })
            .map(|(i, _)| i)
            .unwrap();
        assert_ne!(nodes[lexical].kind, nodes[dynamic].kind);
        assert!(
            nodes
                .iter()
                .any(|n| n.kind == "augmented_assignment_expression")
        );
        assert!(nodes.iter().any(|n| n.kind == "assignment_expression"));
        let legacy = nodes
            .iter()
            .enumerate()
            .find(|(i, n)| {
                n.kind == "function_declaration"
                    && field(*i, "name").is_some_and(|i| text(i) == "Legacy")
            })
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(nodes[nodes[legacy].parent.unwrap()].kind, "statement_block");
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
fn native_ecma_concrete_fixtures_retain_context_and_capture_contracts() -> TestResult {
    for (path, source, profile, language) in [
        (
            "component.tsx",
            include_str!("fixtures/ecma/component.tsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/tsx.yaml").as_slice(),
            "tsx",
        ),
        (
            "component.jsx",
            include_str!("fixtures/ecma/component.jsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            "javascript",
        ),
        (
            "declarations.ts",
            include_str!("fixtures/ecma/declarations.ts"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/typescript.yaml")
                .as_slice(),
            "typescript",
        ),
        (
            "names.js",
            include_str!("fixtures/ecma/names.js"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            "javascript",
        ),
        (
            "local.js",
            include_str!("fixtures/ecma/local.js"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/javascript.yaml")
                .as_slice(),
            "javascript",
        ),
        (
            "recovery.tsx",
            include_str!("fixtures/ecma/recovery.tsx"),
            include_bytes!("../../../crates/graphit-ast/profiles/queries/tsx.yaml").as_slice(),
            "tsx",
        ),
    ] {
        let original = context();
        let draft = catalog(profile);
        assert_eq!(draft.profiles()[0].language().value(), language);
        with_source(source, &original, |input| {
            let factory = PreparedTreeSitterContexts::prepare(
                &draft.profiles()[0],
                builtin(draft.profiles()[0].grammar().value())?,
                &original,
            )?;
            let document = factory.collect(input)?;
            let observations = document.observations()?;
            let mut families = BTreeMap::new();
            for (query_index, group) in observations.groups.iter().enumerate() {
                let query = group.query.value();
                for (match_index, matched) in group.matches.iter().enumerate() {
                    if let CaptureBinding::Present(names) = &matched.bindings.name {
                        for &capture_index in names {
                            let scope = document
                                .scopes()?
                                .iter()
                                .find(|scope| {
                                    scope.query_index == query_index
                                        && scope.match_index == match_index
                                        && scope.capture_index == capture_index
                                })
                                .expect("actual captured scope");
                            let node = &observations.nodes[scope.capture_node_index];
                            *families
                                .entry((
                                    query.graph_label().value().clone(),
                                    query.data_key().value().clone(),
                                ))
                                .or_insert(0usize) += 1;
                            assert_eq!(
                                node.node_id,
                                matched.captures[capture_index].native.node_id
                            );
                            let frames: Vec<_> = scope.contexts.iter().map(|&index| {
                                let context = &document.contexts().unwrap()[index];
                                let name = match &context.name {
                                    ContextName::Found {selected,..} => match &selected.original {
                                        graphit_ast::prepared_input::OriginalProjection::Exact(range) => Some(range.text()),
                                        _ => None,
                                    },
                                    ContextName::Missing{..} => None,
                                };
                                (context.label.as_str(), context.owner.as_ref().map(|owner| owner.node_index), name)
                            }).collect();
                            let raw_name = match &matched.captures[capture_index].original {
                                graphit_ast::prepared_input::OriginalProjection::Exact(range) => {
                                    Some(range.text())
                                }
                                _ => None,
                            };
                            println!(
                                "{path} {} {} {raw_name:?} native={} index={} parent={:?} field={:?} scopes={frames:?} unresolved={:?} excluded={:?}",
                                query.graph_label().value(),
                                query.data_key().value(),
                                node.kind,
                                scope.capture_node_index,
                                node.parent,
                                node.field,
                                scope.unresolved,
                                scope.excluded_self
                            );
                        }
                    }
                }
            }
            assert!(!families.is_empty());
            println!("{path} families={families:?}");
            Ok(())
        })?;
    }
    Ok(())
}
