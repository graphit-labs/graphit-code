use graphit_ast::catalog::*;
use graphit_ast::profiles::*;
use graphit_ast::scalar_policy::{RegexLimits, ScalarLimits, ScalarPolicy};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::time::{Duration, Instant};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("fixtures/catalog/", $name, ".yaml")).as_slice()
    };
}
macro_rules! corpus { ($($name:literal),* $(,)?) => { [$(($name,
    include_bytes!(concat!("fixtures/catalog/shipped/", $name, ".yaml")).as_slice()),)*] }; }

// Test-only projection through every public getter, checked against independent fixtures.
fn observe<T>(
    value: &Effective<T>,
    audit: Option<&graphit_ast::scalar_policy::ScalarAudit<'_>>,
    kind: Option<LocationKind>,
) {
    if let Some(audit) = audit {
        let site = match value.provenance() {
            ProvenanceView::Declared(site) => site,
            other => panic!("expected declared source, got {other:?}"),
        };
        assert_eq!(site.source_index().as_u32(), 0);
        if let Some(kind) = kind {
            assert_eq!(site.location().kind(), kind);
        }
        assert!(site.span().end() <= audit.input().len());
    }
}
fn shape_text(
    value: &EffectiveText,
    audit: Option<&graphit_ast::scalar_policy::ScalarAudit<'_>>,
) -> serde_json::Value {
    observe(value, audit, Some(LocationKind::ScalarExact));
    if let Some(audit) = audit {
        let ProvenanceView::Declared(site) = value.provenance() else {
            unreachable!()
        };
        assert_eq!(audit.text_at(site.span()).unwrap(), value.value());
    }
    serde_json::json!(value.value())
}
fn shape_texts(
    value: &Effective<EffectiveList<EffectiveText>>,
    audit: Option<&graphit_ast::scalar_policy::ScalarAudit<'_>>,
) -> serde_json::Value {
    observe(value, audit, Some(LocationKind::NodeStart));
    serde_json::Value::Array(value.value().iter().map(|v| shape_text(v, audit)).collect())
}
fn shape_map<T>(
    value: &Effective<EffectiveMap<T>>,
    audit: Option<&graphit_ast::scalar_policy::ScalarAudit<'_>>,
    mut convert: impl FnMut(&Effective<T>) -> serde_json::Value,
) -> serde_json::Value {
    observe(value, audit, Some(LocationKind::NodeStart));
    let mut object = serde_json::Map::new();
    for (key, item) in value.value().iter() {
        shape_text(key, audit);
        assert!(object.insert(key.value().clone(), convert(item)).is_none());
    }
    serde_json::Value::Object(object)
}
fn text_pairs(value: &Effective<EffectiveMap<String>>) -> Vec<(&str, &str)> {
    value
        .value()
        .iter()
        .map(|(k, v)| (k.value().as_str(), v.value().as_str()))
        .collect()
}
fn profile_shape(
    p: &EffectiveProfile,
    audit: Option<&graphit_ast::scalar_policy::ScalarAudit<'_>>,
) -> serde_json::Value {
    use serde_json::json;
    observe(p.parser(), audit, None);
    observe(p.start_rule(), audit, Some(LocationKind::ScalarExact));
    if let (Some(audit), Some(text)) = (audit, p.start_rule().value()) {
        let ProvenanceView::Declared(site) = p.start_rule().provenance() else {
            unreachable!()
        };
        assert_eq!(audit.text_at(site.span()).unwrap(), text);
    }
    observe(p.exclusive(), audit, Some(LocationKind::ScalarExact));
    if let Some(audit) = audit {
        let ProvenanceView::Declared(site) = p.exclusive().provenance() else {
            unreachable!()
        };
        assert_eq!(audit.bool_at(site.span()).unwrap(), *p.exclusive().value());
    }
    observe(p.queries(), audit, Some(LocationKind::NodeStart));
    let queries: Vec<_> = p.queries().value().iter().map(|entry| {
        observe(entry, audit, Some(LocationKind::NodeStart));
        let q = entry.value();
        observe(q.source(), audit, None);
        observe(q.r#type(), audit, None);
        observe(q.name_is_data(), audit, Some(LocationKind::ScalarExact));
        if let Some(audit) = audit {
            let ProvenanceView::Declared(site) = q.name_is_data().provenance() else { unreachable!() };
            assert_eq!(audit.bool_at(site.span()).unwrap(), *q.name_is_data().value());
        }
        json!({"data_key":shape_text(q.data_key(),audit),"pattern":shape_text(q.pattern(),audit),
            "graph_label":shape_text(q.graph_label(),audit),"name_capture":shape_text(q.name_capture(),audit),
            "relation_type":shape_text(q.relation_type(),audit),"value_capture":shape_text(q.value_capture(),audit),
            "value_label":shape_text(q.value_label(),audit),"name_reject":shape_text(q.name_reject(),audit),
            "span_capture":shape_text(q.span_capture(),audit),"declaration_capture":shape_text(q.declaration_capture(),audit),
            "context_capture":shape_text(q.context_capture(),audit),
            "parent_capture":shape_text(q.parent_capture(),audit),
            "parent_label":shape_text(q.parent_label(),audit),"qualifier_capture":shape_text(q.qualifier_capture(),audit),
            "target_fallback":shape_text(q.target_fallback(),audit),
            "type":match q.r#type().value() {QueryKind::Entity=>"entity",QueryKind::Relation=>"relation"},
            "name_is_data":q.name_is_data().value(),"target_labels":shape_texts(q.target_labels(),audit)})
    }).collect();
    observe(p.exports(), audit, Some(LocationKind::NodeStart));
    let e = p.exports().value();
    observe(e.strategy(), audit, None);
    let strategy = match e.strategy().value() {
        ExportStrategy::None => "none",
        ExportStrategy::CapitalizedName => "capitalized_name",
        ExportStrategy::NoPrefix => "no_prefix",
        ExportStrategy::Modifier => "modifier",
        ExportStrategy::ExportStatement => "export_statement",
        ExportStrategy::NoModifier => "no_modifier",
        ExportStrategy::NoStatic => "no_static",
    };
    let exports = json!({"strategy":strategy,"config":shape_map(e.config(),audit,|v|shape_text(v,audit)),
        "config_list":shape_map(e.config_list(),audit,|v|shape_texts(v,audit))});
    let targets = shape_map(p.target_rules(), audit, |entry| {
        observe(entry, audit, Some(LocationKind::NodeStart));
        let t = entry.value();
        json!({"labels":shape_texts(t.labels(),audit),"fallback":shape_text(t.fallback(),audit)})
    });
    observe(p.embedded(), audit, Some(LocationKind::NodeStart));
    let embedded:Vec<_>=p.embedded().value().iter().map(|entry| {
        observe(entry,audit,Some(LocationKind::NodeStart));let e=entry.value();
        json!({"pattern":shape_text(e.pattern(),audit),"text_capture":shape_text(e.text_capture(),audit),
            "lang_capture":shape_text(e.lang_capture(),audit),"default":shape_text(e.r#default(),audit),
            "wrap_prefix":shape_text(e.wrap_prefix(),audit),"wrap_suffix":shape_text(e.wrap_suffix(),audit),
            "normalize":shape_text(e.normalize(),audit),"languages":shape_map(e.languages(),audit,|v|shape_text(v,audit)),
            "host_labels":shape_texts(e.host_labels(),audit)})
    }).collect();
    let normalizers = shape_map(p.text_normalizers(), audit, |entry| {
        observe(entry, audit, Some(LocationKind::NodeStart));
        let n = entry.value();
        observe(
            n.numeric_char_refs(),
            audit,
            Some(LocationKind::ScalarExact),
        );
        if let Some(audit) = audit {
            let ProvenanceView::Declared(site) = n.numeric_char_refs().provenance() else {
                unreachable!()
            };
            assert_eq!(
                audit.bool_at(site.span()).unwrap(),
                *n.numeric_char_refs().value()
            );
        }
        json!({"replace":shape_map(n.replace(),audit,|v|shape_text(v,audit)),"numeric_char_refs":n.numeric_char_refs().value()})
    });
    observe(p.complexity(), audit, Some(LocationKind::NodeStart));
    let c = p.complexity().value();
    observe(c.head_calls(), audit, Some(LocationKind::NodeStart));
    let head = c.head_calls().value().as_ref().map(|h| {
        json!({"node_type":shape_text(h.node_type(),audit),
        "names":shape_texts(h.names(),audit),"pair_names":shape_texts(h.pair_names(),audit),
        "subject_pair_names":shape_texts(h.subject_pair_names(),audit)})
    });
    json!({"language":shape_text(p.language(),audit),"extensions":shape_texts(p.extensions(),audit),
        "parser":match p.parser().value(){ProfileParser::TreeSitter=>"tree-sitter",ProfileParser::Antlr4=>"antlr4"},
        "grammar":shape_text(p.grammar(),audit),"start_rule":p.start_rule().value(),"exclusive":p.exclusive().value(),
        "queries":queries,"exports":exports,"context_types":shape_map(p.context_types(),audit,|v|shape_text(v,audit)),
        "context_requires_entity":shape_texts(p.context_requires_entity(),audit),
        "context_name_queries":shape_map(p.context_name_queries(),audit,|v|shape_text(v,audit)),"target_rules":targets,
        "embedded":embedded,"text_normalizers":normalizers,"self_keywords":shape_texts(p.self_keywords(),audit),
        "anon_func_types":shape_texts(p.anon_func_types(),audit),"declaration_types":shape_texts(p.declaration_types(),audit),
        "comment_types":shape_texts(p.comment_types(),audit),"embed_labels":shape_texts(p.embed_labels(),audit),
        "complexity":{"node_types":shape_texts(c.node_types(),audit),"operators":shape_texts(c.operators(),audit),"head_calls":head}})
}

#[test]
fn joint_real_decoder_rejections_preserve_old_catalog() {
    macro_rules! joint {
        ($name:literal) => {
            include_bytes!(concat!("fixtures/catalog/joint/", $name, ".yaml")).as_slice()
        };
    }
    let ctx = context();
    let old_inputs = decoded(&[("old", fixture!("minimal-x"))], &ctx);
    let old = build(&old_inputs, &ctx);
    let scalar = ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })
    .unwrap();
    let decoder = ProfileDecoder::new(scalar);
    for (bytes, expected, path) in [
        (joint!("duplicate-key"), ProfileErrorCode::Decode, None),
        (joint!("anchor-alias"), ProfileErrorCode::Decode, None),
        (joint!("merge-key"), ProfileErrorCode::Decode, None),
        (
            joint!("multiple-documents"),
            ProfileErrorCode::QuotaExceeded,
            None,
        ),
        (
            joint!("numeric-language-decimal"),
            ProfileErrorCode::InvalidField,
            Some("language"),
        ),
        (
            joint!("numeric-language-float-magnitude"),
            ProfileErrorCode::InvalidField,
            Some("language"),
        ),
        (
            joint!("numeric-language-hex-magnitude"),
            ProfileErrorCode::InvalidField,
            Some("language"),
        ),
        (
            joint!("numeric-language-octal-magnitude"),
            ProfileErrorCode::InvalidField,
            Some("language"),
        ),
        (
            joint!("exclusive-quoted-bool"),
            ProfileErrorCode::InvalidField,
            Some("exclusive"),
        ),
        (
            joint!("query-half-value-pair"),
            ProfileErrorCode::InvalidField,
            Some("queries.value_capture"),
        ),
        (
            joint!("query-half-parent-pair"),
            ProfileErrorCode::InvalidField,
            Some("queries.parent_capture"),
        ),
        (
            joint!("query-missing-pattern"),
            ProfileErrorCode::InvalidField,
            Some("queries.pattern"),
        ),
    ] {
        let id = ProfileId::new("joint", 16).unwrap();
        let error = decoder
            .decode(ProfileInput { id: &id, bytes }, &input_limits(), &ctx)
            .err()
            .unwrap();
        assert_eq!(error.code(), expected);
        if let Some(path) = path {
            assert_eq!(error.schema_path(), Some(path));
        }

        assert_eq!(old.profiles().len(), 1);
        assert_eq!(profile(&old, "x").grammar().value(), "tree-sitter-x");
    }
    for bytes in [
        joint!("relation-without-graph-label"),
        joint!("opaque-malformed-query"),
    ] {
        let inputs = decoded(&[("valid-joint", bytes)], &ctx);
        let next = build(&inputs, &ctx);
        let q = profile(&next, "x").queries().value().as_slice()[0].value();
        if *q.r#type().value() == QueryKind::Relation {
            assert!(q.graph_label().value().is_empty());
            assert_eq!(q.relation_type().value(), "CALLS");
        } else {
            assert_eq!(q.pattern().value(), "((((");
        }
    }
}

