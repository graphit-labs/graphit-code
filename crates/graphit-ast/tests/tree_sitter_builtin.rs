use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_context::{ContextError, ContextSource, PreparedTreeSitterContexts},
    tree_sitter_engine::{BUILTIN_GRAMMAR_NAMES, EngineError, ParseControl, builtin},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use serde::Deserialize;
use std::{collections::BTreeSet, error::Error, fs, io::ErrorKind, path::PathBuf, time::Instant};

type TestResult = Result<(), Box<dyn Error>>;
#[derive(Deserialize)]
struct Sample {
    grammar: String,
    profile: Option<String>,
    source: String,
}
fn samples() -> Vec<Sample> {
    serde_json::from_str(include_str!("fixtures/tree_sitter_builtin/samples.json")).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}
fn catalog(
    name: &str,
    bytes: &[u8],
    original: &SnapshotContext,
) -> Result<CatalogDraft, Box<dyn Error>> {
    let id = ProfileId::new(name, 256)?;
    let decoder = ProfileDecoder::new(ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })?);
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
            })?,
            original,
        )
        .unwrap_or_else(|error| panic!("profile {name}: {error}"))];
    Ok(CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &decoded,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })?,
        original,
    )?)
}

fn validate_native_sample(sample: &Sample) -> TestResult {
    let language = builtin(&sample.grammar)?;
    let tree = language.parse(&sample.source, ParseControl::default())?;
    let query = language.compile_query("(_) @node")?;
    drop(language);
    if tree.has_syntax_errors() {
        let errors: Vec<_> = tree
            .nodes(ParseControl::default())?
            .into_iter()
            .filter(|n| n.error || n.missing)
            .map(|n| (n.kind, n.range))
            .collect();
        return Err(format!("positive sample has syntax errors: {errors:?}").into());
    }
    let nodes = tree.nodes(ParseControl::default())?;
    assert!(!nodes.is_empty());
    for (index, node) in nodes.iter().enumerate() {
        assert!(
            sample.source.get(node.range.clone()).is_some(),
            "UTF-8 byte range"
        );
        if let Some(parent) = node.parent {
            assert!(parent < index);
            assert!(nodes[parent].range.start <= node.range.start);
            assert!(nodes[parent].range.end >= node.range.end);
        }
    }
    let matches = tree.matches(&query, ParseControl::default())?;
    assert!(matches.iter().any(|m| !m.captures.is_empty()));
    for capture in matches.iter().flat_map(|m| &m.captures) {
        assert_eq!(
            sample.source.get(capture.range.clone()),
            Some(capture.text.as_str())
        );
    }
    Ok(())
}

