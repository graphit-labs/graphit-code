use arrow_array::{RecordBatch, StringArray};
use graphit_ast::indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_generic_model::{
        GenericComplexity, GenericDeclaration, GenericExtent, GenericModelError,
        GenericNameRepresentation, PreparedGenericTreeSitterModel,
    },
    tree_sitter_profile::CaptureBinding,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::error::Error;
use std::sync::Arc;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 16,
        path_component_bytes: 1024,
        identity_text_bytes: 32768,
        owner_depth: 64,
        uid_bytes: 1 << 20,
    })
    .unwrap()
}

fn catalog(bytes: &[u8], original: &SnapshotContext) -> Result<CatalogDraft, Box<dyn Error>> {
    let id = ProfileId::new("generic-fixture", 256)?;
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
        .map_err(|error| std::io::Error::other(error.to_string()))?];
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

fn config(extra: &str) -> String {
    format!(
        "language: native-fixture\ngrammar: tree-sitter-json\ncontext_types: {{}}\nanon_func_types: []\n{extra}\nqueries:\n  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key: (string) @name value: (_) @value) @unit'\n    span_capture: unit\n    value_capture: value\n    value_label: Value\n"
    )
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
fn generic_native_owner_keeps_raw_data_view_and_compiled_queries_after_factory_drop() -> TestResult
{
    let original = context();
    let draft = catalog(config("").as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?;
    let prepared = PreparedGenericTreeSitterModel::prepare(profile, contexts, &original)?;
    let source = "{\"café 日本\": 42}";
    with_source(source, &original, |input| {
        let parsed = prepared.collect(input, &["source.native"], limits())?;
        drop(prepared);
        let observations = parsed.observations()?;
        let matched = &observations.groups[0].matches[0];
        let CaptureBinding::Present(slots) = &matched.bindings.name else {
            panic!("name binding")
        };
        assert_eq!(slots.len(), 1);
        let name = parsed
            .capture_name(0, 0, slots[0])?
            .map_err(|e| format!("name rejected: {e:?}"))?;
        assert_eq!(name.raw().text(), "\"café 日本\"");
        assert_eq!(name.display(), "café 日本");
        assert_eq!(
            name.representation(),
            GenericNameRepresentation::Data {
                delimiter: Some("\"")
            }
        );
        assert_eq!(
            name.raw().text().as_ptr(),
            source[name.raw().start()..].as_ptr()
        );
        assert_eq!(
            name.display_range().text().as_ptr(),
            source[name.display_range().start()..].as_ptr()
        );
        let GenericExtent::Exact {
            extent, node_index, ..
        } = parsed.capture_extent(0, 0, slots[0])?
        else {
            panic!("full native span")
        };
        assert_eq!(extent.range().text(), &source[1..source.len() - 1]);
        assert_eq!(parsed.observations()?.nodes[node_index].kind, "pair");
        let identity = parsed.identity_document()?.symbol(
            "Pair",
            name.raw(),
            &[],
            graphit_ast::symbols::LogicalDeclaration::named(name.display()),
        )?;
        assert_eq!(identity.subject().text(), name.raw().text());
        let index = parsed.node_index(matched.captures[slots[0]].native.node_id)?;
        assert!(
            matches!(parsed.node_origin(index)?, OriginalProjection::Exact(r) if r.text() == name.raw().text())
        );
        let wrong_slot = matched
            .captures
            .iter()
            .enumerate()
            .find(|(slot, _)| !slots.contains(slot))
            .unwrap()
            .0;
        assert!(matches!(
            parsed.capture_name(0, 0, wrong_slot),
            Err(GenericModelError::NativeContract(_))
        ));
        Ok(())
    })
}

#[test]
fn generic_preparation_rejects_undeclared_roles_before_any_document() -> TestResult {
    let original = context();
    let yaml = config("").replace("span_capture: unit", "span_capture: absent");
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?;
    assert!(matches!(
        PreparedGenericTreeSitterModel::prepare(profile, contexts, &original),
        Err(GenericModelError::InvalidRole {
            query_index: 0,
            role: "span",
            ..
        })
    ));
    Ok(())
}

#[test]
fn generic_preparation_rejects_reserved_language_with_foreign_grammar() -> TestResult {
    for language in [
        "rust",
        "go",
        "javascript",
        "typescript",
        "tsx",
        "python",
        "json",
        "plsql",
        "db2",
        "postgresql",
        "tsql",
        "cobol85",
    ] {
        for language in [language.to_owned(), language.to_ascii_uppercase()] {
            let original = context();
            let yaml =
                config("").replace("language: native-fixture", &format!("language: {language}"));
            let draft = catalog(yaml.as_bytes(), &original)?;
            let profile = &draft.profiles()[0];
            let contexts = PreparedTreeSitterContexts::prepare(
                profile,
                builtin("tree-sitter-json")?,
                &original,
            )?;
            assert!(
                matches!(
                    PreparedGenericTreeSitterModel::prepare(profile, contexts, &original),
                    Err(GenericModelError::ReservedProfile)
                ),
                "{language}"
            );
        }
    }
    Ok(())
}

#[test]
fn generic_factory_cannot_mix_catalogs_even_with_equal_text() -> TestResult {
    let original = context();
    let a = catalog(config("").as_bytes(), &original)?;
    let b = catalog(config("").as_bytes(), &original)?;
    let contexts = PreparedTreeSitterContexts::prepare(
        &a.profiles()[0],
        builtin("tree-sitter-json")?,
        &original,
    )?;
    let prepared = PreparedGenericTreeSitterModel::prepare(&b.profiles()[0], contexts, &original)?;
    with_source("{}", &original, |input| {
        assert!(matches!(
            prepared.collect(input, &["source.native"], limits()),
            Err(GenericModelError::NativeContract(
                "generic factory profile mismatch"
            ))
        ));
        Ok(())
    })
}

#[test]
fn generic_extent_missing_does_not_borrow_an_unconfigured_capture_parent() -> TestResult {
    let original = context();
    let yaml = config("").replace("    span_capture: unit\n", "");
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?;
    let prepared = PreparedGenericTreeSitterModel::prepare(profile, contexts, &original)?;
    with_source("{\"name\": 42}", &original, |input| {
        let parsed = prepared.collect(input, &["source.native"], limits())?;
        let CaptureBinding::Present(slots) =
            &parsed.observations()?.groups[0].matches[0].bindings.name
        else {
            panic!("name binding")
        };
        assert!(matches!(
            parsed.capture_extent(0, 0, slots[0])?,
            GenericExtent::Missing { configured: false }
        ));
        Ok(())
    })
}

#[test]
fn generic_extent_keeps_conflicting_native_spans_instead_of_selecting_the_nearest() -> TestResult {
    let original = context();
    let yaml = config("").replace("(string) @name", "(string) @name @unit");
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?;
    let prepared = PreparedGenericTreeSitterModel::prepare(profile, contexts, &original)?;
    with_source("{\"name\": 42}", &original, |input| {
        let parsed = prepared.collect(input, &["source.native"], limits())?;
        let CaptureBinding::Present(slots) =
            &parsed.observations()?.groups[0].matches[0].bindings.name
        else {
            panic!("name binding")
        };
        let GenericExtent::Conflict { capture_indices } = parsed.capture_extent(0, 0, slots[0])?
        else {
            panic!("conflicting spans")
        };
        assert_eq!(capture_indices.len(), 2);
        Ok(())
    })
}

#[test]
fn explicit_clojure_declarations_count_head_calls_and_exclude_only_actual_nested_definitions()
-> TestResult {
    let original = context();
    let yaml = r#"language: clojure
grammar: tree-sitter-clojure
context_types: {}
anon_func_types: [anon_fn_lit]
declaration_types: [list_lit]
complexity:
  head_calls:
    node_type: list_lit
    names: [if]
    pair_names: [cond]
    subject_pair_names: [case]
queries:
  - data_key: functions
    graph_label: Function
    declaration_capture: declaration
    span_capture: declaration
    pattern: '(list_lit (sym_lit) @_def (sym_lit) @name (#eq? @_def "defn")) @declaration'
"#;
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-clojure")?, &original)?;
    let prepared = PreparedGenericTreeSitterModel::prepare(profile, contexts, &original)?;
    let text =
        "(defn f [x] (if x 1 0) (defn g [y] (if y 2 3)) (cond x 1 :else 0) (case x 1 :a 2 :b))";
    with_source(text, &original, |input| {
        let parsed = prepared.collect(input, &["source.clj"], limits())?;
        let matches = &parsed.observations()?.groups[0].matches;
        assert_eq!(matches.len(), 2);
        for (match_index, matched) in matches.iter().enumerate() {
            let CaptureBinding::Present(slots) = &matched.bindings.name else {
                panic!("name binding")
            };
            let name = parsed
                .capture_name(0, match_index, slots[0])?
                .map_err(|e| format!("name rejected: {e:?}"))?;
            let GenericDeclaration::Native { node_index, .. } =
                parsed.declaration(0, match_index, slots[0])?
            else {
                panic!("explicit declaration")
            };
            let GenericComplexity::Score {
                value, decisions, ..
            } = parsed.complexity(Some(node_index))?
            else {
                panic!("native score")
            };
            match name.display() {
                "f" => {
                    assert_eq!(value, 6);
                    assert_eq!(decisions.len(), 3);
                }
                "g" => {
                    assert_eq!(value, 2);
                    assert_eq!(decisions.len(), 1);
                }
                other => panic!("foreign function {other}"),
            }
        }
        Ok(())
    })
}

#[test]
fn generic_entities_keep_value_children_distinct_data_keys_and_real_owner_chains() -> TestResult {
    let original = context();
    let yaml = config("context_name_queries: {pair: '(pair key: (_) @name) @context'}\nembed_labels: [Pair, Value]").replacen(
        "context_types: {}",
        "context_types: {pair: Pair}",
        1,
    );
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(
        r#"{"outer":{"café 日本":"ação"},"again":"ação"}"#,
        &original,
        |input| {
            let parsed = factory.collect(input, &["model.native"], limits())?;
            let document = parsed.assemble()?;
            let entities = document.entities()?;
            let outer = entities
                .iter()
                .position(|e| e.identity().label() == "Pair" && e.name().display() == "outer")
                .ok_or("outer entity")?;
            let inner = entities
                .iter()
                .position(|e| e.identity().label() == "Pair" && e.name().display() == "café 日本")
                .ok_or("nested pair")?;
            assert_eq!(entities[inner].owners().last(), Some(&outer));
            assert_eq!(entities[inner].identity().context().len(), 1);
            assert_eq!(entities[inner].value().unwrap().text(), "\"ação\"");
            let values: Vec<_> = entities
                .iter()
                .filter(|e| e.identity().label() == "Value" && e.name().display() == "ação")
                .collect();
            assert_eq!(values.len(), 2);
            assert_ne!(
                values[0].identity().uid().as_str(),
                values[1].identity().uid().as_str()
            );
            assert!(
                values
                    .iter()
                    .all(|e| e.extent().unwrap().range().text() == "\"ação\"")
            );
            assert!(entities.iter().all(|e| e.conflicts().is_empty()));
            Ok(())
        },
    )
}

async fn raw(
    temp: &TempDir,
    text: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join("raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["model.native"])),
            Arc::new(StringArray::from(vec![text])),
        ],
    )?;
    LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
    let target = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(Some("fixture"))
        .await?
        .materialize()
        .await?;
    Ok(read_branch_source(&target, "model.native", original).await?)
}

#[tokio::test]
async fn generic_graph_owns_real_raw_association_utf8_ranges_and_contains() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{AstGraphGenericInput, AstGraphValue, assemble_graph_generic},
        source_inspection::IndexedSource,
    };
    let original = Arc::new(context());
    let files = TempDir::new()?;
    let raw = raw(&files, r#"{"café 日本":"ação"}"#, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        return Err("stored fixture source".into());
    };
    let draft = catalog(config("").as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    let mut owned = None;
    with_source(text, &original, |input| {
        let parsed = factory.collect(input, &["model.native"], limits())?;
        let document = parsed.assemble()?;
        let batch = assemble_graph_generic(
            AstGraphGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(batch.nodes().len(), 2);
        assert_eq!(batch.edges().len(), 1);
        assert_eq!(batch.edges()[0].kind(), "CONTAINS");
        assert_eq!(batch.association().raw_version(), raw.version());
        assert_eq!(batch.association().path(), "model.native");
        let pair = batch
            .nodes()
            .iter()
            .find(|node| node.label() == "Pair")
            .ok_or("pair graph")?;
        assert_eq!(
            pair.properties().get("name"),
            Some(&AstGraphValue::String("café 日本".into()))
        );
        assert_eq!(
            pair.properties().get("raw_name"),
            Some(&AstGraphValue::String("\"café 日本\"".into()))
        );
        let AstGraphValue::Int64(start) =
            pair.properties().get("start_byte").ok_or("start byte")?
        else {
            return Err("integer start".into());
        };
        let AstGraphValue::Int64(end) = pair.properties().get("end_byte").ok_or("end byte")? else {
            return Err("integer end".into());
        };
        assert_eq!(
            &text[usize::try_from(*start)?..usize::try_from(*end)?],
            "\"café 日本\":\"ação\""
        );
        owned = Some(batch);
        Ok(())
    })?;
    let batch = owned.ok_or("owned graph batch")?;
    assert_eq!(batch.nodes().len(), 2);
    assert_eq!(batch.edges()[0].source_key(), batch.nodes()[0].key());
    Ok(())
}

#[test]
fn generic_embedded_transforms_preserve_original_atoms_wrappers_and_invalid_references()
-> TestResult {
    use graphit_ast::{
        prepared_input::SyntheticReason,
        tree_sitter_generic_model::{GenericEmbeddedIssue, GenericEmbeddedLanguage},
    };
    let original = context();
    let yaml = br#"language: native-xml-fixture
grammar: tree-sitter-xml
context_types: {}
anon_func_types: []
queries:
  - data_key: elements
    graph_label: Element
    pattern: '(element (STag (Name) @name)) @unit'
    span_capture: unit
embedded:
  - pattern: '(element (STag (Name) @_tag) (content (CDSect (CData) @body)) (#eq? @_tag "code"))'
    text_capture: body
    default: javascript
    normalize: xml-code
    wrap_prefix: 'function wrapper(){'
    wrap_suffix: '}'
text_normalizers:
  xml-code:
    replace: {'&lt;': '<', '&le;': '<='}
    numeric_char_refs: true
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-xml")?, &original)?,
        &original,
    )?;
    let source = "<code><![CDATA[ a &lt; b &le; c &#x03B1; &#xD800; &#10; ]]></code>";
    with_source(source, &original, |input| {
        let parsed = factory.collect(input, &["model.xml"], limits())?;
        drop(factory);
        assert_eq!(parsed.assemble()?.entities()?.len(), 1);
        let blocks = parsed.embedded_blocks()?;
        assert_eq!(blocks.len(), 1);
        let raw_observations = parsed.embedded_observations()?;
        let raw_body = &raw_observations[0].matched.captures[blocks[0].body_slot];
        assert_eq!(blocks[0].body.node_id, raw_body.node_id);
        assert_eq!(blocks[0].body.prepared.text(), raw_body.text.as_str());
        assert_eq!(
            blocks[0].body.prepared.text().as_ptr(),
            source[raw_body.range.start..].as_ptr()
        );
        assert_eq!(
            blocks[0].language,
            GenericEmbeddedLanguage::Selected("javascript".into())
        );
        let prepared = parsed
            .prepare_embedded(0)?
            .map_err(|issue| format!("embedded preparation: {issue:?}"))?;
        assert_eq!(
            prepared.prepared_text(),
            "function wrapper(){ a < b <= c α &#xD800; &#10; }"
        );
        assert_eq!(
            prepared
                .diagnostics
                .iter()
                .filter(|issue| matches!(
                    issue,
                    GenericEmbeddedIssue::InvalidNumericReference { .. }
                ))
                .count(),
            2
        );
        prepared.with_input(|child| {
            let text = child.prepared_text();
            let less = text.find('<').unwrap() as u64;
            let OriginalProjection::Exact(range) = child.project_prepared(less, less + 1)? else {
                return Err(GenericModelError::NativeContract("replacement must retain its unique original interval"));
            };
            assert_eq!(range.text(), "&lt;");
            assert!(child.origin_plan().segments.iter().any(|segment| matches!(segment,
                OriginSegment::MappedAtom {prepared,..} if prepared.start == less && prepared.end == less+1)));
            assert!(matches!(child.project_prepared(0, 8)?,OriginalProjection::Synthetic(SyntheticReason::Wrapper)));
            let pair = text.find("<=").unwrap() as u64;
            assert!(matches!(child.project_prepared(pair,pair+1)?,OriginalProjection::Unmappable(_)));
            let alpha = text.find('α').unwrap() as u64;
            let OriginalProjection::Exact(range) = child.project_prepared(alpha, alpha + 2)? else {
                return Err(GenericModelError::NativeContract("numeric reference must retain its unique original interval"));
            };
            assert_eq!(range.text(), "&#x03B1;");
            assert!(child.origin_plan().segments.iter().any(|segment| matches!(segment,
                OriginSegment::MappedAtom {prepared,..} if prepared.start == alpha && prepared.end == alpha+2)));
            assert!(child.project_prepared(alpha, alpha + 1).is_err());
            Ok(())
        })?;
        Ok(())
    })
}

