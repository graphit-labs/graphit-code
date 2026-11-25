use graphit_ast::profiles::*;
use graphit_ast::scalar_policy::{
    RegexLimits, ScalarAudit, ScalarKind, ScalarLimits, ScalarPolicy,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::time::{Duration, Instant};

macro_rules! corpus { ($($name:literal),* $(,)?) => { [$(
    ($name, include_bytes!(concat!("fixtures/profiles/shipped/", $name, ".yaml")).as_slice()),
)*] }; }
const CORPUS: [(&str, &[u8]); 45] = corpus!(
    "bash",
    "c",
    "clojure",
    "cobol85",
    "cpp",
    "csharp",
    "css",
    "dart",
    "db2",
    "dockerfile",
    "elixir",
    "go",
    "graphql",
    "groovy",
    "haskell",
    "hcl",
    "html",
    "java",
    "javascript",
    "json",
    "julia",
    "kotlin",
    "lua",
    "objc",
    "php",
    "plpgsql",
    "plsql",
    "postgresql",
    "protobuf",
    "python",
    "r",
    "ruby",
    "rust",
    "scala",
    "sql",
    "svelte",
    "swift",
    "toml",
    "tsql",
    "tsx",
    "typescript",
    "vue",
    "xml",
    "yaml",
    "zig",
);

fn policy() -> ScalarPolicy {
    ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })
    .unwrap()
}
fn decoder() -> ProfileDecoder {
    ProfileDecoder::new(policy())
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        Cancellation::default(),
    )
}
fn input_limits() -> ProfileLimitsInput {
    ProfileLimitsInput {
        input_bytes: 32768,
        nodes: 2048,
        depth: 16,
        text_bytes: 32768,
        scalar_bytes: 4096,
        collection_entries: 256,
    }
}
fn limits() -> ProfileLimits {
    ProfileLimits::new(input_limits()).unwrap()
}

fn label(name: &str) -> ProfileId {
    ProfileId::new(name, 256).unwrap()
}
fn text(field: Presence<'_, String>) -> Option<&str> {
    match field {
        Presence::Set(value) => Some(value.value().as_str()),
        Presence::Missing => None,
        Presence::Clear { .. } => panic!("unexpected Clear"),
    }
}
fn boolean(field: Presence<'_, bool>) -> Option<bool> {
    match field {
        Presence::Set(value) => Some(*value.value()),
        Presence::Missing => None,
        Presence::Clear { .. } => panic!("unexpected Clear"),
    }
}
fn exact_text(value: &LocatedText, audit: &ScalarAudit<'_>) {
    assert_eq!(value.location().kind(), LocationKind::ScalarExact);
    assert_eq!(audit.text_at(value.span()).unwrap(), value.value());
}
fn text_presence(field: Presence<'_, String>, audit: &ScalarAudit<'_>) {
    match field {
        Presence::Set(value) => exact_text(value, audit),
        Presence::Missing => {}
        Presence::Clear { span } => audit.null_at(span).unwrap(),
    }
}
fn bool_presence(field: Presence<'_, bool>, audit: &ScalarAudit<'_>) {
    match field {
        Presence::Set(value) => {
            assert_eq!(value.location().kind(), LocationKind::ScalarExact);
            assert_eq!(audit.bool_at(value.span()).unwrap(), *value.value());
        }
        Presence::Missing => {}
        Presence::Clear { .. } => panic!("boolean Clear"),
    }
}
fn list_presence(field: Presence<'_, List<String>>, audit: &ScalarAudit<'_>) {
    match field {
        Presence::Set(value) => {
            assert_eq!(value.location().kind(), LocationKind::NodeStart);
            for item in value.value().iter() {
                exact_text(item, audit);
            }
        }
        Presence::Missing => {}
        Presence::Clear { span } => audit.null_at(span).unwrap(),
    }
}
fn map_presence(field: Presence<'_, PatchMap<String>>, audit: &ScalarAudit<'_>) {
    match field {
        Presence::Set(value) => {
            assert_eq!(value.location().kind(), LocationKind::NodeStart);
            for (key, item) in value.value().iter() {
                exact_text(key, audit);
                text_presence(item, audit);
            }
        }
        Presence::Missing => {}
        Presence::Clear { .. } => panic!("map container Clear"),
    }
}
fn check_query(query: &QueryPatch, audit: &ScalarAudit<'_>) {
    for field in [
        query.data_key(),
        query.pattern(),
        query.graph_label(),
        query.name_capture(),
        query.relation_type(),
        query.value_capture(),
        query.value_label(),
        query.name_reject(),
        query.span_capture(),
        query.declaration_capture(),
        query.context_capture(),
        query.parent_capture(),
        query.parent_label(),
        query.qualifier_capture(),
        query.target_fallback(),
    ] {
        text_presence(field, audit);
    }
    if let Presence::Set(value) = query.source() {
        assert_eq!(value.location().kind(), LocationKind::ScalarExact);
        let expected = match value.value() {
            QuerySource::Primary => "primary",
            QuerySource::CobolPreprocessor => "cobol-preprocessor",
        };
        assert_eq!(audit.text_at(value.location().span()).unwrap(), expected);
    }
    list_presence(query.target_labels(), audit);
    bool_presence(query.name_is_data(), audit);
    if let Presence::Set(value) = query.r#type() {
        assert_eq!(value.location().kind(), LocationKind::ScalarExact);
        let expected = match value.value() {
            QueryKind::Entity => "entity",
            QueryKind::Relation => "relation",
        };
        assert_eq!(audit.text_at(value.location().span()).unwrap(), expected);
    }
}
fn check_head(head: &HeadCallsPatch, audit: &ScalarAudit<'_>) {
    text_presence(head.node_type(), audit);
    for field in [head.names(), head.pair_names(), head.subject_pair_names()] {
        list_presence(field, audit);
    }
}
fn check_patch(patch: &ProfilePatch, audit: &ScalarAudit<'_>) {
    assert_eq!(patch.location().kind(), LocationKind::NodeStart);
    for field in [patch.language(), patch.grammar(), patch.start_rule()] {
        text_presence(field, audit);
    }
    for field in [
        patch.extensions(),
        patch.context_requires_entity(),
        patch.self_keywords(),
        patch.anon_func_types(),
        patch.declaration_types(),
        patch.comment_types(),
        patch.embed_labels(),
    ] {
        list_presence(field, audit);
    }
    for field in [patch.merge(), patch.exclusive()] {
        bool_presence(field, audit);
    }
    if let Presence::Set(value) = patch.parser() {
        let expected = match value.value() {
            ProfileParser::TreeSitter => "tree-sitter",
            ProfileParser::Antlr4 => "antlr4",
        };
        assert_eq!(value.location().kind(), LocationKind::ScalarExact);
        assert_eq!(audit.text_at(value.location().span()).unwrap(), expected);
    }
    if let Presence::Set(value) = patch.queries() {
        assert_eq!(value.location().kind(), LocationKind::NodeStart);
        for query in value.value().iter() {
            assert_eq!(query.location().kind(), LocationKind::NodeStart);
            check_query(query.value(), audit);
        }
    }
    match patch.exports() {
        Presence::Set(value) => {
            assert_eq!(value.location().kind(), LocationKind::NodeStart);
            let export = value.value();
            map_presence(export.config(), audit);
            if let Presence::Set(strategy) = export.strategy() {
                let expected = match strategy.value() {
                    ExportStrategy::CapitalizedName => "capitalized_name",
                    ExportStrategy::NoPrefix => "no_prefix",
                    ExportStrategy::Modifier => "modifier",
                    ExportStrategy::ExportStatement => "export_statement",
                    ExportStrategy::NoModifier => "no_modifier",
                    ExportStrategy::NoStatic => "no_static",
                    ExportStrategy::None => "none",
                };
                assert_eq!(audit.text_at(strategy.location().span()).unwrap(), expected);
            }
            if let Presence::Set(list_map) = export.config_list() {
                assert_eq!(list_map.location().kind(), LocationKind::NodeStart);
                for (key, list) in list_map.value().iter() {
                    exact_text(key, audit);
                    list_presence(list, audit);
                }
            }
        }
        Presence::Clear { span } => audit.null_at(span).unwrap(),
        Presence::Missing => {}
    }
    map_presence(patch.context_types(), audit);
    map_presence(patch.context_name_queries(), audit);
    if let Presence::Set(value) = patch.target_rules() {
        for (key, rule) in value.value().iter() {
            exact_text(key, audit);
            match rule {
                Presence::Set(rule) => {
                    assert_eq!(rule.location().kind(), LocationKind::NodeStart);
                    list_presence(rule.value().labels(), audit);
                    text_presence(rule.value().fallback(), audit);
                }
                Presence::Clear { span } => audit.null_at(span).unwrap(),
                Presence::Missing => panic!("missing map entry"),
            }
        }
    }
    if let Presence::Set(value) = patch.embedded() {
        for embedded in value.value().iter() {
            assert_eq!(embedded.location().kind(), LocationKind::NodeStart);
            let embedded = embedded.value();
            for field in [
                embedded.pattern(),
                embedded.text_capture(),
                embedded.lang_capture(),
                embedded.r#default(),
                embedded.wrap_prefix(),
                embedded.wrap_suffix(),
                embedded.normalize(),
            ] {
                text_presence(field, audit);
            }
            map_presence(embedded.languages(), audit);
            list_presence(embedded.host_labels(), audit);
        }
    }
    if let Presence::Set(value) = patch.text_normalizers() {
        for (key, normalizer) in value.value().iter() {
            exact_text(key, audit);
            match normalizer {
                Presence::Set(value) => {
                    map_presence(value.value().replace(), audit);
                    bool_presence(value.value().numeric_char_refs(), audit);
                }
                Presence::Clear { span } => audit.null_at(span).unwrap(),
                Presence::Missing => panic!("missing map entry"),
            }
        }
    }
    match patch.complexity() {
        Presence::Set(value) => {
            let complexity = value.value();
            list_presence(complexity.node_types(), audit);
            list_presence(complexity.operators(), audit);
            match complexity.head_calls() {
                Presence::Set(value) => check_head(value.value(), audit),
                Presence::Clear { span } => audit.null_at(span).unwrap(),
                Presence::Missing => {}
            }
        }
        Presence::Clear { span } => audit.null_at(span).unwrap(),
        Presence::Missing => {}
    }
}