#[test]
fn unicode_crlf_bom_and_original_scalar_sites_survive_materialization() {
    for bytes in [
        include_bytes!("fixtures/catalog/joint/unicode-crlf.yaml").as_slice(),
        include_bytes!("fixtures/catalog/joint/bom-origin.yaml").as_slice(),
    ] {
        let ctx = context();
        let inputs = decoded(&[("original", bytes)], &ctx);
        let draft = build(&inputs, &ctx);
        let p = profile(&draft, "x");
        let scalar = ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap();
        let audit = scalar
            .audit(
                bytes,
                &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 64).unwrap(),
            )
            .unwrap();
        let language = match p.language().provenance() {
            ProvenanceView::Declared(site) => site,
            _ => panic!("original language source"),
        };
        assert_eq!(language.location().kind(), LocationKind::ScalarExact);
        assert_eq!(audit.text_at(language.span()).unwrap(), "x");
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            assert_eq!((language.span().start(), language.span().end()), (13, 14));
        }
        for extension in p.extensions().value().iter() {
            let source = match extension.provenance() {
                ProvenanceView::Declared(site) => site,
                _ => panic!("original extension source"),
            };
            assert_eq!(extension.value(), "α😀");
            assert_eq!(audit.text_at(source.span()).unwrap(), "α😀");
        }
    }
}

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
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        Cancellation::default(),
    )
}