#[test]
fn generic_embedded_first_claim_keeps_unmapped_explicit_language_without_default_recapture()
-> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEmbeddedLanguage;
    let original = context();
    let yaml = br#"language: native-html-fixture
grammar: tree-sitter-html
context_types: {}
anon_func_types: []
queries:
  - data_key: elements
    graph_label: Element
    pattern: '(script_element (start_tag (tag_name) @name)) @unit'
    span_capture: unit
embedded:
  - pattern: '(script_element (start_tag (attribute (attribute_name) @_a (quoted_attribute_value (attribute_value) @lang))) (raw_text) @body (#eq? @_a "type"))'
    text_capture: body
    lang_capture: lang
    languages: {}
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: javascript
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-html")?, &original)?,
        &original,
    )?;
    with_source(
        r#"<script type="custom">call()</script>"#,
        &original,
        |input| {
            let parsed = factory.collect(input, &["model.html"], limits())?;
            let blocks = parsed.embedded_blocks()?;
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].embedded_index, 0);
            assert_eq!(
                blocks[0].language,
                GenericEmbeddedLanguage::Unmapped("custom".into())
            );
            assert!(parsed.prepare_embedded(0)?.is_err());
            Ok(())
        },
    )
}

#[test]
fn generic_stock_samples_prove_explicit_full_units_without_replacing_specialized_models()
-> TestResult {
    let samples: Vec<serde_json::Value> =
        serde_json::from_slice(include_bytes!("fixtures/tree_sitter_builtin/samples.json"))?;
    let cases = [
        (
            "java",
            "Function",
            "greet",
            "public int greet() { return value; }",
        ),
        ("c", "Function", "greet", "int greet(void) { return 1; }"),
        ("cpp", "Function", "greet", "int greet() { return 1; }"),
        ("dart", "Function", "greet", "int greet() { return value; }"),
        ("ruby", "Function", "greet", "def greet\n    1\n  end"),
        (
            "kotlin",
            "Function",
            "greet",
            "fun greet(): Int {\n        return value\n    }",
        ),
        (
            "swift",
            "Function",
            "greet",
            "func greet() -> Int { return value }",
        ),
        ("scala", "Function", "greet", "def greet(): Int = value"),
        ("julia", "Function", "greet", "greet(x) = x + 1"),
        ("clojure", "Function", "greet", "(defn greet [x] (+ x 1))"),
        ("elixir", "Function", "greet", "def greet(x), do: x + 1"),
        (
            "hcl",
            "Resource",
            "demo",
            "resource \"example_item\" \"demo\" {\n  value = \"hello\"\n}",
        ),
        ("toml", "Pair", "value", "value = \"Olá\""),
        ("bash", "Function", "greet", "greet() { echo \"Olá\"; }"),
        (
            "csharp",
            "Function",
            "Greet",
            "public int Greet() { return 1; }",
        ),
        ("css", "CssClass", "item", ".item"),
        ("dockerfile", "Stage", "alpine", "FROM alpine:3.20"),
        ("graphql", "ObjectType", "Item", "type Item { value: Int! }"),
        (
            "groovy",
            "Class",
            "Item",
            "class Item { int value = 1; int greet() { return value } }",
        ),
        ("haskell", "Function", "greet", "greet x = x + 1"),
        ("html", "Element", "div", "<div class=\"item\">Olá</div>"),
        (
            "lua",
            "Function",
            "outer",
            "local function outer(x)\n    local f = function(y) return inside(x + y) end\n    return f(1)\nend",
        ),
        (
            "objc",
            "Function",
            "outer",
            "int outer(int x) {\n    int (^f)(int) = ^(int y) { return inside(x + y); };\n    return f(1);\n}",
        ),
        (
            "php",
            "Method",
            "greet",
            "public function greet() { return 1; }",
        ),
        ("plpgsql", "Variable", "value", "value integer := 1;"),
        (
            "protobuf",
            "Message",
            "Item",
            "message Item { int32 value = 1; }",
        ),
        ("r", "Function", "greet", "greet <- function(x) { x + 1 }"),
        ("sql", "Table", "item", "CREATE TABLE item (value INT)"),
        ("svelte", "Element", "div", "<div>{value}</div>"),
        (
            "vue",
            "Element",
            "template",
            "<template><div>{{ value }}</div></template>",
        ),
        ("xml", "Element", "item", "<item><value>Olá</value></item>"),
        ("yaml", "Mapping", "value", "value: Olá"),
        ("zig", "Function", "greet", "fn greet() i32 { return 1; }"),
    ];
    for (language, label, name, expected) in cases {
        let original = context();
        let sample = samples
            .iter()
            .find(|sample| sample["profile"] == language)
            .ok_or("official stock sample")?;
        let yaml = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/graphit-ast/profiles/queries")
                .join(format!("{language}.yaml")),
        )?;
        let draft = catalog(&yaml, &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(
                profile,
                builtin(profile.grammar().value())?,
                &original,
            )?,
            &original,
        )?;
        with_source(
            sample["source"].as_str().ok_or("sample source")?,
            &original,
            |input| {
                let parsed = factory.collect(input, &["stock.fixture"], limits())?;
                let document = parsed.assemble()?;
                let entity = document
                    .entities()?
                    .iter()
                    .find(|entity| {
                        entity.identity().label() == label && entity.name().display() == name
                    })
                    .ok_or_else(|| {
                        format!(
                            "{language} {label}/{name} missing; diagnostics {:?}",
                            document.diagnostics().ok()
                        )
                    })?;
                assert_eq!(
                    entity
                        .extent()
                        .ok_or_else(|| format!("{language} explicit full extent unavailable"))?
                        .range()
                        .text(),
                    expected,
                    "{language}"
                );
                assert!(
                    entity.conflicts().is_empty(),
                    "{language}: {:?}",
                    entity.conflicts()
                );
                if label == "Function" {
                    assert!(
                        entity.contributions().iter().any(|contribution| matches!(
                            contribution.declaration,
                            GenericDeclaration::Native { .. }
                        )),
                        "{language} actual declaration"
                    );
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}

#[test]
fn native_recovery_is_observed_even_without_entity_matches() -> TestResult {
    let original = context();
    let draft = catalog(config("").as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?;
    let prepared = PreparedGenericTreeSitterModel::prepare(profile, contexts, &original)?;
    with_source("?", &original, |input| {
        let parsed = prepared.collect(input, &["broken.native"], limits())?;
        let document = parsed.assemble()?;
        assert!(document.entities()?.is_empty());
        let recoveries = document.syntax_recoveries()?;
        assert!(!recoveries.is_empty());
        for recovery in recoveries {
            let node = &document.observations()?.nodes[recovery.node_index];
            assert_eq!(recovery.node_id, node.node_id);
            assert_eq!(recovery.kind, node.kind);
            assert_eq!(recovery.prepared, node.range);
            assert!(recovery.error || recovery.missing);
            assert!(
                matches!(&recovery.original, OriginalProjection::Exact(range) if range.text() == "?")
            );
        }
        Ok(())
    })
}

#[test]
fn conflicting_declaration_never_reuses_valid_extent_for_header_or_complexity() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::{GenericEntityIssue, GenericExport};
    let original = context();
    let yaml = config("declaration_types: [pair, string]\nexports: {strategy: modifier, config: {keyword: public}}")
        .replace("(string) @name", "(string) @name @decl")
        .replace(") @unit'", ") @unit @decl'")
        .replace("    span_capture: unit", "    span_capture: unit\n    declaration_capture: decl");
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"name":42}"#, &original, |input| {
        let parsed = factory.collect(input, &["conflict.native"], limits())?;
        let document = parsed.assemble()?;
        let index = document
            .entities()?
            .iter()
            .position(|e| e.identity().label() == "Pair")
            .ok_or("pair")?;
        let entity = &document.entities()?[index];
        assert!(entity.extent().is_some());
        assert!(
            entity
                .conflicts()
                .contains(&GenericEntityIssue::ConflictingDeclaration)
        );
        assert!(matches!(
            document.exports()?[index],
            GenericExport::DeclarationUnavailable { .. }
        ));
        assert!(matches!(
            document.complexities()?[index],
            GenericComplexity::DeclarationUnavailable { .. }
        ));
        Ok(())
    })
}