#[test]
fn all_41_builtin_grammars_parse_real_positive_samples_and_retain_native_queries() -> TestResult {
    let fixtures = samples();
    assert_eq!(fixtures.len(), 41);
    let fixture_names: BTreeSet<_> = fixtures.iter().map(|s| s.grammar.as_str()).collect();
    let builtin_names: BTreeSet<_> = BUILTIN_GRAMMAR_NAMES.iter().copied().collect();
    assert_eq!(fixture_names, builtin_names);
    let mut failures = Vec::new();
    for sample in fixtures {
        let result = validate_native_sample(&sample);
        if let Err(error) = result {
            failures.push(format!("{}: {error}", sample.grammar));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

fn validate_shipped_profile(sample: &Sample, anonymous: Option<&str>) -> TestResult {
    let name = sample
        .profile
        .as_ref()
        .ok_or("sample has no shipped profile")?;
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/graphit-ast/profiles/queries");
    let original = context();
    let draft = catalog(
        name,
        &fs::read(root.join(format!("{name}.yaml")))?,
        &original,
    )?;
    assert_eq!(draft.profiles().len(), 1);
    let profile = &draft.profiles()[0];
    assert_eq!(profile.grammar().value(), &sample.grammar);
    let prepared = PreparedTreeSitterContexts::prepare(
        profile,
        builtin(profile.grammar().value())?,
        &original,
    )?;
    let bound = ByteSpan {
        start: 0,
        end: sample.source.len().try_into()?,
    };
    let segments = [OriginSegment::Direct {
        prepared: bound,
        original: bound,
    }];
    let input = PreparedInput::new(
        sample.source.as_bytes(),
        sample.source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let document = prepared.collect(&input)?;
    drop(prepared);
    let observations = document.observations()?;
    assert!(
        observations
            .groups
            .iter()
            .any(|g| g.matches.iter().any(|m| !m.captures.is_empty())),
        "positive profile capture"
    );
    for capture in observations
        .groups
        .iter()
        .flat_map(|g| &g.matches)
        .flat_map(|m| &m.captures)
    {
        let range = capture.native.range.clone();
        assert_eq!(capture.prepared.text(), &sample.source[range.clone()]);
        let OriginalProjection::Exact(source) = &capture.original else {
            panic!("identity origin changed");
        };
        assert_eq!(source.text().as_ptr(), sample.source[range].as_ptr());
    }
    if let Some(kind) = anonymous {
        assert!(
            document
                .configuration()?
                .applicable_anon_types
                .iter()
                .any(|name| name == kind)
        );
        assert!(
            document
                .contexts()?
                .iter()
                .any(|context| context.source == ContextSource::Anonymous
                    && observations.nodes[context.trigger.node_index].kind == kind),
            "{name}: native closure did not participate in context selection"
        );
    }
    document.contexts()?;
    document.scopes()?;
    document.diagnostics()?;
    Ok(())
}

#[test]
fn all_40_shipped_tree_sitter_profiles_prepare_real_queries_contexts_and_source_captures()
-> TestResult {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/graphit-ast/profiles/queries");
    let files: Vec<_> = fs::read_dir(&root)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        files
            .iter()
            .filter(|e| e.path().extension().is_some_and(|x| x == "yaml"))
            .count(),
        45
    );
    let mut failures = Vec::new();
    let mut checked = 0;
    for sample in samples() {
        let Some(name) = &sample.profile else {
            continue;
        };
        checked += 1;
        let result = validate_shipped_profile(&sample, None);
        if let Err(error) = result {
            failures.push(format!("{name}: {error}"));
        }
    }
    assert_eq!(checked, 40);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn builtin_preserves_original_operation_causes_and_sdk_query_errors() -> TestResult {
    let language = builtin("tree-sitter-rust")?;
    let cancelled = context();
    cancelled.cancellation().cancel();
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    for (original, kind) in [
        (&cancelled, ErrorKind::Interrupted),
        (&expired, ErrorKind::TimedOut),
    ] {
        let result = language.parse_with_context("fn f() {}", ParseControl::default(), original);
        assert!(matches!(result, Err(EngineError::Operation(ref error)) if error.kind() == kind));
    }
    let error = language
        .compile_query("(nonexistent_graphit_node) @name")
        .err()
        .ok_or("invalid native query accepted")?;
    assert!(matches!(error, EngineError::Query(_)));
    assert!(error.source().is_some());
    Ok(())
}

#[test]
fn corrected_closure_profiles_and_kotlin_preserve_actual_native_semantics() -> TestResult {
    let corrections = [
        ("julia", "arrow_function_expression", "lambda_expression"),
        ("lua", "function_definition", "function_body"),
        ("objc", "block_literal", "block_expression"),
    ];
    let fixtures = samples();
    for (name, kind, old) in corrections {
        let sample = fixtures
            .iter()
            .find(|sample| sample.profile.as_deref() == Some(name))
            .ok_or("corrected profile fixture missing")?;
        validate_native_sample(sample)?;
        validate_shipped_profile(sample, Some(kind))?;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../crates/graphit-ast/profiles/queries/{name}.yaml"
        ));
        let bytes = fs::read_to_string(path)?;
        let configured = format!("  - {kind}");
        assert_eq!(bytes.matches(&configured).count(), 1);
        let invalid = bytes.replacen(&configured, &format!("  - {old}"), 1);
        let original = context();
        let draft = catalog(name, invalid.as_bytes(), &original)?;
        let result = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin(&sample.grammar)?,
            &original,
        );
        assert!(
            matches!(result, Err(ContextError::UnknownAnonymousKind { kind }) if kind == old),
            "{name}: invalid anonymous kind was concealed"
        );
    }
    let kotlin = fixtures
        .iter()
        .find(|sample| sample.grammar == "tree-sitter-kotlin")
        .ok_or("Kotlin fixture missing")?;
    validate_native_sample(kotlin)?;
    validate_shipped_profile(kotlin, None)?;
    Ok(())
}