fn input_limits() -> ProfileLimits {
    ProfileLimits::new(ProfileLimitsInput {
        input_bytes: 32768,
        nodes: 2048,
        depth: 16,
        text_bytes: 32768,
        scalar_bytes: 4096,
        collection_entries: 256,
    })
    .unwrap()
}
fn catalog_input() -> CatalogLimitsInput {
    CatalogLimitsInput {
        policy_entries: 256,
        selector_bytes: 4096,
    }
}
fn limits() -> CatalogLimits {
    CatalogLimits::new(catalog_input()).unwrap()
}
fn decode_with_limits(
    bytes: &[u8],
    label: &str,
    ctx: &SnapshotContext,
    policy: &ProfileLimits,
) -> DecodedProfile {
    let id = ProfileId::new(label, 256).unwrap();
    let scalar = ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })
    .unwrap();
    ProfileDecoder::new(scalar)
        .decode(ProfileInput { id: &id, bytes }, policy, ctx)
        .unwrap()
}
fn decoded(inputs: &[(&str, &[u8])], ctx: &SnapshotContext) -> Vec<DecodedProfile> {
    let corpus_bytes = inputs
        .iter()
        .map(|(_, bytes)| bytes.len())
        .max()
        .unwrap_or(1);
    let policy = ProfileLimits::new(ProfileLimitsInput {
        input_bytes: corpus_bytes.max(32768),
        nodes: corpus_bytes.max(2048),
        depth: 16,
        text_bytes: corpus_bytes.max(32768),
        scalar_bytes: 4096,
        collection_entries: 256,
    })
    .unwrap();
    inputs
        .iter()
        .map(|(name, bytes)| decode_with_limits(bytes, name, ctx, &policy))
        .collect()
}
fn build(profiles: &[DecodedProfile], ctx: &SnapshotContext) -> CatalogDraft {
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles,
        }],
        &limits(),
        ctx,
    )
    .unwrap()
}
fn code<T>(result: Result<T, CatalogError>) -> CatalogErrorCode {
    match result {
        Ok(_) => panic!("expected catalog error"),
        Err(error) => error.code(),
    }
}
fn selected<'a>(result: Selection<'a>) -> Vec<&'a str> {
    match result {
        Selection::Candidates(items) => items
            .iter()
            .map(|p| p.language().value().as_str())
            .collect(),
        Selection::Skip(reason) => panic!("unexpected skip {reason:?}"),
    }
}
fn profile<'a>(draft: &'a CatalogDraft, language: &str) -> &'a EffectiveProfile {
    draft
        .profiles()
        .iter()
        .find(|p| p.language().value() == language)
        .unwrap()
}
fn patterns(p: &EffectiveProfile) -> Vec<&str> {
    p.queries()
        .value()
        .iter()
        .map(|q| q.value().pattern().value().as_str())
        .collect()
}

#[test]
fn all_45_real_profiles_retain_inventory_and_declarative_routes() {
    let ctx = context();
    let inputs = decoded(&CORPUS, &ctx);
    let draft = build(&inputs, &ctx);
    assert_eq!(draft.profiles().len(), CORPUS.len());
    for (_, source) in CORPUS {
        let raw: serde_json::Value =
            serde_saphyr::from_str(std::str::from_utf8(source).unwrap()).unwrap();
        let expected: Vec<_> = raw["queries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|query| query["pattern"].as_str().unwrap())
            .collect();
        assert_eq!(
            patterns(profile(&draft, raw["language"].as_str().unwrap())),
            expected
        );
    }
    assert_eq!(
        draft
            .profiles()
            .iter()
            .filter(|p| *p.exclusive().value())
            .count(),
        5
    );
    assert_eq!(
        draft
            .profiles()
            .iter()
            .filter(|p| *p.parser().value() == ProfileParser::TreeSitter)
            .count(),
        40
    );
    for (index, p) in draft.profiles().iter().enumerate() {
        for previous in &draft.profiles()[..index] {
            assert!(
                !p.language()
                    .value()
                    .eq_ignore_ascii_case(previous.language().value())
            );
            assert!(
                !p.grammar()
                    .value()
                    .eq_ignore_ascii_case(previous.grammar().value())
            );
        }
    }
    let policy = SelectionPolicy::new(Vec::new(), Vec::new(), &limits(), &ctx).unwrap();
    for (basename, extension, grammar, want) in [
        ("file.sql", Some(".sql"), None, "sql"),
        ("file.cob", Some(".cob"), None, "cobol85"),
        ("Dockerfile", None, None, "dockerfile"),
        ("module.mts", Some(".mts"), None, "typescript"),
        ("module.cts", Some(".cts"), None, "typescript"),
        ("types.d.mts", Some(".mts"), None, "typescript"),
        ("types.d.cts", Some(".cts"), None, "typescript"),
        ("module.cjs", Some(".cjs"), None, "javascript"),
        (
            "file.sql",
            Some(".sql"),
            Some("antlr-postgresql"),
            "postgresql",
        ),
        ("file.pks", Some(".pks"), Some("antlr-plsql"), "plsql"),
        (
            "file.plpgsql",
            Some(".plpgsql"),
            Some("tree-sitter-plpgsql"),
            "plpgsql",
        ),
    ] {
        assert_eq!(
            selected(
                draft
                    .resolve_file(
                        FileSelector {
                            basename,
                            extension,
                            explicit_grammar: grammar
                        },
                        &policy,
                        &ctx
                    )
                    .unwrap()
            ),
            [want]
        );
    }
    for (basename, extension) in [("file.pks", ".pks"), ("file.plpgsql", ".plpgsql")] {
        assert!(matches!(
            draft
                .resolve_file(
                    FileSelector {
                        basename,
                        extension: Some(extension),
                        explicit_grammar: None
                    },
                    &policy,
                    &ctx
                )
                .unwrap(),
            Selection::Skip(SelectionSkip::NoSelector)
        ));
    }
}

#[test]
fn minimal_profiles_preserve_complete_defaults_and_provenance() {
    let oracle: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/catalog/manual-effective-shape.json"
    ))
    .unwrap();
    let ctx = context();
    let input = decoded(
        &[("x", fixture!("minimal-x")), ("y", fixture!("minimal-y"))],
        &ctx,
    );
    let layers = [ProfileLayer {
        kind: ProfileLayerKind::Runtime,
        profiles: &input,
    }];
    let draft = CatalogDraft::build(&layers, &limits(), &ctx).unwrap();
    assert_eq!(profile_shape(profile(&draft, "x"), None), oracle["x"]);
    let mut expected_y = oracle["x"].clone();
    expected_y["language"] = serde_json::json!("y");
    expected_y["grammar"] = serde_json::json!("tree-sitter-y");
    assert_eq!(profile_shape(profile(&draft, "y"), None), expected_y);
    let x = profile(&draft, "x");
    assert_eq!(x.grammar().value(), "tree-sitter-x");
    assert!(x.start_rule().value().is_none());
    assert!(!*x.exclusive().value());
    assert!(matches!(
        x.exclusive().provenance(),
        ProvenanceView::Default
    ));
    assert!(matches!(x.grammar().provenance(), ProvenanceView::Derived(sites) if sites.len() == 1));
}