#[test]
fn multiple_names_and_values_bind_only_to_their_own_native_units() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    let original = context();
    let yaml = config("").replace("(pair key: (string) @name value: (_) @value) @unit",
        "(object (pair key: (string) @name value: (_) @value) @unit (pair key: (string) @name value: (_) @value) @unit)");
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"first":1,"second":2}"#, &original, |input| {
        let parsed = factory.collect(input, &["multiple.native"], limits())?;
        let document = parsed.assemble()?;
        let pairs: Vec<_> = document
            .entities()?
            .iter()
            .filter(|e| e.identity().label() == "Pair")
            .collect();
        assert_eq!(pairs.len(), 2);
        for (name, value) in [("first", "1"), ("second", "2")] {
            let pair = pairs
                .iter()
                .find(|p| p.name().display() == name)
                .ok_or("native name")?;
            assert_eq!(pair.value().ok_or("scalar native value")?.text(), value);
            let index = document
                .entities()?
                .iter()
                .position(|e| std::ptr::eq(e, *pair))
                .ok_or("owner")?;
            let child = document
                .entities()?
                .iter()
                .find(|e| e.identity().label() == "Value" && e.name().display() == value)
                .ok_or("value child")?;
            assert_eq!(child.owners(), &[index]);
        }
        assert!(
            !document
                .diagnostics()?
                .iter()
                .any(|d| d.issue == GenericEntityIssue::AmbiguousValue)
        );
        Ok(())
    })
}

#[test]
fn distinct_data_keys_preserve_same_uid_endpoints_without_arbitrary_contains() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    let original = context();
    let base = config("");
    let second = base
        .split_once("queries:\n")
        .ok_or("query fixture")?
        .1
        .replace("data_key: pairs", "data_key: alternate");
    let yaml = format!("{base}{second}");
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"same":1}"#, &original, |input| {
        let parsed = factory.collect(input, &["keys.native"], limits())?;
        let document = parsed.assemble()?;
        let pairs: Vec<_> = document
            .entities()?
            .iter()
            .filter(|e| e.identity().label() == "Pair")
            .collect();
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            pairs[0].identity().uid().as_str(),
            pairs[1].identity().uid().as_str()
        );
        assert_ne!(pairs[0].data_key(), pairs[1].data_key());
        let values: Vec<_> = document
            .entities()?
            .iter()
            .filter(|e| e.identity().label() == "Value")
            .collect();
        assert_eq!(values.len(), 2);
        assert!(values.iter().all(|e| e.owners().is_empty()
            && e.conflicts().contains(&GenericEntityIssue::AmbiguousParent)));
        Ok(())
    })
}