#[test]
fn all_45_profiles_preserve_scalar_values_sites_and_inventory() {
    let decoder = decoder();
    let ctx = context();
    let mut antlr = Vec::new();
    let mut tree_sitter = 0;
    let mut languages = Vec::new();
    let mut documented_noninternal = 0;
    let mut exclusive = Vec::new();
    let audit_policy = policy();
    for (name, source) in CORPUS {
        let corpus_bytes = source.len().max(32768);
        let profile_limits = ProfileLimits::new(ProfileLimitsInput {
            input_bytes: corpus_bytes,
            nodes: source.len().max(2048),
            text_bytes: corpus_bytes,
            ..input_limits()
        })
        .unwrap();
        let scalar_limits = ScalarLimits::new(
            corpus_bytes,
            16,
            8192,
            4096,
            corpus_bytes,
            corpus_bytes,
            256,
        )
        .unwrap();
        let id = label(name);
        let decoded = decoder
            .decode(
                ProfileInput {
                    id: &id,
                    bytes: source,
                },
                &profile_limits,
                &ctx,
            )
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(decoded.id().as_str(), name);
        let audit = audit_policy.audit(source, &scalar_limits).unwrap();
        check_patch(decoded.patch(), &audit);
        let mut string_text_bytes = 0;
        for record in audit.records() {
            match record.kind() {
                ScalarKind::String => string_text_bytes += record.text().len(),
                ScalarKind::Boolean => {}
                other => panic!("{name}: unexpected corpus scalar kind {other:?}"),
            }
        }
        let usage = decoded.usage();
        assert_eq!(usage.text_bytes(), string_text_bytes, "{name}");
        assert_eq!(usage.input_bytes(), source.len());
        let raw: serde_json::Value =
            serde_saphyr::from_str(std::str::from_utf8(source).unwrap()).unwrap();
        let expected: Vec<_> = raw["queries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|query| query["pattern"].as_str().unwrap())
            .collect();
        let actual: Vec<_> = match decoded.patch().queries() {
            Presence::Set(value) => value
                .value()
                .iter()
                .map(|query| text(query.value().pattern()).unwrap())
                .collect(),
            Presence::Missing => Vec::new(),
            Presence::Clear { .. } => panic!("{name}: cleared shipped queries"),
        };
        assert_eq!(actual, expected, "{name}");
        match decoded.patch().parser() {
            Presence::Set(value) if *value.value() == ProfileParser::Antlr4 => antlr.push(name),
            Presence::Missing | Presence::Set(_) => tree_sitter += 1,
            Presence::Clear { .. } => panic!("{name}: cleared corpus parser"),
        }
        if boolean(decoded.patch().exclusive()) == Some(true) {
            exclusive.push(name);
        }
        let language = text(decoded.patch().language()).unwrap();
        // Documented public inventory excludes the sole internal embedding profile.
        // Actual catalog registration and routing are consumer obligations.
        if language != "plpgsql" {
            documented_noninternal += 1;
        }
        assert!(
            !languages
                .iter()
                .any(|old: &String| old.eq_ignore_ascii_case(language))
        );
        languages.push(language.to_owned());
    }
    assert_eq!(antlr, ["cobol85", "db2", "plsql", "postgresql", "tsql"]);
    assert_eq!(tree_sitter, 40);
    assert_eq!(exclusive, ["db2", "plpgsql", "plsql", "postgresql", "tsql"]);
    assert_eq!(CORPUS.len() - exclusive.len(), 40);
    assert_eq!(documented_noninternal, 44);
    assert_eq!(languages.len(), 45);
    assert!(languages.iter().any(|s| s == "plpgsql"));
    assert!(languages.iter().any(|s| s == "typescript"));
    assert!(languages.iter().any(|s| s == "tsx"));
}

#[test]
fn complete_synthetic_schema_and_legitimate_repeated_queries_remain_visible() {
    let ctx = context();
    let id = label("synthetic");
    let decoder = decoder();
    let input = include_bytes!("fixtures/profiles/all-fields.yaml");
    let decoded = decoder
        .decode(
            ProfileInput {
                id: &id,
                bytes: input,
            },
            &limits(),
            &ctx,
        )
        .unwrap();
    assert_eq!(text(decoded.patch().language()), Some("tracked_lang"));
    assert_eq!(text(decoded.patch().grammar()), Some("tree-sitter-custom"));
    assert_eq!(boolean(decoded.patch().merge()), Some(true));
    assert_eq!(boolean(decoded.patch().exclusive()), Some(false));
    check_full_synthetic(decoded.patch());
    let audit = policy()
        .audit(
            input,
            &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 256).unwrap(),
        )
        .unwrap();
    check_patch(decoded.patch(), &audit);
    let input = include_bytes!("fixtures/profiles/schema-cases/query-duplicate-group-order.yaml");
    let decoded = decoder
        .decode(
            ProfileInput {
                id: &id,
                bytes: input,
            },
            &limits(),
            &ctx,
        )
        .unwrap();
    let Presence::Set(queries) = decoded.patch().queries() else {
        panic!("queries");
    };
    assert_eq!(
        queries
            .value()
            .iter()
            .map(|q| text(q.value().data_key()).unwrap())
            .collect::<Vec<_>>(),
        ["a", "a"]
    );
    assert_eq!(
        queries
            .value()
            .iter()
            .map(|q| text(q.value().pattern()).unwrap())
            .collect::<Vec<_>>(),
        ["(one)", "(two)"]
    );
}