#[test]
fn ordered_query_merge_replacement_and_embedded_first_claim_declarations() {
    let ctx = context();
    let runtime = decoded(
        &[
            ("runtime", fixture!("overlay-runtime")),
            ("y", fixture!("minimal-y")),
        ],
        &ctx,
    );
    let user = decoded(&[("user", fixture!("overlay-user"))], &ctx);
    let project = decoded(&[("project", fixture!("overlay-project"))], &ctx);
    let draft = CatalogDraft::build(
        &[
            ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &runtime,
            },
            ProfileLayer {
                kind: ProfileLayerKind::User,
                profiles: &user,
            },
            ProfileLayer {
                kind: ProfileLayerKind::Project,
                profiles: &project,
            },
        ],
        &limits(),
        &ctx,
    )
    .unwrap();
    let x = profile(&draft, "x");
    assert_eq!(patterns(x), ["b", "a3", "a4", "c"]);
    assert!(!*x.exclusive().value());
    assert_eq!(draft.profiles().len(), 2);
    let embedded = x.embedded().value().as_slice();
    assert_eq!(
        embedded
            .iter()
            .map(|v| v.value().pattern().value().as_str())
            .collect::<Vec<_>>(),
        ["upper", "base"]
    );
    assert_eq!(
        embedded[0]
            .value()
            .mapped_language("SCSS", &limits(), &ctx)
            .unwrap(),
        Some(EmbeddedLanguageDecision::Skip)
    );
    assert_eq!(
        embedded[0]
            .value()
            .mapped_language("unknown", &limits(), &ctx)
            .unwrap(),
        Some(EmbeddedLanguageDecision::Skip)
    );
    assert_eq!(
        embedded[1]
            .value()
            .mapped_language("unknown", &limits(), &ctx)
            .unwrap(),
        None
    );
    let replacement = decoded(&[("replacement", fixture!("replacement-project"))], &ctx);
    let next = CatalogDraft::build(
        &[
            ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &runtime,
            },
            ProfileLayer {
                kind: ProfileLayerKind::Project,
                profiles: &replacement,
            },
        ],
        &limits(),
        &ctx,
    )
    .unwrap();
    assert_eq!(patterns(profile(&next, "x")), ["c"]);
    assert_eq!(patterns(x), ["b", "a3", "a4", "c"]);
    for (q, origin, data_key, pattern) in x
        .queries()
        .value()
        .iter()
        .zip(["runtime", "user", "user", "project"])
        .zip([("b", "b"), ("a", "a3"), ("a", "a4"), ("c", "c")])
        .map(|((q, origin), (key, pattern))| (q, origin, key, pattern))
    {
        let ProvenanceView::Declared(site) = q.provenance() else {
            panic!("query lost declared origin")
        };
        assert_eq!(site.location().kind(), LocationKind::NodeStart);
        assert_eq!(
            draft.source_profile(site.source_index()).unwrap().as_str(),
            origin
        );
        assert_eq!(q.value().data_key().value(), data_key);
        assert_eq!(q.value().pattern().value(), pattern);
        for field in [q.value().data_key(), q.value().pattern()] {
            let ProvenanceView::Declared(site) = field.provenance() else {
                panic!("query field source")
            };
            assert_eq!(site.location().kind(), LocationKind::ScalarExact);
            assert_eq!(
                draft.source_profile(site.source_index()).unwrap().as_str(),
                origin
            );
        }
    }
    let ProvenanceView::Derived(sites) = x.queries().provenance() else {
        panic!("merged list provenance")
    };
    assert_eq!(
        sites
            .iter()
            .map(|s| draft.source_profile(s.source_index()).unwrap().as_str())
            .collect::<Vec<_>>(),
        ["runtime", "user", "project"]
    );
    for (entry, origin) in embedded.iter().zip(["user", "runtime"]) {
        let ProvenanceView::Declared(site) = entry.provenance() else {
            panic!("embedded entry source")
        };
        assert_eq!(
            draft.source_profile(site.source_index()).unwrap().as_str(),
            origin
        );
    }
}