#[test]
fn data_views_keep_literal_escapes_and_enforce_published_unicode_scalar_rule() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    for (name, searchable) in [
        (" ".to_owned(), false),
        ("".to_owned(), false),
        ("界".repeat(256), true),
        ("界".repeat(257), false),
        (r"e\u0301".to_owned(), true),
        ("é".to_owned(), true),
    ] {
        let original = context();
        let draft = catalog(config("").as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        let encoded = serde_json::to_string(&name)?;
        let source = format!("{{{encoded}:1}}");
        with_source(&source, &original, |input| {
            let parsed = factory.collect(input, &["names.native"], limits())?;
            let document = parsed.assemble()?;
            let pair = document
                .entities()?
                .iter()
                .find(|e| e.identity().label() == "Pair");
            assert_eq!(
                pair.is_some(),
                searchable,
                "name scalar count {}",
                name.chars().count()
            );
            if let Some(pair) = pair {
                assert_eq!(pair.name().raw().text(), encoded);
                assert_eq!(pair.name().display(), &encoded[1..encoded.len() - 1]);
            } else {
                assert!(
                    document
                        .diagnostics()?
                        .iter()
                        .any(|d| matches!(d.issue, GenericEntityIssue::Name(_)))
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn embedded_graph_merge_keeps_native_relation_addresses_and_refreshes_file_metadata()
-> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{
            AstGraphEmbeddedComposition, AstGraphGenericInput, AstGraphValue,
            assemble_graph_generic, merge_graph_embedded,
        },
        source_inspection::IndexedSource,
    };
    let original = Arc::new(context());
    let files = TempDir::new()?;
    let raw = raw(
        &files,
        r#"<script>{"target":0,"caller":"target"}</script>"#,
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        return Err("stored source".into());
    };
    let host_yaml = br#"language: native-html
grammar: tree-sitter-html
context_types: {}
queries:
  - data_key: scripts
    graph_label: Script
    pattern: '(script_element (start_tag (tag_name) @name)) @unit'
    span_capture: unit
embedded:
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: native-json
"#;
    let child_yaml = br#"language: native-json
grammar: tree-sitter-json
context_types: {}
queries:
  - data_key: names
    graph_label: Pair
    pattern: '(pair key: (string (string_content) @name)) @unit'
    span_capture: unit
  - data_key: refs
    type: relation
    relation_type: REFERENCES
    target_labels: [Pair]
    pattern: '(pair value: (string (string_content) @name))'
"#;
    let hd = catalog(host_yaml, &original)?;
    let cd = catalog(child_yaml, &original)?;
    let hp = &hd.profiles()[0];
    let cp = &cd.profiles()[0];
    let hf = PreparedGenericTreeSitterModel::prepare(
        hp,
        PreparedTreeSitterContexts::prepare(hp, builtin("tree-sitter-html")?, &original)?,
        &original,
    )?;
    let cf = PreparedGenericTreeSitterModel::prepare(
        cp,
        PreparedTreeSitterContexts::prepare(cp, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(text, &original, |input| {
        let host = hf.collect(input, &["model.native"], limits())?;
        let host_doc = host.assemble()?;
        let batch = assemble_graph_generic(
            AstGraphGenericInput {
                document: &host_doc,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(batch.nodes().len(), 1);
        let mut composition = AstGraphEmbeddedComposition::new(batch, &original)?;
        let embedded = host
            .prepare_embedded(0)?
            .map_err(|e| format!("embedded {e:?}"))?;
        embedded.with_input(|input| -> TestResult {
            let child = cf.collect(input, &["model.native"], limits())?;
            let document = child.assemble()?;
            let child_batch = assemble_graph_generic(
                AstGraphGenericInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let addresses: Vec<_> = child_batch
                .outcomes()
                .iter()
                .map(|o| {
                    (
                        o.relation_index(),
                        o.source_index(),
                        o.capture_index(),
                        o.evidence_node_indices().to_vec(),
                    )
                })
                .collect();
            assert!(!addresses.is_empty());
            assert!(child_batch.edges().iter().any(|e| e.kind() == "REFERENCES"));
            merge_graph_embedded(
                &mut composition,
                child_batch,
                serde_json::json!({"embedded_index":0}),
                &original,
            )?;
            let batch = composition.finish(&original)?;
            assert_eq!(
                batch
                    .outcomes()
                    .iter()
                    .map(|o| (
                        o.relation_index(),
                        o.source_index(),
                        o.capture_index(),
                        o.evidence_node_indices().to_vec()
                    ))
                    .collect::<Vec<_>>(),
                addresses
            );
            let AstGraphValue::String(outcomes) = batch
                .file_observation()
                .properties()
                .get("resolution_outcomes_json")
                .ok_or("file outcomes")?
            else {
                return Err("outcome json string".into());
            };
            let persisted: serde_json::Value = serde_json::from_str(outcomes)?;
            assert_eq!(
                persisted.as_array().ok_or("outcome array")?.len(),
                addresses.len()
            );
            assert_eq!(persisted[0]["observation"]["embedded"]["embedded_index"], 0);
            Ok(())
        })?;
        Ok(())
    })
}

#[test]
fn context_label_disambiguates_multiple_native_labels_without_relabeling_single_candidates()
-> TestResult {
    let original = context();
    let yaml = br#"language: native-contexts
grammar: tree-sitter-json
context_types: {pair: Method}
context_name_queries: {pair: '(pair key: (_) @name) @context'}
queries:
  - data_key: functions
    graph_label: Function
    name_is_data: true
    pattern: '(pair key: (string) @name) @unit'
    span_capture: unit
  - data_key: methods
    graph_label: Method
    name_is_data: true
    pattern: '(pair key: (string) @name) @unit'
    span_capture: unit
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"outer":{"inner":1}}"#, &original, |input| {
        let parsed = factory.collect(input, &["contexts.native"], limits())?;
        let document = parsed.assemble()?;
        let parent = document
            .entities()?
            .iter()
            .position(|e| e.identity().label() == "Method" && e.name().display() == "outer")
            .ok_or("configured parent label")?;
        let inners: Vec<_> = document
            .entities()?
            .iter()
            .filter(|e| e.name().display() == "inner")
            .collect();
        assert_eq!(inners.len(), 2);
        assert!(inners.iter().all(|e| e.owners() == [parent]));
        Ok(())
    })?;
    let single = std::str::from_utf8(yaml)?
        .split("  - data_key: methods")
        .next()
        .ok_or("single query")?
        .replace("graph_label: Function", "graph_label: Variable");
    let draft = catalog(single.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"outer":{"inner":1}}"#, &original, |input| {
        let parsed = factory.collect(input, &["single.native"], limits())?;
        let document = parsed.assemble()?;
        let parent = document
            .entities()?
            .iter()
            .position(|e| e.name().display() == "outer")
            .ok_or("native Variable parent")?;
        assert_eq!(document.entities()?[parent].identity().label(), "Variable");
        let inner = document
            .entities()?
            .iter()
            .find(|e| e.name().display() == "inner")
            .ok_or("inner")?;
        assert_eq!(inner.owners(), [parent]);
        Ok(())
    })
}

#[test]
fn generic_document_checks_its_retained_original_after_collection() -> TestResult {
    use std::time::{Duration, Instant};
    for deadline in [false, true] {
        let original = if deadline {
            SnapshotContext::new(
                Instant::now() + Duration::from_secs(1),
                Cancellation::default(),
            )
        } else {
            context()
        };
        let draft = catalog(config("").as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(r#"{"name":1}"#, &original, |input| {
            let parsed = factory.collect(input, &["cancel.native"], limits())?;
            let document = parsed.assemble()?;
            let kind = if deadline {
                let remaining = original
                    .deadline()
                    .ok_or("finite fixture deadline")?
                    .saturating_duration_since(Instant::now());
                std::thread::sleep(remaining + Duration::from_millis(2));
                std::io::ErrorKind::TimedOut
            } else {
                original.cancellation().cancel();
                std::io::ErrorKind::Interrupted
            };
            assert!(
                matches!(document.entities(), Err(GenericModelError::Operation(ref e)) if e.kind()==kind)
            );
            assert!(
                matches!(document.syntax_recoveries(), Err(GenericModelError::Operation(ref e)) if e.kind()==kind)
            );
            assert!(
                matches!(document.observations(), Err(GenericModelError::Operation(ref e)) if e.kind()==kind)
            );
            assert!(parsed.node_origin(0).is_err());
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn six_name_and_native_header_export_strategies_ignore_body_literals_and_comments() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericExport;
    let source = r#"class Fixture {
        public int visible() { String text="private static public"; return 1; }
        private int _hidden() { /* public */ return 2; }
        static int local() { return 3; }
        public int Ωmega() { return 4; }
    }"#;
    for (policy, expected) in [
        ("strategy: none", [false, false, false, false]),
        ("strategy: capitalized_name", [false, false, false, true]),
        (
            "strategy: no_prefix\n  config: {prefix: _}",
            [true, false, true, true],
        ),
        (
            "strategy: modifier\n  config: {keyword: public}",
            [true, false, false, true],
        ),
        (
            "strategy: no_modifier\n  config_list: {keywords: [private, protected]}",
            [true, false, true, true],
        ),
        ("strategy: no_static", [true, true, false, true]),
    ] {
        let original = context();
        let yaml = format!(
            "language: native-java\ngrammar: tree-sitter-java\ncontext_types: {{}}\ndeclaration_types: [method_declaration]\ncomment_types: [line_comment, block_comment]\nexports:\n  {policy}\nqueries:\n  - data_key: functions\n    graph_label: Function\n    pattern: '(method_declaration name: (identifier) @name) @unit'\n    span_capture: unit\n    declaration_capture: unit\n"
        );
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-java")?, &original)?,
            &original,
        )?;
        with_source(source, &original, |input| {
            let parsed = factory.collect(input, &["exports.java"], limits())?;
            let document = parsed.assemble()?;
            for (name, expected) in ["visible", "_hidden", "local", "Ωmega"]
                .into_iter()
                .zip(expected)
            {
                let index = document
                    .entities()?
                    .iter()
                    .position(|e| e.name().display() == name)
                    .ok_or_else(|| format!("{policy}: missing {name}"))?;
                let GenericExport::Decision { exported, .. } = document.exports()?[index] else {
                    return Err(format!("{policy}: unavailable native header").into());
                };
                assert_eq!(exported, expected, "{policy}: {name}");
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn julia_export_statement_resolves_lexically_and_keeps_unexported_functions_private() -> TestResult
{
    use graphit_ast::tree_sitter_generic_model::GenericExport;
    let original = context();
    let yaml = br#"language: native-julia
grammar: tree-sitter-julia
context_types: {}
declaration_types: [assignment]
exports: {strategy: export_statement}
queries:
  - data_key: functions
    graph_label: Function
    pattern: '(assignment (call_expression (identifier) @name)) @unit'
    span_capture: unit
    declaration_capture: unit
  - data_key: exports
    graph_label: Export
    pattern: '(export_statement (identifier) @name) @unit'
    span_capture: unit
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-julia")?, &original)?,
        &original,
    )?;
    with_source(
        "export visible\nvisible(x) = x + 1\nhidden(x) = x - 1\n",
        &original,
        |input| {
            let parsed = factory.collect(input, &["exports.jl"], limits())?;
            let document = parsed.assemble()?;
            for (name, expected) in [("visible", true), ("hidden", false)] {
                let index = document
                    .entities()?
                    .iter()
                    .position(|e| e.identity().label() == "Function" && e.name().display() == name)
                    .ok_or("function")?;
                let GenericExport::Decision { exported, .. } = document.exports()?[index] else {
                    return Err("export ambiguous".into());
                };
                assert_eq!(exported, expected, "{name}");
            }
            Ok(())
        },
    )
}

#[tokio::test]
async fn embedded_specialized_go_keeps_source_ordinals_and_real_edges_after_merge() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{
            AstGraphEmbeddedComposition, AstGraphGenericInput, AstGraphTsInput, AstGraphValue,
            assemble_graph_generic, assemble_graph_ts, merge_graph_embedded,
        },
        source_inspection::IndexedSource,
    };
    let original = Arc::new(context());
    let files = TempDir::new()?;
    let raw = raw(
        &files,
        "<script>package sample\nfunc target() {}\nfunc caller() { target() }\n</script>",
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        return Err("stored source".into());
    };
    let host_yaml = br#"language: native-html
grammar: tree-sitter-html
context_types: {}
queries:
  - data_key: scripts
    graph_label: Script
    pattern: '(script_element (start_tag (tag_name) @name)) @unit'
    span_capture: unit
embedded:
  - pattern: '(script_element (raw_text) @body)'
    text_capture: body
    default: native-json
"#;
    let child_yaml = include_bytes!("fixtures/catalog/shipped/go.yaml");
    let hd = catalog(host_yaml, &original)?;
    let cd = catalog(child_yaml, &original)?;
    let hp = &hd.profiles()[0];
    let cp = &cd.profiles()[0];
    let hf = PreparedGenericTreeSitterModel::prepare(
        hp,
        PreparedTreeSitterContexts::prepare(hp, builtin("tree-sitter-html")?, &original)?,
        &original,
    )?;
    let cf = PreparedTreeSitterContexts::prepare(cp, builtin("tree-sitter-go")?, &original)?;
    with_source(text, &original, |input| {
        let host = hf.collect(input, &["model.native"], limits())?;
        let host_doc = host.assemble()?;
        let batch = assemble_graph_generic(
            AstGraphGenericInput {
                document: &host_doc,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(batch.nodes().len(), 1);
        let mut composition = AstGraphEmbeddedComposition::new(batch, &original)?;
        let embedded = host
            .prepare_embedded(0)?
            .map_err(|e| format!("embedded {e:?}"))?;
        embedded.with_input(|input| -> TestResult {
            let child = graphit_ast::tree_sitter_model::ParsedTreeSitterDocument::collect(
                &cf,
                input,
                &["model.native"],
                limits(),
            )?;
            let document = child.assemble()?;
            let child_batch = assemble_graph_ts(
                AstGraphTsInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let addresses: Vec<_> = child_batch
                .outcomes()
                .iter()
                .map(|o| {
                    (
                        o.relation_index(),
                        o.source_index(),
                        o.capture_index(),
                        o.evidence_node_indices().to_vec(),
                    )
                })
                .collect();
            assert!(!addresses.is_empty());
            assert!(
                child_batch.edges().iter().any(|e| e.kind() == "CALLS"),
                "native Go child nodes={:?}; outcomes={:?}",
                child_batch.nodes(),
                child_batch.outcomes()
            );
            merge_graph_embedded(
                &mut composition,
                child_batch,
                serde_json::json!({"embedded_index":0}),
                &original,
            )?;
            let batch = composition.finish(&original)?;
            assert_eq!(
                batch
                    .outcomes()
                    .iter()
                    .map(|o| (
                        o.relation_index(),
                        o.source_index(),
                        o.capture_index(),
                        o.evidence_node_indices().to_vec()
                    ))
                    .collect::<Vec<_>>(),
                addresses
            );
            let AstGraphValue::String(outcomes) = batch
                .file_observation()
                .properties()
                .get("resolution_outcomes_json")
                .ok_or("file outcomes")?
            else {
                return Err("outcome json string".into());
            };
            let persisted: serde_json::Value = serde_json::from_str(outcomes)?;
            assert_eq!(
                persisted.as_array().ok_or("outcome array")?.len(),
                addresses.len()
            );
            assert_eq!(persisted[0]["observation"]["embedded"]["embedded_index"], 0);
            Ok(())
        })?;
        Ok(())
    })
}

#[test]
fn required_context_kind_must_be_declared_before_any_document() -> TestResult {
    let original = context();
    let draft = catalog(
        config("context_requires_entity: [absent]").as_bytes(),
        &original,
    )?;
    let profile = &draft.profiles()[0];
    let contexts =
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?;
    assert!(matches!(
        PreparedGenericTreeSitterModel::prepare(profile, contexts, &original),
        Err(GenericModelError::InvalidPolicy {
            field: "context_requires_entity",
            ..
        })
    ));
    Ok(())
}

#[test]
fn required_polymorphic_contexts_keep_real_entities_without_fabricating_namespaces() -> TestResult {
    for (language, source, actual_parent, actual_child) in [
        (
            "r",
            "outer <- function(x) { inside <- 1 }\nresult <- a + (nested <- 2)\n",
            "outer",
            "inside",
        ),
        (
            "elixir",
            "defmodule Alias do\n real = 1\nend\nwrap(Alias) do\n false_scope = 2\nend\n",
            "Alias",
            "real",
        ),
    ] {
        let original = context();
        let yaml = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/graphit-ast/profiles/queries")
                .join(format!("{language}.yaml")),
        )?;
        let draft = catalog(&yaml, &original)?;
        let profile = &draft.profiles()[0];
        assert!(profile.context_types().value().is_empty());
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(
                profile,
                builtin(profile.grammar().value())?,
                &original,
            )?,
            &original,
        )?;
        with_source(source, &original, |input| {
            let parsed = factory.collect(input, &["contexts.fixture"], limits())?;
            let document = parsed.assemble()?;
            assert!(
                document.syntax_recoveries()?.is_empty(),
                "{language} valid syntax"
            );
            assert!(
                document.namespaces()?.is_empty(),
                "{language} must not invent a namespace for a polymorphic expression"
            );
            let parent = document
                .entities()?
                .iter()
                .position(|e| {
                    e.name().display() == actual_parent
                        && e.identity().label()
                            == if language == "r" {
                                "Function"
                            } else {
                                "Module"
                            }
                })
                .ok_or("actual declaration")?;
            let child = document
                .entities()?
                .iter()
                .find(|e| e.name().display() == actual_child)
                .ok_or_else(||format!("{language} actual child {actual_child} missing; entities {:?}; diagnostics {:?}",document.entities().map(|es|es.iter().map(|e|(e.identity().label(),e.name().display())).collect::<Vec<_>>()),document.diagnostics()))?;
            assert!(
                child.owners().contains(&parent),
                "{language} declaration owner"
            );
            assert!(
                document.diagnostics()?.is_empty(),
                "{language}: {:?}",
                document.diagnostics()?
            );
            for name in if language == "r" {
                vec!["result", "nested"]
            } else {
                vec!["false_scope"]
            } {
                let ordinary = document
                    .entities()?
                    .iter()
                    .find(|e| e.name().display() == name)
                    .ok_or("ordinary expression child")?;
                assert!(
                    ordinary.owners().is_empty(),
                    "ordinary expression cannot own {name}"
                );
                assert!(ordinary.identity().context().is_empty());
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn configured_namespace_and_class_without_entity_queries_keep_native_identity() -> TestResult {
    let original = context();
    let yaml = br#"language: native-csharp
grammar: tree-sitter-c_sharp
context_types: {namespace_declaration: Namespace, class_declaration: Class}
context_name_queries: {namespace_declaration: '(namespace_declaration name: (_) @name) @context', class_declaration: '(class_declaration name: (_) @name) @context'}
queries:
  - graph_label: Function
    data_key: functions
    pattern: '(method_declaration name: (identifier) @name) @unit'
    span_capture: unit
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-c_sharp")?, &original)?,
        &original,
    )?;
    with_source(
        "namespace N { class C { int F() { return 1; } } }",
        &original,
        |input| {
            let parsed = factory.collect(input, &["namespace.cs"], limits())?;
            let document = parsed.assemble()?;
            assert!(document.diagnostics()?.is_empty());
            assert_eq!(document.entities()?.len(), 1);
            let namespaces = document.namespaces()?;
            let mut names = namespaces
                .iter()
                .map(|n| (n.label(), n.subject().text()))
                .collect::<Vec<_>>();
            names.sort_unstable();
            assert_eq!(names, [("Class", "C"), ("Namespace", "N")]);
            let function = &document.entities()?[0];
            assert_eq!(function.identity().context().len(), 2);
            assert_eq!(
                function
                    .identity()
                    .context()
                    .iter()
                    .map(|f| f.label())
                    .collect::<Vec<_>>(),
                ["Namespace", "Class"]
            );
            Ok(())
        },
    )
}

#[test]
fn lua_named_vararg_keeps_missing_full_unit_instead_of_borrowing_parameters() -> TestResult {
    let original = context();
    let yaml = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/graphit-ast/profiles/queries/lua.yaml"),
    )?;
    let draft = catalog(&yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-lua")?, &original)?,
        &original,
    )?;
    with_source(
        "function f(a, b, ...rest) return a end",
        &original,
        |input| {
            let parsed = factory.collect(input, &["parameters.lua"], limits())?;
            let document = parsed.assemble()?;
            assert!(document.syntax_recoveries()?.is_empty());
            for name in ["a", "b"] {
                let entity = document
                    .entities()?
                    .iter()
                    .find(|e| e.identity().label() == "Parameter" && e.name().display() == name)
                    .ok_or("ordinary parameter")?;
                assert_eq!(
                    entity
                        .extent()
                        .ok_or("atomic parameter unit")?
                        .range()
                        .text(),
                    name
                );
            }
            let rest = document
                .entities()?
                .iter()
                .find(|e| e.identity().label() == "Parameter" && e.name().display() == "rest")
                .ok_or("named vararg")?;
            assert!(rest.extent().is_none());
            assert!(
                rest.contributions()
                    .iter()
                    .all(|c| matches!(c.extent, GenericExtent::Missing { configured: true }))
            );
            Ok(())
        },
    )
}

#[test]
fn omitted_declaration_contribution_does_not_conflict_with_native_proof() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    for (second_declaration, conflicting) in [("", false), ("declaration_capture: name", true)] {
        let original = context();
        let yaml = format!(
            "language: native-proof\ngrammar: tree-sitter-json\ncontext_types: {{}}\ndeclaration_types: [pair, string]\nqueries:\n  - data_key: pairs\n    graph_label: Pair\n    name_is_data: true\n    pattern: '(pair key: (string) @name) @unit'\n    span_capture: unit\n    declaration_capture: unit\n  - data_key: pairs\n    graph_label: Pair\n    name_is_data: true\n    pattern: '(pair key: (string) @name) @unit'\n    span_capture: unit\n    {second_declaration}\n"
        );
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(r#"{"name":1}"#, &original, |input| {
            let parsed = factory.collect(input, &["proof.native"], limits())?;
            let document = parsed.assemble()?;
            assert_eq!(document.entities()?.len(), 1);
            let entity = &document.entities()?[0];
            assert_eq!(entity.contributions().len(), 2);
            assert_eq!(
                entity
                    .conflicts()
                    .contains(&GenericEntityIssue::ConflictingDeclaration),
                conflicting
            );
            if !conflicting {
                assert!(matches!(
                    entity.contributions()[1].declaration,
                    GenericDeclaration::NotConfigured
                ));
                assert!(matches!(
                    document.complexities()?[0],
                    GenericComplexity::Score { value: 1, .. }
                ));
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn generic_calls_use_native_namespaces_self_and_owner_qualifiers() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{AstGraphGenericInput, AstGraphRelationTarget, assemble_graph_generic},
        source_inspection::IndexedSource,
        tree_sitter_generic_model::GenericRelationTarget,
    };
    let original = Arc::new(context());
    let files = TempDir::new()?;
    let source = "class Left { void target() {} void caller() { target(); this.target(); Right.target(); Unknown.target(); missing(); } } class Right { static void target() {} } class target {}";
    let raw = raw(&files, source, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        return Err("stored Java source".into());
    };
    let yaml = br#"language: native-java
grammar: tree-sitter-java
context_types: {class_declaration: Class, method_declaration: Method}
declaration_types: [class_declaration, method_declaration]
self_keywords: [this]
target_rules:
  CALLS: {labels: [Method], fallback: Fallback}
queries:
  - data_key: classes
    graph_label: Class
    pattern: '(class_declaration name: (identifier) @name) @unit'
    span_capture: unit
    declaration_capture: unit
  - data_key: methods
    graph_label: Method
    pattern: '(method_declaration name: (identifier) @name) @unit'
    span_capture: unit
    declaration_capture: unit
  - data_key: fallback
    graph_label: Fallback
    pattern: '(class_declaration name: (identifier) @name (#eq? @name "target")) @unit'
    span_capture: unit
    declaration_capture: unit
  - data_key: calls
    type: relation
    relation_type: CALLS
    qualifier_capture: qualifier
    pattern: '[(method_invocation !object name: (identifier) @name) (method_invocation object: (_) @qualifier name: (identifier) @name)]'
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-java")?, &original)?,
        &original,
    )?;
    with_source(text, &original, |input| {
        let parsed = factory.collect(input, &["model.native"], limits())?;
        let document = parsed.assemble()?;
        assert!(document.syntax_recoveries()?.is_empty());
        let entities = document.entities()?;
        let left = entities
            .iter()
            .position(|e| e.name().display() == "Left")
            .ok_or("Left class")?;
        let right = entities
            .iter()
            .position(|e| e.name().display() == "Right")
            .ok_or("Right class")?;
        let target = |owner| {
            entities
                .iter()
                .position(|e| e.name().display() == "target" && e.owners() == [owner])
        };
        let lt = target(left).ok_or("Left target")?;
        let rt = target(right).ok_or("Right target")?;
        let caller = entities
            .iter()
            .position(|e| e.name().display() == "caller")
            .ok_or("native caller")?;
        assert_ne!(
            entities[lt].identity().uid().as_str(),
            entities[rt].identity().uid().as_str()
        );
        let relations = document.relations()?;
        assert_eq!(relations.len(), 5);
        let expected = [Some(lt), Some(lt), Some(rt), None, None];
        for (relation, expected) in relations.iter().zip(expected) {
            assert_eq!(relation.source_index, Some(caller));
            assert!(!relation.evidence_node_indices.is_empty());
            match expected {
                Some(entity_index) => assert_eq!(
                    relation.target,
                    GenericRelationTarget::Resolved { entity_index }
                ),
                None => assert_eq!(relation.target, GenericRelationTarget::NotFound),
            }
        }
        let batch = assemble_graph_generic(
            AstGraphGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        let calls = batch
            .edges()
            .iter()
            .filter(|e| e.kind() == "CALLS")
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 3);
        assert!(
            calls
                .iter()
                .all(|e| e.source_key() == batch.nodes()[caller].key())
        );
        assert_eq!(
            calls
                .iter()
                .filter(|e| e.target_key() == batch.nodes()[lt].key())
                .count(),
            2
        );
        assert_eq!(
            calls
                .iter()
                .filter(|e| e.target_key() == batch.nodes()[rt].key())
                .count(),
            1
        );
        assert_eq!(
            batch
                .outcomes()
                .iter()
                .filter(|o| matches!(o.target(), AstGraphRelationTarget::NotFound))
                .count(),
            2
        );
        Ok(())
    })
}

#[tokio::test]
async fn generic_relations_preserve_ambiguity_absence_and_invalid_native_sources() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{AstGraphGenericInput, AstGraphRelationTarget, assemble_graph_generic},
        source_inspection::IndexedSource,
        tree_sitter_generic_model::GenericRelationTarget,
    };
    for second_label in ["Pair", "Alias"] {
        let original = Arc::new(context());
        let files = TempDir::new()?;
        let raw = raw(
            &files,
            r#"{"one":0,"one":1,"caller":"one","absent":"missing"}"#,
            &original,
        )
        .await?;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            return Err("stored source".into());
        };
        let yaml = format!(
            "language: native-relations\ngrammar: tree-sitter-json\ncontext_types: {{}}\ntarget_rules:\n  REFERENCES: {{labels: [Pair]}}\nqueries:\n  - data_key: names\n    graph_label: Pair\n    pattern: '(pair key: (string (string_content) @name)) @unit'\n    span_capture: unit\n  - data_key: names\n    graph_label: {second_label}\n    pattern: '(pair key: (string (string_content) @name)) @unit'\n    span_capture: unit\n  - data_key: references\n    type: relation\n    relation_type: REFERENCES\n    pattern: '(pair value: (string (string_content) @name))'\n  - data_key: declaration_refs\n    type: relation\n    relation_type: DECLARATION_REF\n    target_labels: [Pair]\n    pattern: '(pair key: (string (string_content) @name))'\n"
        );
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(text, &original, |input| {
            let parsed = factory.collect(input, &["model.native"], limits())?;
            let document = parsed.assemble()?;
            let relations = document.relations()?;
            let references = relations
                .iter()
                .filter(|r| r.kind == "REFERENCES")
                .collect::<Vec<_>>();
            assert_eq!(references.len(), 2);
            if second_label == "Pair" {
                assert!(
                    document
                        .entities()?
                        .iter()
                        .all(|e| e.contributions().len() == 2)
                );
                let GenericRelationTarget::Ambiguous { entity_indices } = &references[0].target
                else {
                    return Err("duplicate physical subjects must stay ambiguous".into());
                };
                assert_eq!(entity_indices.len(), 2);
                assert_ne!(
                    document.entities()?[entity_indices[0]]
                        .identity()
                        .uid()
                        .as_str(),
                    document.entities()?[entity_indices[1]]
                        .identity()
                        .uid()
                        .as_str()
                );
                assert_eq!(references[1].target, GenericRelationTarget::NotFound);
            } else {
                assert!(references.iter().all(|r| matches!(
                    r.target,
                    GenericRelationTarget::Unresolved {
                        reason: "ambiguous native source declaration"
                    }
                )));
            }
            let declarations = relations
                .iter()
                .filter(|r| r.kind == "DECLARATION_REF")
                .collect::<Vec<_>>();
            assert_eq!(declarations.len(), 4);
            assert!(declarations.iter().all(|r| matches!(
                r.target,
                GenericRelationTarget::Unresolved {
                    reason: "capture is a declaration subject, not a reference"
                }
            )));
            let batch = assemble_graph_generic(
                AstGraphGenericInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            assert!(
                batch
                    .edges()
                    .iter()
                    .all(|e| e.kind() != "REFERENCES" && e.kind() != "DECLARATION_REF")
            );
            assert_eq!(batch.outcomes().len(), 6);
            if second_label == "Pair" {
                assert!(
                    matches!(batch.outcomes()[0].target(),AstGraphRelationTarget::Ambiguous{keys} if keys.len()==2)
                );
                assert!(matches!(
                    batch.outcomes()[1].target(),
                    AstGraphRelationTarget::NotFound
                ));
            }
            assert!(
                batch
                    .outcomes()
                    .iter()
                    .all(|o| !o.evidence_node_indices().is_empty())
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn generic_target_fallback_resolves_only_configured_real_labels() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{AstGraphGenericInput, assemble_graph_generic},
        source_inspection::IndexedSource,
        tree_sitter_generic_model::GenericRelationTarget,
    };
    for (query_fallback, expected_label) in [("", "Fallback"), ("target_fallback: Pair", "Pair")] {
        let original = Arc::new(context());
        let files = TempDir::new()?;
        let raw = raw(&files, r#"{"target":0,"caller":"target"}"#, &original).await?;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            return Err("stored source".into());
        };
        let yaml = format!(
            "language: native-fallback\ngrammar: tree-sitter-json\ncontext_types: {{}}\ntarget_rules:\n  REFERENCES: {{labels: [Absent], fallback: Fallback}}\nqueries:\n  - data_key: names\n    graph_label: Pair\n    pattern: '(pair key: (string (string_content) @name)) @unit'\n    span_capture: unit\n  - data_key: fallback\n    graph_label: Fallback\n    pattern: '(pair key: (string (string_content) @name) (#eq? @name \"target\")) @unit'\n    span_capture: unit\n  - data_key: references\n    type: relation\n    relation_type: REFERENCES\n    {query_fallback}\n    pattern: '(pair value: (string (string_content) @name))'\n"
        );
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(text, &original, |input| {
            let parsed = factory.collect(input, &["model.native"], limits())?;
            let document = parsed.assemble()?;
            assert_eq!(document.relations()?.len(), 1);
            let relation = &document.relations()?[0];
            let GenericRelationTarget::Resolved { entity_index } = relation.target else {
                return Err("configured fallback target".into());
            };
            assert_eq!(
                document.entities()?[entity_index].identity().label(),
                expected_label
            );
            assert_eq!(
                document.entities()?[entity_index].name().display(),
                "target"
            );
            let batch = assemble_graph_generic(
                AstGraphGenericInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let edges = batch
                .edges()
                .iter()
                .filter(|e| e.kind() == "REFERENCES")
                .collect::<Vec<_>>();
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].target_key(), batch.nodes()[entity_index].key());
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn generic_target_labels_union_sibling_queries_rules_and_declared_defaults() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{AstGraphGenericInput, AstGraphRelationTarget, assemble_graph_generic},
        source_inspection::IndexedSource,
        tree_sitter_generic_model::GenericRelationTarget,
    };
    for (target_label, competing_label, defaults) in [
        ("Method", false, false),
        ("Class", false, false),
        ("Rejected", false, false),
        ("Method", true, false),
        ("Method", false, true),
    ] {
        let original = Arc::new(context());
        let files = TempDir::new()?;
        let raw = raw(&files, r#"{"target":0,"caller":"target"}"#, &original).await?;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            return Err("stored source".into());
        };
        let (rule_labels, query_labels, sibling_labels) = if defaults {
            ("[]", "[]", "[]")
        } else {
            ("[Class]", "[Function]", "[Method]")
        };
        let competitor = if competing_label {
            "  - data_key: competing\n    graph_label: Class\n    pattern: '(pair key: (string (string_content) @name) (#eq? @name \"target\")) @unit'\n    span_capture: unit\n"
        } else {
            ""
        };
        let yaml = format!(
            "language: native-labels\ngrammar: tree-sitter-json\ncontext_types: {{}}\ntarget_rules:\n  REFERENCES: {{labels: {rule_labels}}}\nqueries:\n  - data_key: names\n    graph_label: {target_label}\n    pattern: '(pair key: (string (string_content) @name)) @unit'\n    span_capture: unit\n{competitor}  - data_key: references\n    type: relation\n    relation_type: REFERENCES\n    target_labels: {query_labels}\n    pattern: '(pair value: (string (string_content) @name))'\n  - data_key: sibling\n    type: relation\n    relation_type: REFERENCES\n    target_labels: {sibling_labels}\n    pattern: '(array (string (string_content) @name))'\n"
        );
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(text, &original, |input| {
            let parsed = factory.collect(input, &["model.native"], limits())?;
            let document = parsed.assemble()?;
            assert_eq!(document.relations()?.len(), 1);
            let relation = &document.relations()?[0];
            let batch = assemble_graph_generic(
                AstGraphGenericInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let edges = batch
                .edges()
                .iter()
                .filter(|e| e.kind() == "REFERENCES")
                .collect::<Vec<_>>();
            if competing_label {
                let GenericRelationTarget::Ambiguous { entity_indices } = &relation.target else {
                    return Err("union must preserve ambiguity".into());
                };
                assert_eq!(entity_indices.len(), 2);
                assert!(edges.is_empty());
                assert!(
                    matches!(batch.outcomes()[0].target(),AstGraphRelationTarget::Ambiguous{keys} if keys.len()==2)
                );
            } else if target_label == "Rejected" {
                assert_eq!(relation.target, GenericRelationTarget::NotFound);
                assert!(edges.is_empty());
            } else {
                let GenericRelationTarget::Resolved { entity_index } = relation.target else {
                    return Err(format!(
                        "effective labels did not resolve {target_label}, defaults={defaults}"
                    )
                    .into());
                };
                let entity = &document.entities()?[entity_index];
                assert_eq!(entity.identity().label(), target_label);
                assert_eq!(entity.name().display(), "target");
                assert_eq!(edges.len(), 1);
                assert_eq!(edges[0].target_key(), batch.nodes()[entity_index].key());
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[tokio::test]
async fn explicit_function_scopes_keep_r_owners_complexity_and_graph_endpoints() -> TestResult {
    use graphit_ast::{
        native_nn::NativeNnSource,
        semantic_graph::{AstGraphGenericInput, assemble_graph_generic},
        source_inspection::IndexedSource,
        tree_sitter_generic_model::GenericRelationTarget,
    };
    let original = Arc::new(context());
    let source = "target <- function() { 1 }\nouter <- function(x) {\n if (x) target()\n inside <- 1\n nested <- function(y) { if (y) target() }\n function(z) { if (z) target() }\n}\n";
    let files = TempDir::new()?;
    let raw = raw(&files, source, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        return Err("stored R source".into());
    };
    let yaml = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/graphit-ast/profiles/queries/r.yaml"),
    )?;
    let draft = catalog(&yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-r")?, &original)?,
        &original,
    )?;
    with_source(text, &original, |input| {
        let parsed = factory.collect(input, &["model.native"], limits())?;
        let document = parsed.assemble()?;
        assert!(document.syntax_recoveries()?.is_empty());
        let entities = document.entities()?;
        let find = |label: &str, name: &str| {
            entities
                .iter()
                .position(|e| e.identity().label() == label && e.name().display() == name)
        };
        let target = find("Function", "target").ok_or("target")?;
        let outer = find("Function", "outer").ok_or("outer")?;
        let nested = find("Function", "nested").ok_or("nested")?;
        let inside = find("Variable", "inside").ok_or("inside")?;
        assert_eq!(entities[nested].owners(), [outer]);
        assert_eq!(entities[inside].owners(), [outer]);
        assert!(find("Variable", "outer").is_some());
        assert!(find("Variable", "nested").is_some());
        for (function, value) in [(target, 1), (outer, 2), (nested, 2)] {
            assert!(
                matches!(&document.complexities()?[function],GenericComplexity::Score{value:actual,..} if *actual==value),
                "{} complexity {:?}",
                entities[function].name().display(),
                document.complexities()?[function]
            );
        }
        for name in ["x", "y"] {
            let parameter = find("Parameter", name).ok_or("parameter")?;
            assert!(entities[parameter].conflicts().is_empty());
            assert_eq!(
                entities[parameter]
                    .extent()
                    .ok_or("parameter fullspan")?
                    .range()
                    .text(),
                name
            );
        }
        let calls = document
            .relations()?
            .iter()
            .filter(|r| r.kind == "CALLS")
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().any(|r| r.source_index == Some(outer)
            && r.target
                == GenericRelationTarget::Resolved {
                    entity_index: target
                }));
        assert!(calls.iter().any(|r| r.source_index == Some(nested)
            && r.target
                == GenericRelationTarget::Resolved {
                    entity_index: target
                }));
        assert!(
            calls.iter().any(|r| r.source_index.is_none()
                && matches!(r.target, GenericRelationTarget::Unresolved { .. })),
            "unclaimed anonymous body must remain a source barrier"
        );
        let batch = assemble_graph_generic(
            AstGraphGenericInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        let edges = batch
            .edges()
            .iter()
            .filter(|e| e.kind() == "CALLS")
            .collect::<Vec<_>>();
        assert_eq!(edges.len(), 2);
        for caller in [outer, nested] {
            assert!(
                edges
                    .iter()
                    .any(|e| e.source_key() == batch.nodes()[caller].key()
                        && e.target_key() == batch.nodes()[target].key())
            );
        }
        Ok(())
    })
}

#[test]
fn direct_anonymous_child_does_not_become_a_clojure_functions_own_body() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericRelationTarget;
    let original = context();
    let stock = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/graphit-ast/profiles/queries/clojure.yaml"),
    )?;
    let yaml = stock.replacen(
        "    declaration_capture: unit\n",
        "    declaration_capture: unit\n    context_capture: unit\n",
        1,
    );
    let draft = catalog(yaml.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-clojure")?, &original)?,
        &original,
    )?;
    with_source(
        "(defn target [] 1)\n(defn outer [] #(if true (target) 0) (if true (target) 0) (defn nested [] (if true 1 0)))",
        &original,
        |input| {
            let parsed = factory.collect(input, &["model.clj"], limits())?;
            let document = parsed.assemble()?;
            assert!(document.syntax_recoveries()?.is_empty());
            let entities = document.entities()?;
            let outer = entities
                .iter()
                .position(|e| e.name().display() == "outer")
                .ok_or("outer defn")?;
            let nested = entities
                .iter()
                .position(|e| e.name().display() == "nested")
                .ok_or("nested defn")?;
            assert_eq!(entities[nested].owners(), [outer]);
            for index in [outer, nested] {
                assert!(
                    matches!(
                        document.complexities()?[index],
                        GenericComplexity::Score { value: 2, .. }
                    ),
                    "own body must exclude nested anonymous and named bodies: {:?}",
                    document.complexities()?[index]
                );
            }
            let calls = document
                .relations()?
                .iter()
                .filter(|r| matches!(r.target, GenericRelationTarget::Resolved { .. }))
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].source_index, Some(outer));
            assert!(
                document
                    .relations()?
                    .iter()
                    .any(|r| r.source_index.is_none()
                        && matches!(
                            r.target,
                            GenericRelationTarget::Unresolved {
                                reason: "unresolved native source scope"
                                    | "unowned anonymous source scope"
                            }
                        ))
            );
            Ok(())
        },
    )
}

#[test]
fn required_custom_contexts_use_real_entity_evidence_and_keep_unmatched_barriers() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    let original = context();
    let yaml=br#"language: native-context-assertion
grammar: tree-sitter-json
context_types: {pair: Pair}
context_name_queries: {pair: '(pair key: (_) @name) @context'}
context_requires_entity: [pair]
queries:
  - data_key: objects
    graph_label: Pair
    pattern: '(pair key: (string (string_content) @_key) @name value: (object) (#eq? @_key "real")) @unit'
    span_capture: unit
    name_is_data: true
  - data_key: numbers
    graph_label: Number
    pattern: '(pair key: (string) @name value: (number)) @unit'
    span_capture: unit
    name_is_data: true
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(
        r#"{"real":{"child":1},"unmatched":{"blocked":2}}"#,
        &original,
        |input| {
            let parsed = factory.collect(input, &["model.custom"], limits())?;
            let doc = parsed.assemble()?;
            assert!(doc.namespaces()?.is_empty());
            let entities = doc.entities()?;
            let parent = entities
                .iter()
                .position(|e| e.name().display() == "real")
                .ok_or("real parent")?;
            let child = entities
                .iter()
                .find(|e| e.name().display() == "child")
                .ok_or("real child")?;
            assert_eq!(child.owners(), [parent]);
            assert!(!entities.iter().any(|e| e.name().display() == "blocked"));
            assert!(
                doc.diagnostics()?
                    .iter()
                    .any(|d| d.issue == GenericEntityIssue::IncompleteContext)
            );
            Ok(())
        },
    )
}

#[test]
fn explicit_context_roles_retain_missing_invalid_and_ambiguous_evidence() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    let original = context();
    for (pattern, span, declaration, source, issue) in [
        (
            "[ (pair key: (string) @name value: (object) @scope) @unit (pair key: (string) @name value: (number)) @unit ]",
            "unit",
            "unit",
            r#"{"a":1}"#,
            GenericEntityIssue::MissingContext,
        ),
        (
            "(document (object (pair key: (string) @name) @unit) @scope)",
            "unit",
            "unit",
            r#"{"a":1}"#,
            GenericEntityIssue::InvalidContext,
        ),
        (
            "(object (pair key: (string) @name) (pair key: (string) @name)) @unit @scope",
            "unit",
            "unit",
            r#"{"a":1,"b":2}"#,
            GenericEntityIssue::AmbiguousContext,
        ),
    ] {
        let yaml = format!(
            "language: native-context-errors\ngrammar: tree-sitter-json\ndeclaration_types: [pair, object]\ncontext_types: {{}}\nqueries:\n  - graph_label: Pair\n    data_key: pairs\n    pattern: '{pattern}'\n    span_capture: {span}\n    declaration_capture: {declaration}\n    context_capture: scope\n    name_is_data: true\n"
        );
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(source, &original, |input| {
            let parsed = factory.collect(input, &["model.custom"], limits())?;
            let doc = parsed.assemble()?;
            assert!(
                doc.diagnostics()?.iter().any(|d| d.issue == issue),
                "{issue:?}: {:?}",
                doc.diagnostics()?
            );
            assert!(
                doc.entities()?
                    .iter()
                    .all(|e| e.contributions().iter().all(|c| c.contexts.is_empty()))
            );
            Ok(())
        })?;
    }
    let yaml=b"language: native-context-errors\ngrammar: tree-sitter-json\nqueries:\n  - graph_label: Pair\n    data_key: pairs\n    pattern: '(pair key: (string) @name) @unit'\n    context_capture: missing_role\n";
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    assert!(matches!(
        PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original
        ),
        Err(GenericModelError::InvalidRole {
            role: "context",
            ..
        })
    ));
    // A narrow display span must not prevent a proven declaration from owning its body.
    let yaml=b"language: native-context-declaration\ngrammar: tree-sitter-json\ndeclaration_types: [pair]\ncontext_types: {}\nqueries:\n  - graph_label: Pair\n    data_key: pairs\n    pattern: '(pair key: (string) @name value: (object) @scope) @unit'\n    span_capture: name\n    declaration_capture: unit\n    context_capture: scope\n    name_is_data: true\n";
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"a":{}}"#, &original, |input| {
        let parsed = factory.collect(input, &["model.custom"], limits())?;
        let doc = parsed.assemble()?;
        assert!(doc.diagnostics()?.is_empty());
        assert_eq!(doc.entities()?[0].contributions()[0].contexts.len(), 1);
        Ok(())
    })
}

#[test]
fn native_explicit_parent_cycles_do_not_fabricate_identities() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::GenericEntityIssue;
    let original = context();
    let yaml=br#"language: native-cycle-fixture
grammar: tree-sitter-json
context_types: {}
queries:
  - data_key: pairs
    graph_label: Pair
    pattern: '(object (pair key: (string (string_content) @name)) @unit (pair key: (string (string_content) @parent)))'
    span_capture: unit
    parent_capture: parent
    parent_label: Pair
  - data_key: pairs
    graph_label: Pair
    pattern: '(object (pair key: (string (string_content) @parent)) (pair key: (string (string_content) @name)) @unit)'
    span_capture: unit
    parent_capture: parent
    parent_label: Pair
"#;
    let draft = catalog(yaml, &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"a":1,"b":2}"#, &original, |input| {
        let parsed = factory.collect(input, &["cycle.custom"], limits())?;
        assert_eq!(
            parsed
                .observations()?
                .groups
                .iter()
                .map(|g| g.matches.len())
                .sum::<usize>(),
            2
        );
        let doc = parsed.assemble()?;
        assert!(doc.entities()?.is_empty());
        assert!(doc.namespaces()?.is_empty());
        assert_eq!(
            doc.diagnostics()?
                .iter()
                .filter(|d| d.issue == GenericEntityIssue::InconsistentOwnerChain)
                .count(),
            2
        );
        Ok(())
    })
}

#[test]
fn embedded_errors_unused_recipes_and_original_cancellation_remain_explicit() -> TestResult {
    use graphit_ast::tree_sitter_generic_model::{GenericEmbeddedIssue, GenericEmbeddedLanguage};
    let base = r#"language: native-embedded-errors
grammar: tree-sitter-json
context_types: {}
queries:
  - data_key: pairs
    graph_label: Pair
    pattern: '(pair key: (string (string_content) @name)) @unit'
    span_capture: unit
embedded:
  - pattern: '(pair value: (string) @body)'
    text_capture: body
    default: javascript
    normalize: missing-recipe
text_normalizers:
  unused:
    replace: {x: y}
"#;
    for (source, expect_body) in [
        (r#"{"visible":1}"#, false),
        (r#"{"visible":"call()"}"#, true),
    ] {
        let original = context();
        let draft = catalog(base.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        let factory = PreparedGenericTreeSitterModel::prepare(
            profile,
            PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
            &original,
        )?;
        with_source(source, &original, |input| {
            let parsed = factory.collect(input, &["embedded.custom"], limits())?;
            assert_eq!(
                parsed.assemble()?.entities()?.len(),
                1,
                "unused or failing embedded recipe cannot erase host entities"
            );
            assert_eq!(parsed.embedded_blocks()?.len(), usize::from(expect_body));
            if expect_body {
                assert!(
                    matches!(parsed.prepare_embedded(0)?,Err(GenericEmbeddedIssue::MissingNormalizer(name)) if name=="missing-recipe")
                );
            }
            Ok(())
        })?;
    }
    let original = context();
    for (old, new, role) in [
        ("text_capture: body", "text_capture: absent", "text"),
        ("default: javascript", "lang_capture: absent", "lang"),
    ] {
        let yaml = base.replace(old, new);
        let draft = catalog(yaml.as_bytes(), &original)?;
        let profile = &draft.profiles()[0];
        assert!(
            matches!(PreparedGenericTreeSitterModel::prepare(profile,PreparedTreeSitterContexts::prepare(profile,builtin("tree-sitter-json")?,&original)?,&original),Err(GenericModelError::InvalidEmbeddedRole{role:actual,..}) if actual==role)
        );
    }
    let missing_language = base
        .replace("    default: javascript\n", "")
        .replace("    normalize: missing-recipe\n", "");
    let draft = catalog(missing_language.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"visible":"call()"}"#, &original, |input| {
        let parsed = factory.collect(input, &["embedded.custom"], limits())?;
        assert_eq!(
            parsed.embedded_blocks()?[0].language,
            GenericEmbeddedLanguage::Missing
        );
        assert!(matches!(
            parsed.prepare_embedded(0)?,
            Err(GenericEmbeddedIssue::LanguageUnavailable)
        ));
        Ok(())
    })?;
    let valid = base.replace("    normalize: missing-recipe\n", "");
    let draft = catalog(valid.as_bytes(), &original)?;
    let profile = &draft.profiles()[0];
    let factory = PreparedGenericTreeSitterModel::prepare(
        profile,
        PreparedTreeSitterContexts::prepare(profile, builtin("tree-sitter-json")?, &original)?,
        &original,
    )?;
    with_source(r#"{"visible":"call()"}"#, &original, |input| {
        let parsed = factory.collect(input, &["embedded.custom"], limits())?;
        let child = parsed
            .prepare_embedded(0)?
            .map_err(|e| format!("unexpected transform issue: {e:?}"))?;
        child.with_input(|child| {
            assert!(std::ptr::eq(child.original_context(), &original));
            Ok::<_, GenericModelError>(())
        })?;
        original.cancellation().cancel();
        assert!(
            matches!(parsed.embedded_blocks(),Err(GenericModelError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(
            matches!(parsed.prepare_embedded(0),Err(GenericModelError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        assert!(
            matches!(child.with_input(|_|Ok::<_,GenericModelError>(())),Err(GenericModelError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        Ok(())
    })
}