#[test]
fn all_prepared_schema_cases_prove_declared_results_and_closed_error_paths() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/profiles/schema-case-expectations.json"
    ))
    .unwrap();
    let ctx = context();
    let decoder = decoder();
    let id = label("schema-cases");
    for row in manifest["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let source = schema_source(name);
        assert_eq!(source.len(), row["bytes"].as_u64().unwrap() as usize);
        let result = decoder.decode(
            ProfileInput {
                id: &id,
                bytes: source,
            },
            &limits(),
            &ctx,
        );
        if row["expected"]["outcome"] == "error" {
            let error = result.err().unwrap_or_else(|| panic!("{name}: accepted"));
            assert_eq!(
                format!("{:?}", error.code()),
                row["expected"]["code"].as_str().unwrap(),
                "{name}"
            );
            if let Some(path) = row["expected"]["schema_path"].as_str() {
                assert_eq!(error.schema_path(), Some(path), "{name}");
            }
            assert_eq!(error.profile_id().unwrap().as_str(), id.as_str());
            assert!(!format!("{error:?}").contains(id.as_str()));
        } else {
            let decoded = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let audit = policy()
                .audit(
                    source,
                    &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 256).unwrap(),
                )
                .unwrap();
            check_patch(decoded.patch(), &audit);
            check_positive_case(name, decoded.patch(), &row["expected"]);
        }
    }
}

fn schema_source(name: &str) -> &'static [u8] {
    match name {
        "exports-null-lower" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-null-lower.yaml")
        }
        "exports-null-title" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-null-title.yaml")
        }
        "exports-null-upper" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-null-upper.yaml")
        }
        "exports-null-tilde" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-null-tilde.yaml")
        }
        "exports-null-implicit" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-null-implicit.yaml")
        }
        "exports-empty-object" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-empty-object.yaml")
        }
        "exports-clear" => include_bytes!("fixtures/profiles/schema-cases/exports-clear.yaml"),
        "complexity-empty-object" => {
            include_bytes!("fixtures/profiles/schema-cases/complexity-empty-object.yaml")
        }
        "complexity-clear" => {
            include_bytes!("fixtures/profiles/schema-cases/complexity-clear.yaml")
        }
        "head-calls-clear" => {
            include_bytes!("fixtures/profiles/schema-cases/head-calls-clear.yaml")
        }
        "map-text-null-clear" => {
            include_bytes!("fixtures/profiles/schema-cases/map-text-null-clear.yaml")
        }
        "map-string-null-set" => {
            include_bytes!("fixtures/profiles/schema-cases/map-string-null-set.yaml")
        }
        "config-list-null-clear" => {
            include_bytes!("fixtures/profiles/schema-cases/config-list-null-clear.yaml")
        }
        "empty-collections-set" => {
            include_bytes!("fixtures/profiles/schema-cases/empty-collections-set.yaml")
        }
        "boolean-false-set" => {
            include_bytes!("fixtures/profiles/schema-cases/boolean-false-set.yaml")
        }
        "start-rule-explicit-empty" => {
            include_bytes!("fixtures/profiles/schema-cases/start-rule-explicit-empty.yaml")
        }
        "relation-without-graph-label" => {
            include_bytes!("fixtures/profiles/schema-cases/relation-without-graph-label.yaml")
        }
        "query-duplicate-group-order" => {
            include_bytes!("fixtures/profiles/schema-cases/query-duplicate-group-order.yaml")
        }
        "opaque-malformed-query" => {
            include_bytes!("fixtures/profiles/schema-cases/opaque-malformed-query.yaml")
        }
        "embedded-language-empty-skip" => {
            include_bytes!("fixtures/profiles/schema-cases/embedded-language-empty-skip.yaml")
        }
        "core-string-controls" => {
            include_bytes!("fixtures/profiles/schema-cases/core-string-controls.yaml")
        }
        "unicode-crlf" => include_bytes!("fixtures/profiles/schema-cases/unicode-crlf.yaml"),
        "bom-origin" => include_bytes!("fixtures/profiles/schema-cases/bom-origin.yaml"),
        "numeric-language-decimal" => {
            include_bytes!("fixtures/profiles/schema-cases/numeric-language-decimal.yaml")
        }
        "numeric-language-float-magnitude" => {
            include_bytes!("fixtures/profiles/schema-cases/numeric-language-float-magnitude.yaml")
        }
        "numeric-language-hex-magnitude" => {
            include_bytes!("fixtures/profiles/schema-cases/numeric-language-hex-magnitude.yaml")
        }
        "numeric-language-octal-magnitude" => {
            include_bytes!("fixtures/profiles/schema-cases/numeric-language-octal-magnitude.yaml")
        }
        "exports-nonnull-scalar-mixed-null" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-nonnull-scalar-mixed-null.yaml")
        }
        "exports-nonnull-scalar-quoted-null" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-nonnull-scalar-quoted-null.yaml")
        }
        "exports-nonnull-scalar-tagged-string-null" => include_bytes!(
            "fixtures/profiles/schema-cases/exports-nonnull-scalar-tagged-string-null.yaml"
        ),
        "exports-nonnull-scalar-boolean" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-nonnull-scalar-boolean.yaml")
        }
        "exports-nonnull-scalar-numeric" => {
            include_bytes!("fixtures/profiles/schema-cases/exports-nonnull-scalar-numeric.yaml")
        }
        "language-empty" => include_bytes!("fixtures/profiles/schema-cases/language-empty.yaml"),
        "language-edge-space" => {
            include_bytes!("fixtures/profiles/schema-cases/language-edge-space.yaml")
        }
        "language-nul" => include_bytes!("fixtures/profiles/schema-cases/language-nul.yaml"),
        "language-missing" => {
            include_bytes!("fixtures/profiles/schema-cases/language-missing.yaml")
        }
        "parser-invalid" => include_bytes!("fixtures/profiles/schema-cases/parser-invalid.yaml"),
        "exclusive-quoted-bool" => {
            include_bytes!("fixtures/profiles/schema-cases/exclusive-quoted-bool.yaml")
        }
        "extensions-null-element" => {
            include_bytes!("fixtures/profiles/schema-cases/extensions-null-element.yaml")
        }
        "config-list-null-element" => {
            include_bytes!("fixtures/profiles/schema-cases/config-list-null-element.yaml")
        }
        "entity-without-label" => {
            include_bytes!("fixtures/profiles/schema-cases/entity-without-label.yaml")
        }
        "relation-without-type" => {
            include_bytes!("fixtures/profiles/schema-cases/relation-without-type.yaml")
        }
        "query-half-value-pair" => {
            include_bytes!("fixtures/profiles/schema-cases/query-half-value-pair.yaml")
        }
        "query-half-parent-pair" => {
            include_bytes!("fixtures/profiles/schema-cases/query-half-parent-pair.yaml")
        }
        "query-missing-pattern" => {
            include_bytes!("fixtures/profiles/schema-cases/query-missing-pattern.yaml")
        }
        "embedded-missing-text-capture" => {
            include_bytes!("fixtures/profiles/schema-cases/embedded-missing-text-capture.yaml")
        }
        "embedded-language-casefold-ambiguity" => include_bytes!(
            "fixtures/profiles/schema-cases/embedded-language-casefold-ambiguity.yaml"
        ),
        "replace-empty-key" => {
            include_bytes!("fixtures/profiles/schema-cases/replace-empty-key.yaml")
        }
        "head-calls-missing-node" => {
            include_bytes!("fixtures/profiles/schema-cases/head-calls-missing-node.yaml")
        }
        "unknown-root" => include_bytes!("fixtures/profiles/schema-cases/unknown-root.yaml"),
        "unknown-nested" => include_bytes!("fixtures/profiles/schema-cases/unknown-nested.yaml"),
        "duplicate-key" => include_bytes!("fixtures/profiles/schema-cases/duplicate-key.yaml"),
        "anchor-alias" => include_bytes!("fixtures/profiles/schema-cases/anchor-alias.yaml"),
        "merge-key" => include_bytes!("fixtures/profiles/schema-cases/merge-key.yaml"),
        "unsupported-tag" => include_bytes!("fixtures/profiles/schema-cases/unsupported-tag.yaml"),
        "multiple-documents" => {
            include_bytes!("fixtures/profiles/schema-cases/multiple-documents.yaml")
        }
        "nullable-compound-sequence" => {
            include_bytes!("fixtures/profiles/schema-cases/nullable-compound-sequence.yaml")
        }
        _ => panic!("unknown prepared fixture"),
    }
}