#[test]
fn map_empty_removal_clear_and_compound_replacement_are_distinct() {
    let ctx = context();
    let base = decoded(&[("base", fixture!("map-runtime"))], &ctx);
    for (name, bytes) in [
        ("remove", fixture!("map-remove")),
        ("empty", fixture!("map-empty")),
        ("clear", fixture!("map-clear")),
        ("lists", fixture!("list-clear")),
    ] {
        let upper = decoded(&[(name, bytes)], &ctx);
        let draft = CatalogDraft::build(
            &[
                ProfileLayer {
                    kind: ProfileLayerKind::Runtime,
                    profiles: &base,
                },
                ProfileLayer {
                    kind: ProfileLayerKind::Project,
                    profiles: &upper,
                },
            ],
            &limits(),
            &ctx,
        )
        .unwrap();
        let x = profile(&draft, "x");
        match name {
            "remove" => {
                let entries: Vec<_> = x
                    .context_types()
                    .value()
                    .iter()
                    .map(|(k, v)| (k.value().as_str(), v.value().as_str()))
                    .collect();
                assert_eq!(entries, [("keep", "Method"), ("new", "Class")]);
                assert!(x.complexity().value().head_calls().value().is_none());
                assert_eq!(x.complexity().value().node_types().value().len(), 1);
            }
            "empty" => {
                assert!(x.context_types().value().is_empty());
                assert!(x.complexity().value().head_calls().value().is_some());
                assert_eq!(
                    *x.exports().value().strategy().value(),
                    ExportStrategy::None
                );
                assert!(x.exports().value().config().value().is_empty());
            }
            "clear" => {
                assert!(x.complexity().value().node_types().value().is_empty());
                assert!(x.complexity().value().head_calls().value().is_none());
                assert_eq!(
                    *x.exports().value().strategy().value(),
                    ExportStrategy::None
                );
                assert!(matches!(
                    x.complexity().provenance(),
                    ProvenanceView::Default
                ));
            }
            "lists" => {
                assert!(x.queries().value().is_empty());
                assert!(x.embedded().value().is_empty());
                assert!(x.extensions().value().is_empty());
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn full_grammar_priority_survives_order_case_and_denied_alias() {
    let ctx = context();
    for reverse in [false, true] {
        let mut inputs = decoded(
            &[
                ("x", fixture!("full-first-x")),
                ("prefixed", fixture!("full-first-prefixed")),
            ],
            &ctx,
        );
        if reverse {
            inputs.reverse();
        }
        let draft = build(&inputs, &ctx);
        let all = SelectionPolicy::new(Vec::new(), Vec::new(), &limits(), &ctx).unwrap();
        for selector in ["tree-sitter-x", "TREE-SITTER-X"] {
            assert_eq!(
                selected(
                    draft
                        .resolve_file(
                            FileSelector {
                                basename: "",
                                extension: None,
                                explicit_grammar: Some(selector)
                            },
                            &all,
                            &ctx
                        )
                        .unwrap()
                ),
                ["x"]
            );
        }
        assert_eq!(
            selected(
                draft
                    .resolve_file(
                        FileSelector {
                            basename: "",
                            extension: None,
                            explicit_grammar: Some("tree-sitter-tree-sitter-x")
                        },
                        &all,
                        &ctx
                    )
                    .unwrap()
            ),
            ["tree-sitter-x"]
        );
        for alias in ["X", "TREE-SITTER-X"] {
            let blocked =
                SelectionPolicy::new(Vec::new(), vec![alias.into()], &limits(), &ctx).unwrap();
            assert_eq!(
                code(draft.resolve_file(
                    FileSelector {
                        basename: "",
                        extension: None,
                        explicit_grammar: Some("tree-sitter-x")
                    },
                    &blocked,
                    &ctx
                )),
                CatalogErrorCode::DisabledGrammar
            );
        }
    }
}

#[test]
fn bare_alias_ambiguity_precedes_policy_and_unknown_remains_unknown() {
    let ctx = context();
    let mut inputs = decoded(
        &[
            ("ts", fixture!("ordered-ts")),
            ("antlr", fixture!("ordered-antlr")),
        ],
        &ctx,
    );
    let draft = build(&inputs, &ctx);
    let filter = SelectionPolicy::new(
        vec!["ts_alias".into(), "unknown".into()],
        vec!["antlr_alias".into()],
        &limits(),
        &ctx,
    )
    .unwrap();
    assert_eq!(
        code(draft.resolve_file(
            FileSelector {
                basename: "",
                extension: None,
                explicit_grammar: Some("shared")
            },
            &filter,
            &ctx
        )),
        CatalogErrorCode::AmbiguousSelector
    );
    assert_eq!(
        code(draft.resolve_file(
            FileSelector {
                basename: "",
                extension: None,
                explicit_grammar: Some("unknown")
            },
            &filter,
            &ctx
        )),
        CatalogErrorCode::UnknownGrammar
    );
    assert_eq!(
        selected(
            draft
                .resolve_file(
                    FileSelector {
                        basename: "",
                        extension: None,
                        explicit_grammar: Some("TREE-SITTER-SHARED")
                    },
                    &filter,
                    &ctx
                )
                .unwrap()
        ),
        ["ts_alias"]
    );
    assert_eq!(
        code(SelectionPolicy::new(
            vec!["x".into(), "X".into()],
            Vec::new(),
            &limits(),
            &ctx
        )),
        CatalogErrorCode::InvalidPolicy
    );
    assert_eq!(
        code(SelectionPolicy::new(
            vec![" x".into()],
            Vec::new(),
            &limits(),
            &ctx
        )),
        CatalogErrorCode::InvalidPolicy
    );
    let all = SelectionPolicy::new(Vec::new(), Vec::new(), &limits(), &ctx).unwrap();
    let no_known_alias =
        SelectionPolicy::new(vec!["unknown".into()], Vec::new(), &limits(), &ctx).unwrap();
    for reverse in [false, true] {
        if reverse {
            inputs.reverse();
        }
        let ordered = build(&inputs, &ctx);
        let selector = FileSelector {
            basename: "file.shared",
            extension: Some(".shared"),
            explicit_grammar: None,
        };
        assert_eq!(
            selected(ordered.resolve_file(selector, &all, &ctx).unwrap()),
            ["ts_alias", "antlr_alias"]
        );
        assert_eq!(
            selected(ordered.resolve_file(selector, &filter, &ctx).unwrap()),
            ["ts_alias"]
        );
        assert!(matches!(
            ordered
                .resolve_file(selector, &no_known_alias, &ctx)
                .unwrap(),
            Selection::Skip(SelectionSkip::Filtered)
        ));
    }
}

#[test]
fn exclusive_routes_and_global_grammar_conflicts_preserve_previous_draft() {
    let ctx = context();
    let inputs = decoded(
        &[
            ("a", fixture!("exclusive-a")),
            ("b", fixture!("exclusive-b")),
            ("ordinary", fixture!("nonexclusive-a")),
        ],
        &ctx,
    );
    let draft = build(&inputs, &ctx);
    assert_eq!(draft.profiles().len(), 3);
    let all = SelectionPolicy::new(Vec::new(), Vec::new(), &limits(), &ctx).unwrap();
    assert_eq!(
        selected(
            draft
                .resolve_file(
                    FileSelector {
                        basename: "file.shared",
                        extension: Some(".shared"),
                        explicit_grammar: None
                    },
                    &all,
                    &ctx
                )
                .unwrap()
        ),
        ["ordinary_a"]
    );
    let blocked =
        SelectionPolicy::new(Vec::new(), vec!["ordinary_a".into()], &limits(), &ctx).unwrap();
    assert!(matches!(
        draft
            .resolve_file(
                FileSelector {
                    basename: "file.shared",
                    extension: Some(".shared"),
                    explicit_grammar: None
                },
                &blocked,
                &ctx
            )
            .unwrap(),
        Selection::Skip(SelectionSkip::Filtered)
    ));
    for selector in ["exclusive_a", "tree-sitter-exclusive_b"] {
        assert_eq!(
            selected(
                draft
                    .resolve_file(
                        FileSelector {
                            basename: "",
                            extension: None,
                            explicit_grammar: Some(selector)
                        },
                        &all,
                        &ctx
                    )
                    .unwrap()
            )
            .len(),
            1
        );
    }
    for (pairs, want) in [
        (
            [
                ("a", fixture!("nonexclusive-a")),
                ("b", fixture!("nonexclusive-b")),
            ],
            CatalogErrorCode::AmbiguousSelector,
        ),
        (
            [
                ("x", fixture!("grammar-collision-x")),
                ("y", fixture!("grammar-collision-y")),
            ],
            CatalogErrorCode::DuplicateGrammar,
        ),
    ] {
        let bad = decoded(&pairs, &ctx);
        let error = CatalogDraft::build(
            &[ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &bad,
            }],
            &limits(),
            &ctx,
        )
        .err()
        .unwrap();
        assert_eq!(error.code(), want);
        assert!(error.site().is_some());
        assert!(error.conflicting_site().is_some());
        assert!(error.schema_path().is_some());
    }
    assert_eq!(draft.profiles().len(), 3);
    assert_eq!(profile(&draft, "ordinary_a").extensions().value().len(), 1);
}

#[test]
fn every_nested_getter_retains_declared_values_and_original_sites() {
    let ctx = context();
    let bytes = fixture!("all-fields");
    let inputs = decoded(&[("all-fields", bytes)], &ctx);
    let draft = build(&inputs, &ctx);
    let p = profile(&draft, "tracked_lang");
    let expected: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/catalog/all-fields-expected.json"))
            .unwrap();
    let scalar = ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })
    .unwrap();
    let audit = scalar
        .audit(
            bytes,
            &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 64).unwrap(),
        )
        .unwrap();
    assert_eq!(profile_shape(p, Some(&audit)), expected);
    assert_eq!(
        text_pairs(p.exports().value().config()),
        [("node_type", "modifier"), ("name", "public")]
    );
    assert_eq!(
        text_pairs(p.context_types()),
        [("class_declaration", "Type")]
    );
    assert_eq!(
        text_pairs(p.context_name_queries()),
        [(
            "class_declaration",
            "(class_declaration name: (_) @name) @context"
        )]
    );
    assert_eq!(
        text_pairs(p.embedded().value().as_slice()[0].value().languages()),
        [("sql", "sql"), ("scss", "")]
    );
    assert_eq!(
        text_pairs(
            p.text_normalizers()
                .value()
                .iter()
                .next()
                .unwrap()
                .1
                .value()
                .replace()
        ),
        [("&lt;", "<"), ("\\r\\n", "\\n")]
    );
    assert_eq!(p.grammar().value(), "tree-sitter-custom");
    assert_eq!(p.start_rule().value().as_deref(), Some("source_file"));
    for array in [
        p.extensions(),
        p.self_keywords(),
        p.anon_func_types(),
        p.declaration_types(),
        p.comment_types(),
    ] {
        assert_eq!(array.value().len(), 1);
    }
    assert_eq!(
        p.embed_labels()
            .value()
            .iter()
            .map(|v| v.value().as_str())
            .collect::<Vec<_>>(),
        ["Function", "Type", "Function"]
    );
    let q = p.queries().value().as_slice()[0].value();
    for (value, want) in [
        (q.data_key(), "declarations"),
        (q.graph_label(), "Function"),
        (q.name_capture(), "name"),
        (q.relation_type(), ""),
        (q.value_capture(), "value"),
        (q.value_label(), "Value"),
        (q.name_reject(), "^internal_"),
        (q.span_capture(), "span"),
        (q.declaration_capture(), "span"),
        (q.context_capture(), "span"),
        (q.parent_capture(), "parent"),
        (q.parent_label(), "Type"),
        (q.qualifier_capture(), "qualifier"),
        (q.target_fallback(), "file"),
    ] {
        assert_eq!(value.value(), want);
    }
    assert!(q.pattern().value().contains("function_declaration"));
    assert_eq!(*q.r#type().value(), QueryKind::Entity);
    assert!(*q.name_is_data().value());
    assert_eq!(
        q.target_labels()
            .value()
            .iter()
            .map(|v| v.value().as_str())
            .collect::<Vec<_>>(),
        ["Function", "CustomLabel"]
    );
    let exports = p.exports().value();
    assert_eq!(*exports.strategy().value(), ExportStrategy::Modifier);
    assert_eq!(exports.config().value().len(), 2);
    assert_eq!(
        exports
            .config_list()
            .value()
            .iter()
            .next()
            .unwrap()
            .1
            .value()
            .len(),
        2
    );
    assert_eq!(p.context_types().value().len(), 1);
    assert_eq!(p.context_name_queries().value().len(), 1);
    let target = p.target_rules().value().iter().next().unwrap().1.value();
    assert_eq!(target.fallback().value(), "file");
    assert_eq!(target.labels().value().len(), 2);
    let e = p.embedded().value().as_slice()[0].value();
    for (value, want) in [
        (e.pattern(), "(template_string) @text"),
        (e.text_capture(), "text"),
        (e.lang_capture(), "language"),
        (e.r#default(), "sql"),
        (e.wrap_prefix(), "select "),
        (e.wrap_suffix(), ";"),
        (e.normalize(), "html_refs"),
    ] {
        assert_eq!(value.value(), want);
    }
    assert_eq!(e.host_labels().value().len(), 1);
    assert_eq!(e.languages().value().len(), 2);
    assert_eq!(
        e.mapped_language("SQL", &limits(), &ctx).unwrap(),
        Some(EmbeddedLanguageDecision::Language("sql"))
    );
    assert_eq!(
        e.mapped_language("scss", &limits(), &ctx).unwrap(),
        Some(EmbeddedLanguageDecision::Skip)
    );
    let normalizer = p
        .text_normalizers()
        .value()
        .iter()
        .next()
        .unwrap()
        .1
        .value();
    assert!(*normalizer.numeric_char_refs().value());
    assert_eq!(normalizer.replace().value().len(), 2);
    let c = p.complexity().value();
    assert_eq!(c.node_types().value().len(), 1);
    assert_eq!(c.operators().value().len(), 2);
    let head = c.head_calls().value().as_ref().unwrap();
    assert_eq!(head.node_type().value(), "call_expression");
    assert_eq!(head.names().value().len(), 1);
    assert_eq!(head.pair_names().value().len(), 1);
    assert_eq!(head.subject_pair_names().value().len(), 1);
    let site = match p.language().provenance() {
        ProvenanceView::Declared(site) => site,
        _ => panic!("language origin"),
    };
    assert_eq!(site.location().kind(), LocationKind::ScalarExact);
    assert_eq!(
        draft.source_profile(site.source_index()).unwrap().as_str(),
        "all-fields"
    );
    assert_eq!(
        &bytes[site.span().start()..site.span().end()],
        b"tracked_lang"
    );
    let compound = match p.exports().provenance() {
        ProvenanceView::Declared(site) => site,
        _ => panic!("exports origin"),
    };
    assert_eq!(compound.location().kind(), LocationKind::NodeStart);
    assert!(compound.span().end() < bytes.len());
}

#[test]
fn grammar_missing_inherits_empty_rederives_and_custom_queries_stay_opaque() {
    let ctx = context();
    let base = decoded(&[("base", fixture!("overlay-runtime"))], &ctx);
    for (name, bytes, grammar) in [
        ("inherit", fixture!("grammar-inherit"), "tree-sitter-x"),
        ("derive", fixture!("grammar-rederive"), "antlr-x"),
    ] {
        let upper = decoded(&[(name, bytes)], &ctx);
        let draft = CatalogDraft::build(
            &[
                ProfileLayer {
                    kind: ProfileLayerKind::Runtime,
                    profiles: &base,
                },
                ProfileLayer {
                    kind: ProfileLayerKind::Project,
                    profiles: &upper,
                },
            ],
            &limits(),
            &ctx,
        )
        .unwrap();
        assert_eq!(profile(&draft, "x").grammar().value(), grammar);
        assert_eq!(
            *profile(&draft, "x").parser().value(),
            ProfileParser::Antlr4
        );
    }
    let custom = decoded(
        &[
            ("custom", fixture!("custom")),
            ("markdown", fixture!("markdown")),
        ],
        &ctx,
    );
    let draft = build(&custom, &ctx);
    assert_eq!(patterns(profile(&draft, "tracked_lang")), ["(invalid"]);
    assert_eq!(
        profile(&draft, "tracked_lang").extensions().value().len(),
        2
    );
    let all = SelectionPolicy::new(Vec::new(), Vec::new(), &limits(), &ctx).unwrap();
    assert_eq!(
        selected(
            draft
                .resolve_file(
                    FileSelector {
                        basename: "a.tracked",
                        extension: Some(".TRACKED"),
                        explicit_grammar: None
                    },
                    &all,
                    &ctx
                )
                .unwrap()
        ),
        ["tracked_lang"]
    );
    assert_eq!(
        selected(
            draft
                .resolve_file(
                    FileSelector {
                        basename: "a.md",
                        extension: Some(".md"),
                        explicit_grammar: None
                    },
                    &all,
                    &ctx
                )
                .unwrap()
        ),
        ["markdown"]
    );
    for bad in ["", "tracked", " .tracked", ".tracked ", ".x\0"] {
        assert_eq!(
            code(draft.resolve_file(
                FileSelector {
                    basename: "a",
                    extension: Some(bad),
                    explicit_grammar: None
                },
                &all,
                &ctx
            )),
            CatalogErrorCode::InvalidSelector
        );
    }
}

#[test]
fn cancellation_deadline_and_layer_errors_are_closed() {
    let token = Cancellation::default();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(120), token.clone());
    let inputs = decoded(&[("x", fixture!("minimal-x"))], &ctx);
    let draft = build(&inputs, &ctx);
    let all = SelectionPolicy::new(Vec::new(), Vec::new(), &limits(), &ctx).unwrap();
    assert_eq!(
        code(CatalogDraft::build(
            &[
                ProfileLayer {
                    kind: ProfileLayerKind::Project,
                    profiles: &inputs
                },
                ProfileLayer {
                    kind: ProfileLayerKind::Runtime,
                    profiles: &inputs
                }
            ],
            &limits(),
            &ctx
        )),
        CatalogErrorCode::InvalidLayer
    );
    let duplicate = decoded(
        &[("first", b"language: X\n"), ("second", b"language: x\n")],
        &ctx,
    );
    let error = CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &duplicate,
        }],
        &limits(),
        &ctx,
    )
    .err()
    .unwrap();
    assert_eq!(error.code(), CatalogErrorCode::DuplicateLanguage);
    assert_eq!(error.layer_index(), Some(0));
    assert_eq!(error.profile_index(), Some(1));
    assert_eq!(error.schema_path(), Some("language"));
    token.cancel();
    assert_eq!(
        code(CatalogDraft::build(&[], &limits(), &ctx)),
        CatalogErrorCode::Cancelled
    );
    assert_eq!(
        code(SelectionPolicy::new(
            Vec::new(),
            Vec::new(),
            &limits(),
            &ctx
        )),
        CatalogErrorCode::Cancelled
    );
    assert_eq!(
        code(draft.resolve_file(
            FileSelector {
                basename: "",
                extension: None,
                explicit_grammar: Some("x")
            },
            &all,
            &ctx
        )),
        CatalogErrorCode::Cancelled
    );

    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert_eq!(
        code(CatalogDraft::build(&[], &limits(), &expired)),
        CatalogErrorCode::DeadlineExceeded
    );
}

#[test]
fn exact_and_one_over_policy_selector_and_embedded_domain_limits() {
    let ctx = context();
    let inputs = decoded(&[("x", fixture!("minimal-x"))], &ctx);
    let mut cap = catalog_input();
    cap.policy_entries = 2;
    cap.selector_bytes = "tree-sitter-x".len();
    let cap = CatalogLimits::new(cap).unwrap();
    let draft = CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &inputs,
        }],
        &cap,
        &ctx,
    )
    .unwrap();
    let policy =
        SelectionPolicy::new(vec!["tree-sitter-x".into()], vec!["y".into()], &cap, &ctx).unwrap();

    for (allow, deny) in [
        (vec!["x".into(), "z".into()], vec!["y".into()]),
        (vec!["tree-sitter-xy".into()], Vec::new()),
    ] {
        assert_eq!(
            code(SelectionPolicy::new(allow, deny, &cap, &ctx)),
            CatalogErrorCode::QuotaExceeded
        );
    }
    assert_eq!(
        selected(
            draft
                .resolve_file(
                    FileSelector {
                        basename: "",
                        extension: None,
                        explicit_grammar: Some("tree-sitter-x")
                    },
                    &policy,
                    &ctx
                )
                .unwrap()
        ),
        ["x"]
    );
    for valid in [
        FileSelector {
            basename: "xxxxxxxxxxxxx",
            extension: None,
            explicit_grammar: None,
        },
        FileSelector {
            basename: "",
            extension: Some(".xxxxxxxxxxxx"),
            explicit_grammar: None,
        },
    ] {
        assert!(matches!(
            draft.resolve_file(valid, &policy, &ctx).unwrap(),
            Selection::Skip(SelectionSkip::NoSelector)
        ));
    }
    for invalid in [
        FileSelector {
            basename: "xxxxxxxxxxxxxx",
            extension: None,
            explicit_grammar: None,
        },
        FileSelector {
            basename: "",
            extension: Some(".xxxxxxxxxxxxx"),
            explicit_grammar: None,
        },
        FileSelector {
            basename: "",
            extension: None,
            explicit_grammar: Some("tree-sitter-xy"),
        },
    ] {
        assert_eq!(
            code(draft.resolve_file(invalid, &policy, &ctx)),
            CatalogErrorCode::QuotaExceeded
        );
    }
    let inputs = decoded(&[("embedded", fixture!("all-fields"))], &ctx);
    let all = build(&inputs, &ctx);
    let e = profile(&all, "tracked_lang").embedded().value().as_slice()[0].value();
    let mut short = catalog_input();
    short.selector_bytes = 3;
    let short = CatalogLimits::new(short).unwrap();
    assert_eq!(
        e.mapped_language("SQL", &short, &ctx).unwrap(),
        Some(EmbeddedLanguageDecision::Language("sql"))
    );
    assert_eq!(
        code(e.mapped_language("SQLx", &short, &ctx)),
        CatalogErrorCode::QuotaExceeded
    );

    assert_eq!(
        code(e.mapped_language("S\0L", &short, &ctx)),
        CatalogErrorCode::InvalidSelector
    );
    assert_eq!(
        e.mapped_language("sql", &short, &ctx).unwrap(),
        Some(EmbeddedLanguageDecision::Language("sql"))
    );
}
#[test]
fn mixed_case_null_text_is_preserved_without_clear_or_normalization() {
    let ctx = context();
    let inputs = decoded(&[("mixed", fixture!("mixed-case-text"))], &ctx);
    let draft = build(&inputs, &ctx);
    let map = profile(&draft, "x").exports().value().config();
    assert_eq!(text_pairs(map), [("nUlL", "NuLl")]);
    let bytes = fixture!("mixed-case-text");
    let scalar = ScalarPolicy::new(RegexLimits {
        compiled_size_policy_bytes: 1 << 20,
        dfa_cache_policy_bytes: 1 << 20,
    })
    .unwrap();
    let audit = scalar
        .audit(
            bytes,
            &ScalarLimits::new(32768, 16, 8192, 4096, 32768, 32768, 64).unwrap(),
        )
        .unwrap();
    let (key, value) = map.value().iter().next().unwrap();
    assert_eq!(shape_text(key, Some(&audit)), serde_json::json!("nUlL"));
    assert_eq!(shape_text(value, Some(&audit)), serde_json::json!("NuLl"));
}