fn set<T>(field: Presence<'_, T>) -> &T {
    match field {
        Presence::Set(value) => value.value(),
        _ => panic!("expected explicit Set"),
    }
}
fn strings(field: Presence<'_, List<String>>) -> Vec<&str> {
    set(field).iter().map(|v| v.value().as_str()).collect()
}
fn json_strings(value: &serde_json::Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}
fn map_strings(field: Presence<'_, PatchMap<String>>) -> Vec<(&str, &str)> {
    set(field)
        .iter()
        .map(|(key, value)| (key.value().as_str(), text(value).unwrap()))
        .collect()
}
fn check_full_synthetic(patch: &ProfilePatch) {
    assert_eq!(text(patch.language()), Some("tracked_lang"));
    assert_eq!(text(patch.grammar()), Some("tree-sitter-custom"));
    assert_eq!(text(patch.start_rule()), Some("source_file"));
    assert_eq!(*set(patch.parser()), ProfileParser::TreeSitter);
    assert_eq!(boolean(patch.merge()), Some(true));
    assert_eq!(boolean(patch.exclusive()), Some(false));
    for (field, expected) in [
        (patch.extensions(), &[".tracked"][..]),
        (patch.context_requires_entity(), &["class_declaration"][..]),
        (patch.self_keywords(), &["self"][..]),
        (patch.anon_func_types(), &["lambda_expression"][..]),
        (patch.declaration_types(), &["function_declaration"][..]),
        (patch.comment_types(), &["comment"][..]),
        (patch.embed_labels(), &["Function", "Type", "Function"][..]),
    ] {
        assert_eq!(strings(field), expected);
    }
    let queries = set(patch.queries());
    assert_eq!(queries.len(), 1);
    let query = queries.as_slice()[0].value();
    for (field, expected) in [
        (query.data_key(), "declarations"),
        (
            query.pattern(),
            "(function_declaration name: (identifier) @name) @span",
        ),
        (query.graph_label(), "Function"),
        (query.name_capture(), "name"),
        (query.relation_type(), ""),
        (query.value_capture(), "value"),
        (query.value_label(), "Value"),
        (query.name_reject(), "^internal_"),
        (query.span_capture(), "span"),
        (query.declaration_capture(), "span"),
        (query.context_capture(), "span"),
        (query.parent_capture(), "parent"),
        (query.parent_label(), "Type"),
        (query.qualifier_capture(), "qualifier"),
        (query.target_fallback(), "file"),
    ] {
        assert_eq!(text(field), Some(expected));
    }
    assert_eq!(*set(query.r#type()), QueryKind::Entity);
    assert_eq!(boolean(query.name_is_data()), Some(true));
    assert_eq!(strings(query.target_labels()), ["Function", "CustomLabel"]);
    let export = set(patch.exports());
    assert_eq!(*set(export.strategy()), ExportStrategy::Modifier);
    assert_eq!(
        map_strings(export.config()),
        [("node_type", "modifier"), ("name", "public")]
    );
    let map = set(export.config_list());
    assert_eq!(map.len(), 1);
    let (key, field) = map.iter().next().unwrap();
    assert_eq!(key.value(), "names");
    assert_eq!(strings(field), ["public", "exported"]);
    assert_eq!(
        map_strings(patch.context_types()),
        [("class_declaration", "Type")]
    );
    assert_eq!(
        map_strings(patch.context_name_queries()),
        [(
            "class_declaration",
            "(class_declaration name: (_) @name) @context"
        )]
    );
    let map = set(patch.target_rules());
    assert_eq!(map.len(), 1);
    let (key, field) = map.iter().next().unwrap();
    assert_eq!(key.value(), "call");
    let rule = set(field);
    assert_eq!(strings(rule.labels()), ["Function", "CustomLabel"]);
    assert_eq!(text(rule.fallback()), Some("file"));
    let embedded = set(patch.embedded());
    assert_eq!(embedded.len(), 1);
    let embedded = embedded.as_slice()[0].value();
    for (field, expected) in [
        (embedded.pattern(), "(template_string) @text"),
        (embedded.text_capture(), "text"),
        (embedded.lang_capture(), "language"),
        (embedded.r#default(), "sql"),
        (embedded.wrap_prefix(), "select "),
        (embedded.wrap_suffix(), ";"),
        (embedded.normalize(), "html_refs"),
    ] {
        assert_eq!(text(field), Some(expected));
    }
    assert_eq!(
        map_strings(embedded.languages()),
        [("sql", "sql"), ("scss", "")]
    );
    assert_eq!(strings(embedded.host_labels()), ["Function"]);
    let normalizers = set(patch.text_normalizers());
    assert_eq!(normalizers.len(), 1);
    let (key, field) = normalizers.iter().next().unwrap();
    assert_eq!(key.value(), "html_refs");
    let normalizer = set(field);
    assert_eq!(
        map_strings(normalizer.replace()),
        [("&lt;", "<"), (r"\r\n", r"\n")]
    );
    assert_eq!(boolean(normalizer.numeric_char_refs()), Some(true));
    let complexity = set(patch.complexity());
    assert_eq!(strings(complexity.node_types()), ["if_statement"]);
    assert_eq!(strings(complexity.operators()), ["&&", "||"]);
    let head = set(complexity.head_calls());
    assert_eq!(text(head.node_type()), Some("call_expression"));
    assert_eq!(strings(head.names()), ["if"]);
    assert_eq!(strings(head.pair_names()), ["elsif"]);
    assert_eq!(strings(head.subject_pair_names()), ["when"]);
}

fn check_positive_case(name: &str, patch: &ProfilePatch, expected: &serde_json::Value) {
    assert_eq!(expected["outcome"], "decoded_patch");
    assert_eq!(text(patch.language()), Some("x"));
    match name {
        "exports-null-lower"
        | "exports-null-title"
        | "exports-null-upper"
        | "exports-null-tilde"
        | "exports-null-implicit"
        | "exports-clear" => {
            assert_eq!(json_strings(&expected["clear_paths"]), ["exports"]);
            assert!(matches!(patch.exports(), Presence::Clear { .. }));
        }
        "complexity-clear" => {
            assert_eq!(json_strings(&expected["clear_paths"]), ["complexity"]);
            assert!(matches!(patch.complexity(), Presence::Clear { .. }));
        }
        "head-calls-clear" => {
            assert_eq!(
                json_strings(&expected["clear_paths"]),
                ["complexity.head_calls"]
            );
            assert!(matches!(
                set(patch.complexity()).head_calls(),
                Presence::Clear { .. }
            ));
        }
        "exports-empty-object" => {
            assert_eq!(json_strings(&expected["set_paths"]), ["exports"]);
            assert_eq!(expected["preserve_empty_object"], true);
            let value = set(patch.exports());
            assert!(matches!(value.strategy(), Presence::Missing));
            assert!(matches!(value.config(), Presence::Missing));
            assert!(matches!(value.config_list(), Presence::Missing));
        }
        "complexity-empty-object" => {
            assert_eq!(json_strings(&expected["set_paths"]), ["complexity"]);
            assert_eq!(expected["preserve_empty_object"], true);
            let value = set(patch.complexity());
            assert!(matches!(value.node_types(), Presence::Missing));
            assert!(matches!(value.operators(), Presence::Missing));
            assert!(matches!(value.head_calls(), Presence::Missing));
        }
        "map-text-null-clear" | "map-string-null-set" => {
            let map = set(patch.context_types());
            let clears: Vec<_> = map
                .iter()
                .filter_map(|(key, value)| {
                    matches!(value, Presence::Clear { .. }).then_some(key.value().as_str())
                })
                .collect();
            assert_eq!(
                clears,
                json_strings(&expected["clear_map_keys"]["context_types"])
            );
            let values = expected["text_map_values"].as_object().unwrap();
            for (path, expected_value) in values {
                let key = path.strip_prefix("context_types.").unwrap();
                let (_, field) = map.iter().find(|(name, _)| name.value() == key).unwrap();
                assert_eq!(text(field), expected_value.as_str());
            }
            assert_eq!(map.len(), clears.len() + values.len());
        }
        "config-list-null-clear" => {
            let map = set(set(patch.exports()).config_list());
            let clears: Vec<_> = map
                .iter()
                .filter_map(|(key, value)| {
                    matches!(value, Presence::Clear { .. }).then_some(key.value().as_str())
                })
                .collect();
            assert_eq!(
                clears,
                json_strings(&expected["clear_map_keys"]["exports.config_list"])
            );
            let (_, keep) = map.iter().find(|(key, _)| key.value() == "keep").unwrap();
            assert_eq!(
                strings(keep),
                json_strings(&expected["ordered_lists"]["exports.config_list.keep"])
            );
            assert_eq!(map.len(), 2);
        }
        "empty-collections-set" => {
            assert_eq!(
                json_strings(&expected["set_empty"]),
                ["queries", "context_types", "embedded", "embed_labels"]
            );
            assert!(set(patch.queries()).is_empty());
            assert!(set(patch.context_types()).is_empty());
            assert!(set(patch.embedded()).is_empty());
            assert!(set(patch.embed_labels()).is_empty());
        }
        "boolean-false-set" => {
            assert_eq!(
                boolean(patch.merge()),
                expected["boolean_values"]["merge"].as_bool()
            );
            assert_eq!(
                boolean(patch.exclusive()),
                expected["boolean_values"]["exclusive"].as_bool()
            );
        }
        "start-rule-explicit-empty" => assert_eq!(
            text(patch.start_rule()),
            expected["text_values"]["start_rule"].as_str()
        ),
        "relation-without-graph-label" => {
            let queries = set(patch.queries());
            assert_eq!(queries.len(), 1);
            let query = queries.as_slice()[0].value();
            assert_eq!(json_strings(&expected["query_types"]), ["relation"]);
            assert_eq!(*set(query.r#type()), QueryKind::Relation);
            assert!(matches!(query.graph_label(), Presence::Missing));
            assert_eq!(text(query.relation_type()), Some("CALLS"));
            assert_eq!(text(query.pattern()), expected["opaque_pattern"].as_str());
        }
        "query-duplicate-group-order" | "opaque-malformed-query" => {
            let queries = set(patch.queries());
            assert_eq!(
                queries
                    .iter()
                    .map(|q| text(q.value().pattern()).unwrap())
                    .collect::<Vec<_>>(),
                json_strings(&expected["ordered_query_patterns"])
            );
            if name == "query-duplicate-group-order" {
                assert_eq!(
                    queries
                        .iter()
                        .map(|q| text(q.value().data_key()).unwrap())
                        .collect::<Vec<_>>(),
                    json_strings(&expected["ordered_query_data_keys"])
                );
            } else {
                assert_eq!(expected["native_compile_executed"], false);
            }
        }
        "embedded-language-empty-skip" => {
            let embedded = set(patch.embedded());
            assert_eq!(embedded.len(), 1);
            let map = set(embedded.as_slice()[0].value().languages());
            assert_eq!(map.len(), 2);
            for (path, expected_value) in expected["text_map_values"].as_object().unwrap() {
                let key = path.strip_prefix("embedded.languages.").unwrap();
                let (_, field) = map.iter().find(|(name, _)| name.value() == key).unwrap();
                assert_eq!(text(field), expected_value.as_str());
            }
        }
        "core-string-controls" => assert_eq!(
            strings(patch.extensions()),
            json_strings(&expected["ordered_lists"]["extensions"])
        ),
        "unicode-crlf" => {
            assert_eq!(
                strings(patch.extensions()),
                [expected["text_values"]["extensions[0]"].as_str().unwrap()]
            );
            assert_eq!(expected["source_bytes_preserved"], true);
        }
        "bom-origin" => {
            assert_eq!(
                text(patch.language()),
                expected["text_values"]["language"].as_str()
            );
            assert_eq!(expected["producer_original_origin"], true);
        }
        _ => panic!("uncovered positive fixture: {name}"),
    }
}

fn decode_with<'a>(
    decoder: &ProfileDecoder,
    id: &'a ProfileId,
    source: &'a [u8],
    limits: &ProfileLimits,
    ctx: &SnapshotContext,
) -> Result<DecodedProfile, ProfileError<'a>> {
    decoder.decode(ProfileInput { id, bytes: source }, limits, ctx)
}

#[test]
fn minimal_exact_semantic_caps_and_each_excess_are_validated() {
    let decoder = decoder();
    let ctx = context();
    let id = label("caps");
    let source = include_bytes!("fixtures/profiles/minimal.yaml");
    let exact = || ProfileLimitsInput {
        input_bytes: 12,
        nodes: 2,
        depth: 2,
        text_bytes: 9,
        scalar_bytes: 8,
        collection_entries: 1,
    };
    let decoded = decode_with(
        &decoder,
        &id,
        source,
        &ProfileLimits::new(exact()).unwrap(),
        &ctx,
    )
    .unwrap();
    let usage = decoded.usage();
    assert_eq!(
        (
            usage.input_bytes(),
            usage.nodes(),
            usage.depth(),
            usage.text_bytes(),
            usage.max_scalar_bytes(),
            usage.max_collection_entries()
        ),
        (12, 2, 2, 9, 8, 1)
    );
    for index in 0..5 {
        let mut value = exact();
        match index {
            0 => value.input_bytes -= 1,
            1 => value.nodes -= 1,
            2 => value.depth -= 1,
            3 => value.text_bytes -= 1,
            4 => value.scalar_bytes -= 1,
            _ => unreachable!(),
        }
        let error = decode_with(
            &decoder,
            &id,
            source,
            &ProfileLimits::new(value).unwrap(),
            &ctx,
        )
        .err()
        .unwrap();
        assert_eq!(
            error.code(),
            if index == 0 {
                ProfileErrorCode::InputTooLarge
            } else {
                ProfileErrorCode::QuotaExceeded
            }
        );
    }
    // C=1 is exact for the minimal map; a second field exceeds that same positive cap.
    let mut value = input_limits();
    value.collection_entries = 1;
    let error = decode_with(
        &decoder,
        &id,
        b"language: x\nmerge: false\n",
        &ProfileLimits::new(value).unwrap(),
        &ctx,
    )
    .err()
    .unwrap();
    assert_eq!(error.code(), ProfileErrorCode::QuotaExceeded);
}

#[test]
fn constructors_reject_zero_fields_and_native_counter_overflow() {
    for index in 0..6 {
        let mut value = input_limits();
        match index {
            0 => value.input_bytes = 0,
            1 => value.nodes = 0,
            2 => value.depth = 0,
            3 => value.text_bytes = 0,
            4 => value.scalar_bytes = 0,
            5 => value.collection_entries = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            ProfileLimits::new(value).err(),
            Some(ProfileLimitError::InvalidLimits)
        );
    }
    let mut value = input_limits();
    value.nodes = usize::MAX;
    assert_eq!(
        ProfileLimits::new(value).err(),
        Some(ProfileLimitError::ArithmeticOverflow)
    );
    assert_eq!(ProfileId::new("", 1).err(), Some(ProfileIdError::InvalidId));
    assert_eq!(
        ProfileId::new("a\0b", 3).err(),
        Some(ProfileIdError::InvalidId)
    );
    assert_eq!(
        ProfileId::new("x", 0).err(),
        Some(ProfileIdError::InvalidLimits)
    );
    assert_eq!(
        ProfileId::new("xy", 1).err(),
        Some(ProfileIdError::InputTooLarge)
    );
    assert_eq!(ProfileId::new("xy", 2).unwrap().as_str(), "xy");
    if usize::BITS > 32 {
        let too_large = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
        let mut value = input_limits();
        value.input_bytes = too_large;
        assert_eq!(
            ProfileLimits::new(value).err(),
            Some(ProfileLimitError::InvalidLimits)
        );
        let mut value = input_limits();
        value.collection_entries = too_large;
        assert_eq!(
            ProfileLimits::new(value).err(),
            Some(ProfileLimitError::InvalidLimits)
        );
    }
}

#[test]
fn original_context_cancel_and_deadline_precede_source_validation() {
    let decoder = decoder();
    let id = label("contexts");
    let source = b"language: x\n";
    let signal = Cancellation::default();
    let original = SnapshotContext::new(Instant::now() + Duration::from_secs(120), signal.clone());
    decode_with(&decoder, &id, source, &limits(), &original).unwrap();
    signal.cancel();
    assert_eq!(
        decode_with(&decoder, &id, source, &limits(), &original)
            .err()
            .unwrap()
            .code(),
        ProfileErrorCode::Cancelled
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        decode_with(&decoder, &id, source, &limits(), &expired)
            .err()
            .unwrap()
            .code(),
        ProfileErrorCode::DeadlineExceeded
    );
    let signal = Cancellation::default();
    signal.cancel();
    let both = SnapshotContext::new(Instant::now() - Duration::from_secs(1), signal);
    assert_eq!(
        decode_with(&decoder, &id, source, &limits(), &both)
            .err()
            .unwrap()
            .code(),
        ProfileErrorCode::Cancelled
    );
}

#[test]
fn missing_clear_empty_and_map_removal_are_distinct_readonly_outputs() {
    let decoder = decoder();
    let ctx = context();
    let id = label("presence");
    let minimal = decode_with(&decoder, &id, b"language: x\n", &limits(), &ctx).unwrap();
    assert!(matches!(minimal.patch().exports(), Presence::Missing));
    assert!(matches!(minimal.patch().start_rule(), Presence::Missing));
    let empty = decode_with(
        &decoder,
        &id,
        schema_source("empty-collections-set"),
        &limits(),
        &ctx,
    )
    .unwrap();
    for field in [
        empty.patch().queries().map_len(),
        empty.patch().embedded().map_len(),
        empty.patch().embed_labels().map_len(),
    ] {
        assert_eq!(field, Some(0));
    }
    let Presence::Set(map) = empty.patch().context_types() else {
        panic!("empty map is Set");
    };
    assert!(map.value().is_empty());
    let clear = decode_with(
        &decoder,
        &id,
        schema_source("exports-clear"),
        &limits(),
        &ctx,
    )
    .unwrap();
    assert!(matches!(clear.patch().exports(), Presence::Clear { .. }));
    let object = decode_with(
        &decoder,
        &id,
        schema_source("exports-empty-object"),
        &limits(),
        &ctx,
    )
    .unwrap();
    let Presence::Set(object) = object.patch().exports() else {
        panic!("empty object is Set");
    };
    assert!(matches!(object.value().strategy(), Presence::Missing));
    let decoded = decode_with(
        &decoder,
        &id,
        schema_source("map-text-null-clear"),
        &limits(),
        &ctx,
    )
    .unwrap();
    let Presence::Set(map) = decoded.patch().context_types() else {
        panic!("map");
    };
    let mut entries = map.value().iter();
    let (key, field) = entries.next().unwrap();
    assert_eq!(key.value(), "gone");
    assert!(matches!(field, Presence::Clear { .. }));
    let (key, field) = entries.next().unwrap();
    assert_eq!((key.value().as_str(), text(field)), ("keep", Some("value")));
    assert!(entries.next().is_none());
    let decoded = decode_with(
        &decoder,
        &id,
        schema_source("map-string-null-set"),
        &limits(),
        &ctx,
    )
    .unwrap();
    let Presence::Set(map) = decoded.patch().context_types() else {
        panic!("map");
    };
    assert_eq!(
        map.value()
            .iter()
            .map(|(key, field)| (key.value().as_str(), text(field).unwrap()))
            .collect::<Vec<_>>(),
        [("quoted", "null"), ("tagged", "null"), ("mixed", "NuLl")]
    );
    let decoded = decode_with(
        &decoder,
        &id,
        schema_source("config-list-null-clear"),
        &limits(),
        &ctx,
    )
    .unwrap();
    let Presence::Set(exports) = decoded.patch().exports() else {
        panic!("exports");
    };
    let Presence::Set(map) = exports.value().config_list() else {
        panic!("config_list");
    };
    let mut entries = map.value().iter();
    assert!(matches!(entries.next().unwrap().1, Presence::Clear { .. }));
    let (_, Presence::Set(list)) = entries.next().unwrap() else {
        panic!("retained list");
    };
    assert_eq!(
        list.value()
            .iter()
            .map(|v| v.value().as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(entries.next().is_none());
    let decoded = decode_with(
        &decoder,
        &id,
        schema_source("start-rule-explicit-empty"),
        &limits(),
        &ctx,
    )
    .unwrap();
    assert_eq!(text(decoded.patch().start_rule()), Some(""));
    let decoded = decode_with(
        &decoder,
        &id,
        schema_source("boolean-false-set"),
        &limits(),
        &ctx,
    )
    .unwrap();
    assert_eq!(boolean(decoded.patch().exclusive()), Some(false));
    let decoded = decode_with(
        &decoder,
        &id,
        schema_source("opaque-malformed-query"),
        &limits(),
        &ctx,
    )
    .unwrap();
    let Presence::Set(queries) = decoded.patch().queries() else {
        panic!("queries");
    };
    assert_eq!(
        text(queries.value().as_slice()[0].value().pattern()),
        Some("((((")
    );
}

trait ListPresenceLength {
    fn map_len(self) -> Option<usize>;
}
impl<T> ListPresenceLength for Presence<'_, List<T>> {
    fn map_len(self) -> Option<usize> {
        match self {
            Presence::Set(v) => Some(v.value().len()),
            _ => None,
        }
    }
}

#[test]
fn sdk_scalar_transport_preserves_tagged_boolean_and_visible_native_rejections() {
    let decoder = decoder();
    let ctx = context();
    let id = label("sdk-boundaries");
    let source = include_bytes!("fixtures/profiles/sdk-boundaries/tagged-bool.yaml");
    let decoded = decode_with(&decoder, &id, source, &limits(), &ctx).unwrap();
    assert_eq!(boolean(decoded.patch().merge()), Some(true));
    assert_eq!(boolean(decoded.patch().exclusive()), Some(false));
    let audit = policy()
        .audit(
            source,
            &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 256).unwrap(),
        )
        .unwrap();
    check_patch(decoded.patch(), &audit);
    for source in [
        include_bytes!("fixtures/profiles/sdk-boundaries/nullable-nonfinite.yaml").as_slice(),
        include_bytes!("fixtures/profiles/sdk-boundaries/nullable-tagged-int-overflow.yaml")
            .as_slice(),
        include_bytes!("fixtures/profiles/sdk-boundaries/text-tagged-bool.yaml").as_slice(),
    ] {
        let error = decode_with(&decoder, &id, source, &limits(), &ctx)
            .err()
            .unwrap();
        assert_eq!(error.code(), ProfileErrorCode::Decode);

        assert_eq!(
            error.schema_path(),
            None,
            "native visitor error has no invented semantic path"
        );
        if let Some(span) = error.span() {
            assert!(span.end() <= source.len());
        }
    }
    for source in [
        b"language: x\ncontext_types: {a: !!bool false}\n".as_slice(),
        b"language: x\ncontext_types: {a: !!int 123}\n".as_slice(),
    ] {
        let error = decode_with(&decoder, &id, source, &limits(), &ctx)
            .err()
            .unwrap();
        assert_eq!(error.code(), ProfileErrorCode::InvalidField);
        assert_eq!(error.schema_path(), Some("context_types"));
        assert_eq!(error.indices(), [Some(0), None, None, None]);
    }
}

#[test]
fn original_scalar_spans_and_native_node_starts_are_not_container_ranges() {
    let decoder = decoder();
    let ctx = context();
    let id = label("sites");
    let source = include_bytes!("fixtures/profiles/implicit-null.yaml");
    let decoded = decode_with(&decoder, &id, source, &limits(), &ctx).unwrap();
    let Presence::Clear { span } = decoded.patch().exports() else {
        panic!("implicit Clear");
    };
    assert_eq!((span.start(), span.end()), (19, 19));
    assert_ne!(span.end(), source.len());
    // Independently freeze SDK/audit colon-empty sites; neither fragment is a full profile.
    for (source, offset) in [(b"x:\n".as_slice(), 1), (b"exports:\n".as_slice(), 7)] {
        let native: std::collections::BTreeMap<String, serde_saphyr::Spanned<Option<String>>> =
            serde_saphyr::from_str(std::str::from_utf8(source).unwrap()).unwrap();
        let audit = policy()
            .audit(
                source,
                &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 256).unwrap(),
            )
            .unwrap();
        let value = native.values().next().unwrap();
        assert!(value.value.is_none());
        let site = value.defined.span();
        let span = audit
            .original_span(
                graphit_ast::scalar_policy::CodecSpan::from_parts(
                    site.byte_offset(),
                    site.byte_len(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!((span.start(), span.end()), (offset, offset));
        audit.null_at(span).unwrap();
        let eof = audit
            .original_span(graphit_ast::scalar_policy::CodecSpan::new(
                source.len() as u64,
                0,
            ))
            .unwrap();
        assert_ne!(span, eof);
    }
    for source in [schema_source("unicode-crlf"), schema_source("bom-origin")] {
        let decoded = decode_with(&decoder, &id, source, &limits(), &ctx).unwrap();
        let audit = policy()
            .audit(
                source,
                &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 256).unwrap(),
            )
            .unwrap();
        check_patch(decoded.patch(), &audit);
        let Presence::Set(language) = decoded.patch().language() else {
            panic!("language");
        };
        assert_eq!(
            &source[language.span().start()..language.span().end()],
            b"x"
        );
        if let Presence::Set(extensions) = decoded.patch().extensions() {
            assert_eq!(extensions.value().as_slice()[0].value(), "α😀");
        }
    }
    #[derive(serde::Deserialize)]
    struct NativeSites {
        exports: serde_saphyr::Spanned<NativeExport>,
    }
    #[derive(serde::Deserialize)]
    struct NativeExport {
        config: serde_saphyr::Spanned<std::collections::BTreeMap<String, String>>,
    }
    let source = b"language: x\nexports: {config: {a: b}}\n";
    let decoded = decode_with(&decoder, &id, source, &limits(), &ctx).unwrap();
    let native: serde_saphyr::Spanned<NativeSites> =
        serde_saphyr::from_str(std::str::from_utf8(source).unwrap()).unwrap();
    let audit = policy()
        .audit(
            source,
            &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 256).unwrap(),
        )
        .unwrap();
    let Presence::Set(exports) = decoded.patch().exports() else {
        panic!("exports");
    };
    let Presence::Set(config) = exports.value().config() else {
        panic!("config");
    };
    for (actual, expected) in [
        (decoded.patch().location(), native.defined),
        (exports.location(), native.value.exports.defined),
        (config.location(), native.value.exports.value.config.defined),
    ] {
        let span = expected.span();
        let expected = audit
            .original_span(
                graphit_ast::scalar_policy::CodecSpan::from_parts(
                    span.byte_offset(),
                    span.byte_len(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(actual.kind(), LocationKind::NodeStart);
        assert_eq!(actual.span(), expected);
    }
    assert_eq!(
        native
            .value
            .exports
            .value
            .config
            .value
            .get("a")
            .map(String::as_str),
        Some("b")
    );
    assert!(config.location().span().end() < source.len() - 1);
}

#[test]
fn closed_semantic_error_sites_keep_ordered_indices_and_hide_source() {
    let decoder = decoder();
    let ctx = context();
    let id = label("logical-label-secret");
    for (source, path, indices) in [
        (
            b"language: x\nextensions: [ok, null]\n".as_slice(),
            "extensions",
            [Some(1), None, None, None],
        ),
        (
            b"language: x\nexports: {config_list: {keep: [a, null]}}\n".as_slice(),
            "exports.config_list",
            [Some(0), Some(1), None, None],
        ),
        (
            schema_source("query-half-value-pair"),
            "queries.value_capture",
            [Some(0), None, None, None],
        ),
    ] {
        let error = decode_with(&decoder, &id, source, &limits(), &ctx)
            .err()
            .unwrap();
        assert_eq!(error.code(), ProfileErrorCode::InvalidField);
        assert_eq!(error.schema_path(), Some(path));
        assert_eq!(error.indices(), indices);
        let span = error.span().unwrap();
        assert!(span.end() <= source.len());
        assert!(!format!("{error:?}").contains(id.as_str()));
        assert!(!format!("{error}").contains("language:"));
    }
    assert_eq!(
        decode_with(&decoder, &id, b"\xff", &limits(), &ctx)
            .err()
            .unwrap()
            .code(),
        ProfileErrorCode::InvalidUtf8
    );
}

#[test]
fn declaration_capture_preserves_missing_empty_and_literal_null_text() {
    let decoder = decoder();
    let ctx = context();
    let id = label("declaration-presence");
    let source = b"language: x\nqueries:\n  - data_key: omitted\n    graph_label: Function\n    pattern: p\n  - data_key: empty\n    graph_label: Function\n    pattern: p\n    declaration_capture: \"\"\n  - data_key: text\n    graph_label: Function\n    pattern: p\n    declaration_capture: \"null\"\n";
    let decoded = decode_with(&decoder, &id, source, &limits(), &ctx).unwrap();
    let queries = set(decoded.patch().queries());
    assert!(matches!(
        queries.as_slice()[0].value().declaration_capture(),
        Presence::Missing
    ));
    assert_eq!(
        text(queries.as_slice()[1].value().declaration_capture()),
        Some("")
    );
    assert_eq!(
        text(queries.as_slice()[2].value().declaration_capture()),
        Some("null")
    );
}

#[test]
fn declaration_capture_null_is_a_closed_typed_error_at_its_original_site() {
    let decoder = decoder();
    let ctx = context();
    let id = label("declaration-null");
    let source = b"language: x\nqueries:\n  - data_key: declarations\n    pattern: p\n    declaration_capture: null\n";
    let error = decode_with(&decoder, &id, source, &limits(), &ctx)
        .err()
        .unwrap();
    assert_eq!(error.code(), ProfileErrorCode::InvalidField);
    assert_eq!(error.schema_path(), Some("queries.declaration_capture"));
    assert_eq!(error.indices(), [Some(0), None, None, None]);
    let span = error.span().unwrap();
    assert_eq!(&source[span.start()..span.end()], b"null");
    assert!(!format!("{error:?}").contains(id.as_str()));
}

#[test]
fn context_capture_preserves_missing_empty_and_literal_null_text() {
    let decoder = decoder();
    let ctx = context();
    let id = label("context-presence");
    let source = b"language: x\nqueries:\n  - data_key: omitted\n    graph_label: Function\n    pattern: p\n  - data_key: empty\n    graph_label: Function\n    pattern: p\n    context_capture: \"\"\n  - data_key: text\n    graph_label: Function\n    pattern: p\n    context_capture: \"null\"\n";
    let decoded = decode_with(&decoder, &id, source, &limits(), &ctx).unwrap();
    let queries = set(decoded.patch().queries());
    assert!(matches!(
        queries.as_slice()[0].value().context_capture(),
        Presence::Missing
    ));
    assert_eq!(
        text(queries.as_slice()[1].value().context_capture()),
        Some("")
    );
    assert_eq!(
        text(queries.as_slice()[2].value().context_capture()),
        Some("null")
    );
}

#[test]
fn context_capture_null_is_a_closed_typed_error_at_its_original_site() {
    let decoder = decoder();
    let ctx = context();
    let id = label("context-null");
    let source = b"language: x\nqueries:\n  - data_key: declarations\n    pattern: p\n    context_capture: null\n";
    let error = decode_with(&decoder, &id, source, &limits(), &ctx)
        .err()
        .unwrap();
    assert_eq!(error.code(), ProfileErrorCode::InvalidField);
    assert_eq!(error.schema_path(), Some("queries.context_capture"));
    assert_eq!(error.indices(), [Some(0), None, None, None]);
    let span = error.span().unwrap();
    assert_eq!(&source[span.start()..span.end()], b"null");
    assert!(!format!("{error:?}").contains(id.as_str()));
}

#[test]
fn native_antlr_capture_operations_preserve_slots_and_reject_ambiguous_or_unknown_input() {
    let decoder = decoder();
    let ctx = context();
    let id = label("native-captures");
    let valid = b"language: custom\nparser: antlr4\ngrammar: antlr-plsql\nantlr_contexts: [{pattern: '//create_procedure_body', label: Procedure, name: [{xpath: '/procedure_name'}]}]\nqueries: [{data_key: names, graph_label: Function, pattern: '//general_element_part', child_rule: function_argument, captures: {name: [{xpath: '/id_expression'}], qualifier: [{nearest_ancestor: update_statement}, {xpath: '/general_table_ref'}]}}]\n";
    let decoded = decode_with(&decoder, &id, valid, &limits(), &ctx).unwrap();
    let contexts = set(decoded.patch().antlr_contexts());
    assert_eq!(contexts.len(), 1);
    assert_eq!(
        text(contexts.as_slice()[0].value().label()),
        Some("Procedure")
    );
    let queries = set(decoded.patch().queries());
    assert_eq!(set(queries.as_slice()[0].value().captures()).len(), 2);
    for bad in [
        "captures: {name: [{}]}",
        "captures: {name: [{xpath: '/identifier', nearest_descendant: identifier}]}",
        "captures: {unknown: []}",
        "captures: {value: [{xpath: '/expression'}]}",
        "captures: {parent: []}",
    ] {
        let source = format!(
            "language: x\nqueries: [{{data_key: x, graph_label: Function, pattern: '//x', {bad}}}]\n"
        );
        assert!(
            decode_with(&decoder, &id, source.as_bytes(), &limits(), &ctx).is_err(),
            "{bad}"
        );
    }
    let old = b"language: json\ncontext_name_paths: {pair: key}\n";
    assert!(decode_with(&decoder, &id, old, &limits(), &ctx).is_err());
}