#[test]
fn query_source_default_and_merged_provenance_remain_explicit() {
    let ctx = context();
    let runtime = decoded(
        &[(
            "copy-runtime",
            b"language: cobol
parser: antlr4
grammar: antlr-cobol85
queries: [{data_key: old, graph_label: Main, pattern: '//programUnit', type: entity}]
",
        )],
        &ctx,
    );
    let project = decoded(&[("copy-project", b"language: cobol
merge: true
queries: [{data_key: imports, source: cobol-preprocessor, graph_label: Include, pattern: '//copyStatement/copySource', type: entity}, {data_key: imports, source: cobol-preprocessor, graph_label: Include, pattern: '//copyStatement/copySource', type: entity}]
")], &ctx);
    let draft = CatalogDraft::build(
        &[
            ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &runtime,
            },
            ProfileLayer {
                kind: ProfileLayerKind::Project,
                profiles: &project,
            },
        ],
        &limits(),
        &ctx,
    )
    .unwrap();
    let queries = profile(&draft, "cobol").queries().value().as_slice();
    assert_eq!(queries.len(), 3);
    assert_eq!(*queries[0].value().source().value(), QuerySource::Primary);
    assert!(matches!(
        queries[0].value().source().provenance(),
        ProvenanceView::Default
    ));
    for query in &queries[1..] {
        assert_eq!(
            *query.value().source().value(),
            QuerySource::CobolPreprocessor
        );
        let ProvenanceView::Declared(site) = query.value().source().provenance() else {
            panic!("source provenance")
        };
        assert_eq!(site.location().kind(), LocationKind::ScalarExact);
        assert_eq!(
            draft.source_profile(site.source_index()).unwrap().as_str(),
            "copy-project"
        );
    }
    assert!(!std::ptr::eq(queries[1].value(), queries[2].value()));
}

#[test]
fn declaration_capture_defaults_and_group_replacement_preserve_actual_provenance() {
    let ctx = context();
    let lower = decoded(&[("lower", b"language: x\nqueries:\n  - data_key: same\n    graph_label: Function\n    pattern: base\n    declaration_capture: lower_decl\n  - data_key: kept\n    graph_label: Function\n    pattern: kept\n    declaration_capture: kept_decl\n  - data_key: cleared\n    graph_label: Function\n    pattern: old\n    declaration_capture: old_decl\n")], &ctx);
    let upper = decoded(&[("upper", b"language: x\nmerge: true\nqueries:\n  - data_key: same\n    graph_label: Function\n    pattern: replacement\n  - data_key: added\n    graph_label: Function\n    pattern: added\n    declaration_capture: upper_decl\n  - data_key: cleared\n    graph_label: Function\n    pattern: cleared\n    declaration_capture: \"\"\n")], &ctx);
    let draft = CatalogDraft::build(
        &[
            ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &lower,
            },
            ProfileLayer {
                kind: ProfileLayerKind::User,
                profiles: &upper,
            },
        ],
        &limits(),
        &ctx,
    )
    .unwrap();
    let p = profile(&draft, "x");
    let find = |key: &str| {
        p.queries()
            .value()
            .iter()
            .map(|q| q.value())
            .find(|q| q.data_key().value().as_str() == key)
            .unwrap()
    };
    assert_eq!(find("same").pattern().value(), "replacement");
    assert_eq!(find("same").declaration_capture().value(), "");
    assert!(matches!(
        find("same").declaration_capture().provenance(),
        ProvenanceView::Default
    ));
    assert_eq!(find("kept").declaration_capture().value(), "kept_decl");
    assert_eq!(find("added").declaration_capture().value(), "upper_decl");
    assert_eq!(find("cleared").declaration_capture().value(), "");
    for key in ["added", "cleared"] {
        let ProvenanceView::Declared(site) = find(key).declaration_capture().provenance() else {
            panic!("declared site")
        };
        assert_eq!(site.source_index().as_u32(), 1);
        assert_eq!(site.location().kind(), LocationKind::ScalarExact);
    }
}

#[test]
fn required_context_list_inherits_replaces_and_clears_with_original_provenance() {
    let ctx = context();
    let lower = decoded(
        &[(
            "lower-context",
            b"language: x\nmerge: true\ncontext_types: {a: Type, b: Module}\ncontext_requires_entity: [a]\n",
        )],
        &ctx,
    );
    for (bytes, expected, source) in [
        (b"language: x\nmerge: true\n".as_slice(), vec!["a"], 0),
        (
            b"language: x\nmerge: true\ncontext_requires_entity: [b]\n".as_slice(),
            vec!["b"],
            1,
        ),
        (
            b"language: x\nmerge: true\ncontext_requires_entity: []\n".as_slice(),
            vec![],
            1,
        ),
    ] {
        let upper = decoded(&[("upper-context", bytes)], &ctx);
        let draft = CatalogDraft::build(
            &[
                ProfileLayer {
                    kind: ProfileLayerKind::Runtime,
                    profiles: &lower,
                },
                ProfileLayer {
                    kind: ProfileLayerKind::User,
                    profiles: &upper,
                },
            ],
            &limits(),
            &ctx,
        )
        .unwrap();
        let list = profile(&draft, "x").context_requires_entity();
        assert_eq!(
            list.value()
                .iter()
                .map(|v| v.value().as_str())
                .collect::<Vec<_>>(),
            expected
        );
        let ProvenanceView::Declared(site) = list.provenance() else {
            panic!("declared list site")
        };
        assert_eq!(site.source_index().as_u32(), source);
    }
}

#[test]
fn context_capture_defaults_and_group_replacement_preserve_actual_provenance() {
    let ctx = context();
    let lower = decoded(&[("lower", b"language: x\nqueries:\n  - data_key: same\n    graph_label: Function\n    pattern: base\n    context_capture: lower_decl\n  - data_key: kept\n    graph_label: Function\n    pattern: kept\n    context_capture: kept_decl\n  - data_key: cleared\n    graph_label: Function\n    pattern: old\n    context_capture: old_decl\n")], &ctx);
    let upper = decoded(&[("upper", b"language: x\nmerge: true\nqueries:\n  - data_key: same\n    graph_label: Function\n    pattern: replacement\n  - data_key: added\n    graph_label: Function\n    pattern: added\n    context_capture: upper_decl\n  - data_key: cleared\n    graph_label: Function\n    pattern: cleared\n    context_capture: \"\"\n")], &ctx);
    let draft = CatalogDraft::build(
        &[
            ProfileLayer {
                kind: ProfileLayerKind::Runtime,
                profiles: &lower,
            },
            ProfileLayer {
                kind: ProfileLayerKind::User,
                profiles: &upper,
            },
        ],
        &limits(),
        &ctx,
    )
    .unwrap();
    let p = profile(&draft, "x");
    let find = |key: &str| {
        p.queries()
            .value()
            .iter()
            .map(|q| q.value())
            .find(|q| q.data_key().value().as_str() == key)
            .unwrap()
    };
    assert_eq!(find("same").pattern().value(), "replacement");
    assert_eq!(find("same").context_capture().value(), "");
    assert!(matches!(
        find("same").context_capture().provenance(),
        ProvenanceView::Default
    ));
    assert_eq!(find("kept").context_capture().value(), "kept_decl");
    assert_eq!(find("added").context_capture().value(), "upper_decl");
    assert_eq!(find("cleared").context_capture().value(), "");
    for key in ["added", "cleared"] {
        let ProvenanceView::Declared(site) = find(key).context_capture().provenance() else {
            panic!("declared site")
        };
        assert_eq!(site.source_index().as_u32(), 1);
        assert_eq!(site.location().kind(), LocationKind::ScalarExact);
    }
}
