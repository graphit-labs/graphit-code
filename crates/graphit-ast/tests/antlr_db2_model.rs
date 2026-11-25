use graphit_ast::{
    antlr_engine::model::{
        AntlrDeclarationRole, AntlrEntityExtent, AntlrIssue, AntlrModelError, AntlrProjection,
        AntlrTypeObservation, AntlrTypeRole, AntlrValueObservation, ParsedAntlrDocument,
    },
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    symbols::{IdentityLimits, IdentityLimitsInput},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{error::Error, io::ErrorKind, time::Instant};
type TestResult = Result<(), Box<dyn Error>>;
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn limits() -> IdentityLimits {
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

fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("semantic-json-fixture", 256).unwrap();
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

fn with_input(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let prepared = text.to_owned();
    assert_ne!(text.as_ptr(), prepared.as_ptr());
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

#[test]
fn stock_db2_headers_and_nine_parameter_slots_assemble() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let fixtures: [(&str, &str, &str, &[&str], &str); 11] = [
        (
            "CREATE FUNCTION F_SCALAR(P_SCALAR INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE JAVA PARAMETER STYLE JAVA EXTERNAL NAME 'pkg.scalar' NO SQL;",
            "functions",
            "F_SCALAR",
            &["P_SCALAR"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_TABLE(P_TABLE CLOB DEFAULT NULL AS LOCATOR) RETURNS TABLE(C_A INTEGER) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.table' NO SQL;",
            "functions",
            "F_TABLE",
            &["P_TABLE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_OLE(P_OLE VARCHAR(40) DEFAULT 'cmd') RETURNS TABLE(C_A INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
            "functions",
            "F_OLE",
            &["P_OLE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_SOURCE(P_SOURCE INTEGER DEFAULT 1) RETURNS INTEGER SOURCE SOURCE_F(INTEGER);",
            "functions",
            "F_SOURCE",
            &["P_SOURCE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_TEMPLATE(P_TEMPLATE INTEGER DEFAULT 1) RETURNS INTEGER AS TEMPLATE NOT DETERMINISTIC NO EXTERNAL ACTION;",
            "functions",
            "F_TEMPLATE",
            &["P_TEMPLATE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_SQL(P_SQL INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC DECLARE LOCAL_F INTEGER DEFAULT 2; RETURN P_SQL; END;",
            "functions",
            "F_SQL",
            &["P_SQL"],
            "create_function_statement",
        ),
        (
            "CREATE PROCEDURE P_EXTERNAL(IN P_IN INTEGER DEFAULT 1,OUT P_OUT INTEGER,INOUT P_IO INTEGER) LANGUAGE JAVA PARAMETER STYLE JAVA EXTERNAL NAME 'pkg.proc';",
            "procedures",
            "P_EXTERNAL",
            &["P_IN", "P_OUT", "P_IO"],
            "create_procedure_statement",
        ),
        (
            "CREATE PROCEDURE P_SQL(IN P_IN INTEGER DEFAULT 1,OUT P_OUT INTEGER,INOUT P_IO INTEGER) LANGUAGE SQL BEGIN ATOMIC CALL OTHER_F(P_IN=>P_IN); SET P_OUT=P_IN; RETURN; END;",
            "procedures",
            "P_SQL",
            &["P_IN", "P_OUT", "P_IO"],
            "create_procedure_statement",
        ),
        (
            "CREATE METHOD METHOD_F(P_METHOD INTEGER,P_LOCATOR CLOB AS LOCATOR) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
            "functions",
            "METHOD_F",
            &["P_METHOD", "P_LOCATOR"],
            "method_signature",
        ),
        (
            "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_SPEC INTEGER,P_SPEC_2 INTEGER) RETURNS INTEGER LANGUAGE SQL;",
            "functions",
            "METHOD_F",
            &["P_SPEC", "P_SPEC_2"],
            "method_specification",
        ),
        (
            "CREATE FUNCTION AGG_F(IN P_AGG DOUBLE DEFAULT 1) RETURNS DOUBLE AGGREGATE WITH (S_STATE DOUBLE) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F;",
            "functions",
            "AGG_F",
            &["P_AGG"],
            "create_function_aggregate_interface_statement",
        ),
    ];

    for (body, key, name, parameters, _) in fixtures {
        let source = format!("-- α\r\n/* β /* γ */ δ */ {body}");
        let original = context();
        with_input(&source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "headers.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{body}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            assert!(
                model.diagnostics().is_empty(),
                "{body}: {:?}; unindexed {:?}",
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>(),
                parsed
                    .observations()
                    .syntax
                    .nodes
                    .iter()
                    .filter(|n| matches!(
                        n.status,
                        graphit_ast::antlr_engine::SpanStatus::Unindexed(_)
                    ))
                    .collect::<Vec<_>>()
            );
            let owner = model
                .entities()
                .iter()
                .find(|e| e.data_key() == key && e.name() == name)
                .expect("actual header entity");
            assert!(
                matches!(owner.extent(), AntlrEntityExtent::Exact(e) if e.range().text().contains(name))
            );
            let actual = model
                .entities()
                .iter()
                .filter(|e| e.data_key() == "parameters")
                .collect::<Vec<_>>();
            assert_eq!(
                actual.iter().map(|e| e.name()).collect::<Vec<_>>(),
                parameters
            );
            for param in actual {
                assert!(
                    param
                        .namespace()
                        .iter()
                        .any(|n| n.identity().uid().as_str() == owner.identity().uid().as_str())
                );
                assert!(matches!(
                    param.contributions()[0].declared_type,
                    AntlrTypeObservation::Present { .. }
                ));
                assert_eq!(
                    param.contributions()[0].type_roles[0].role,
                    AntlrTypeRole::Declared
                );
            }
            if key == "functions" {
                assert!(
                    !owner.contributions()[0].type_roles.is_empty(),
                    "typed function return"
                );
            }
            for e in model.entities() {
                assert_eq!(
                    e.name_raw().text().as_ptr(),
                    source[e.name_raw().start()..].as_ptr()
                );
            }
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, AntlrProjection::Available(_)))
            );
            Ok(())
        })?;
    }
    Ok(())
}
#[test]
fn stock_db2_families_qnames_groups_and_projection_policy() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    for body in [
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[100];",
        "CREATE TYPE CURSOR_F AS CURSOR;",
        "CREATE TYPE DISTINCT_F AS INTEGER;",
        "CREATE TYPE ROW_F AS ROW(FIELD_F INTEGER);",
        "CREATE TABLE TABLE_F(C INTEGER);",
        "CREATE INDEX INDEX_F ON TABLE_F(C,D DESC) CLUSTER;",
        "CREATE VIEW V_VALUES_F(C_A) AS VALUES 11;",
        "CREATE VIEW V_SELECT_F AS SELECT 11 FROM SRC_DUMMY_F;",
        "CREATE SEQUENCE SEQ_F AS INTEGER;",
        "CREATE SEQUENCE SEQ_NONE_F;",
        "CREATE SCHEMA S_F;",
        "CREATE SCHEMA S_F AUTHORIZATION OWNER_F CREATE VIEW V_F(C_A) AS VALUES 11;",
        "CREATE SCHEMA AUTHORIZATION OWNER_F CREATE VIEW V_F(C_A) AS VALUES 11;",
        "CREATE SCHEMA AUTHORIZATION OWNER_F;",
        "CREATE TABLESPACE TS_F MANAGED BY AUTOMATIC STORAGE;",
        "CREATE TRIGGER TR_F AFTER INSERT ON T_F FOR EACH ROW INSERT INTO AUDIT_F VALUES 11;",
        "CREATE ALIAS A_F FOR TABLE T_F;",
        "CREATE ALIAS A_F FOR MODULE M_F;",
        "CREATE ALIAS A_F FOR SEQUENCE Q_F;",
        "BEGIN ATOMIC DECLARE V_A,V_B INTEGER DEFAULT NULL; END;",
        "CREATE PROCEDURE P_F() LANGUAGE SQL BEGIN DECLARE V_F INTEGER DEFAULT 1; END;",
        "CREATE PROCEDURE P_SOURCE SOURCE REMOTE_SCHEMA.SOURCE_PROC_F() FOR SERVER SERVER_F;",
        "CREATE FUNCTION FUNC_SCALAR_F(INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
        "CREATE METHOD METHOD_F() RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE FUNCTION \"café\".\"função\"(\"parâmetro\" INTEGER) RETURNS INTEGER LANGUAGE SQL RETURN 1;",
    ] {
        let original = context();
        let source = format!("-- α\r\n{body}");
        with_input(&source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "families.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{body}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            assert!(
                model.diagnostics().is_empty(),
                "{body}: {:?}",
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            if body == "CREATE SCHEMA AUTHORIZATION OWNER_F;" {
                assert!(model.entities().is_empty());
            } else {
                assert!(!model.entities().is_empty(), "{body}");
            }
            for entity in model.entities() {
                let q = entity.qualified_name().expect("DB2 QName");
                assert_eq!(
                    entity.name(),
                    q.components().last().unwrap().decoded.as_ref()
                );
                for component in q.components() {
                    assert_eq!(
                        component.raw.text().as_ptr(),
                        source[component.raw.start()..].as_ptr()
                    );
                }
                assert!(matches!(entity.extent(), AntlrEntityExtent::Exact(_)));
                if entity.role() == AntlrDeclarationRole::IndexKeySpecification {
                    assert_eq!(
                        entity.namespace().last().unwrap().identity().label(),
                        "Index"
                    );
                }
            }
            if body.contains("V_A,V_B") {
                let vars = model
                    .entities()
                    .iter()
                    .filter(|e| e.data_key() == "variables")
                    .collect::<Vec<_>>();
                assert_eq!(vars.len(), 2);
                assert_ne!(
                    vars[0].identity().uid().as_str(),
                    vars[1].identity().uid().as_str()
                );
                assert_eq!(
                    vars[0].declaration_node_index(),
                    vars[1].declaration_node_index()
                );
            }
            for projection in model.code_projections()? {
                let AntlrProjection::Available(p) = projection else {
                    panic!("exact stock projection")
                };
                assert!(
                    draft.profiles()[0]
                        .embed_labels()
                        .value()
                        .iter()
                        .any(|l| l.value() == p.entity.identity().label())
                );
                assert_eq!(p.language, "db2");
                assert_eq!(p.grammar, "antlr-db2");
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn custom(queries: &str) -> CatalogDraft {
    catalog(format!("language: db2\nparser: antlr4\ngrammar: antlr-db2\nembed_labels: [Table,Variable,Custom]\nantlr_contexts: [{{pattern: '//create_table_statement', label: Table, name: [{{xpath: '/table_name'}}]}}]\nqueries:\n{queries}").as_bytes())
}
#[test]
fn db2_subject_diagnostics_and_custom_contributions_do_not_smear_groups() -> TestResult {
    let normal = "  - {data_key: t, graph_label: Table, type: entity, pattern: '//create_table_statement/table_name'}\n";
    for bad in [
        "  - {data_key: t, graph_label: Table, type: entity, pattern: '//create_table_statement', captures: {name: [{xpath: '/alias_name'}]}}\n",
        "  - {data_key: t, graph_label: Table, type: entity, pattern: '//create_table_statement', captures: {name: [{nearest_descendant: 'column_name'}]}}\n",
    ] {
        for queries in [format!("{normal}{bad}"), format!("{bad}{normal}")] {
            let draft = custom(&queries);
            let original = context();
            with_input("CREATE TABLE T(C INTEGER);", &original, |input| {
                let parsed = ParsedAntlrDocument::collect(
                    &draft.profiles()[0],
                    input,
                    &["src", "policy.db2"],
                    limits(),
                )?;
                let model = parsed.assemble()?;
                assert_eq!(model.entities().len(), 1);
                assert!(!model.diagnostics().is_empty());
                assert!(!model.entities()[0].diagnostic_indices().is_empty());
                assert!(matches!(
                    &model.code_projections()?[0],
                    AntlrProjection::Unavailable(_)
                ));
                Ok(())
            })?;
        }
    }

    let normal_group = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement/variable_name'}\n";
    let missing_group = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement', captures: {name: [{xpath: '/alias_name'}]}}\n";

    let mismatch_group = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement', captures: {name: [{xpath: '/data_type'}]}}\n";
    for queries in [
        format!("{normal_group}{missing_group}"),
        format!("{missing_group}{normal_group}"),
        format!("{normal_group}{mismatch_group}"),
        format!("{mismatch_group}{normal_group}"),
    ] {
        let draft = custom(&queries);
        let original = context();
        with_input(
            "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
            &original,
            |input| {
                let parsed = ParsedAntlrDocument::collect(
                    &draft.profiles()[0],
                    input,
                    &["src", "group-missing.db2"],
                    limits(),
                )?;
                let model = parsed.assemble()?;
                assert_eq!(model.entities().len(), 2);
                assert!(
                    model
                        .entities()
                        .iter()
                        .all(|e| !e.diagnostic_indices().is_empty())
                );
                assert!(
                    model
                        .code_projections()?
                        .iter()
                        .all(|p| matches!(p, AntlrProjection::Unavailable(_)))
                );
                Ok(())
            },
        )?;
    }
    let draft = custom(
        "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement/variable_name', name_reject: '^V_A$'}\n",
    );
    let original = context();
    with_input(
        "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
        &original,
        |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "group.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert_eq!(model.entities().len(), 1);
            assert_eq!(model.entities()[0].name(), "V_B");
            assert!(
                model.entities()[0].diagnostic_indices().is_empty(),
                "V_A exclusion cannot smear V_B"
            );
            assert_eq!(model.entities()[0].identity().label(), "Custom");
            assert_eq!(model.entities()[0].data_key(), "vars");
            assert!(matches!(
                &model.code_projections()?[0],
                AntlrProjection::Available(_)
            ));
            Ok(())
        },
    )?;
    Ok(())
}

#[test]
fn db2_recovery_and_actual_original_controls_prevent_partial_delivery() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let original = context();
    with_input("CREATE TABLE T(C INTEGER", &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["src", "recovery.db2"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        assert!(!parsed.observations().syntax.diagnostics.is_empty());
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|d| matches!(d.issue, AntlrIssue::Recovery { .. }))
        );
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, AntlrProjection::Unavailable(_)))
        );
        Ok(())
    })?;
    let cancellation = Cancellation::default();
    let original = SnapshotContext::service(cancellation.clone());
    with_input("CREATE TABLE T(C INTEGER);", &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["src", "cancel.db2"],
            limits(),
        )?;
        cancellation.cancel();
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    let original = SnapshotContext::new(Instant::now(), Cancellation::default());
    with_input("CREATE TABLE T(C INTEGER);", &context(), |input| {
        // A distinct expired control cannot replace acquisition's actual Original.
        assert!(original.check().is_err());
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["src", "valid.db2"],
            limits(),
        )?;
        assert!(!parsed.assemble()?.entities().is_empty());
        Ok(())
    })?;
    Ok(())
}

#[test]
fn db2_configured_defaults_keep_missing_distinct_from_null() -> TestResult {
    let draft = custom(
        "  - {data_key: vars, graph_label: Variable, type: entity, pattern: '//declare_variable_statement', value_label: Value, captures: {name: [{xpath: '/variable_name'}], value: [{xpath: '/declare_variable_default_clause/default_values'}]}}\n",
    );
    for (body, has_null) in [
        ("BEGIN ATOMIC DECLARE V INTEGER DEFAULT NULL; END;", true),
        ("BEGIN ATOMIC DECLARE V INTEGER; END;", false),
    ] {
        let original = context();
        with_input(body, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "default.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(model.diagnostics().is_empty());
            let value = &model.entities()[0].contributions()[0].declared_value;
            assert_eq!(
                matches!(value,AntlrValueObservation::Scalar{token:sqlparser::tokenizer::Token::Word(w),..} if w.keyword==sqlparser::keywords::Keyword::NULL),
                has_null
            );
            if !has_null {
                assert!(matches!(value, AntlrValueObservation::Missing));
            }
            Ok(())
        })?;
    }
    Ok(())
}

mod native_nn {
    use super::*;
    use arrow_array::{RecordBatch, StringArray};
    use graphit_ast::{
        indexed_source::{IndexedSourceSnapshot, read_indexed_source, source_schema},
        native_nn::{
            NativeNnAntlrInput, NativeNnError, NativeNnModel, NativeNnSource,
            encode_native_nn_antlr, validate_native_nn,
        },
        source_inspection::IndexedSource,
    };
    use graphit_data::lance_module_store::LanceModuleStore;
    use std::sync::Arc;
    use tempfile::TempDir;
    async fn raw(
        temp: &TempDir,
        name: &str,
        path: &str,
        text: Option<&str>,
        original: &Arc<SnapshotContext>,
    ) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
        let batch = RecordBatch::try_new(
            source_schema(),
            vec![
                Arc::new(StringArray::from(vec![path])),
                Arc::new(StringArray::from(vec![text])),
            ],
        )?;
        let store = LanceModuleStore::create(
            temp.path().join(name).to_str().unwrap(),
            source_schema(),
            vec![batch],
        )
        .await?;
        Ok(read_indexed_source(&store.snapshot(), path, original).await?)
    }
    fn nn_model() -> NativeNnModel<'static> {
        NativeNnModel {
            vector_id: [83; 32],
            dimensions: 2,
            model_id: "actual-db2-fixture",
            revision: "one",
        }
    }
    #[tokio::test]
    async fn db2_real_encoder_qualified_subject_owner_and_causal_association() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let raw=raw(&temp,"source.lance","types.db2",Some("CREATE TYPE APP.TYPE_F AS(A_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL;"),&original).await?;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            panic!("actual Source");
        };
        let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
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
            text.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["types.db2"], limits())?;
        let document = parsed.assemble()?;
        assert!(
            document.diagnostics().is_empty(),
            "{:?}",
            document
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        let indices = document
            .code_projections()?
            .iter()
            .map(|p| match p {
                AntlrProjection::Available(p) => p.entity_index,
                _ => panic!("exact projection"),
            })
            .collect::<Vec<_>>();
        assert_eq!(indices.len(), 2, "Type and Method, not Parameter");
        let vectors = vec![vec![1., 0.]; indices.len()];
        let batch = encode_native_nn_antlr(
            &nn_model(),
            &[NativeNnAntlrInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7),
            }],
            &original,
        )?;
        validate_native_nn(&batch, &nn_model(), &original)?;
        assert_eq!(batch.num_rows(), 2);
        for (row, &index) in indices.iter().enumerate() {
            let entity = &document.entities()[index];
            for (field, expected) in [
                ("name", entity.name()),
                ("language", "db2"),
                ("grammar", "antlr-db2"),
                ("uid", entity.identity().uid().as_str()),
                ("data_key", entity.data_key()),
            ] {
                let a = batch
                    .column_by_name(field)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();
                assert_eq!(a.value(row), expected);
            }
        }
        assert!(
            document
                .entities()
                .iter()
                .any(|e| e.name() == "TYPE_F" && e.name_raw().text() == "APP.TYPE_F")
        );
        assert!(document.entities().iter().any(|e| {
            e.name() == "METHOD_F"
                && e.namespace()
                    .last()
                    .is_some_and(|n| n.anchor().text() == "APP.TYPE_F")
        }));
        let foreign = Arc::new(context());
        assert!(matches!(
            encode_native_nn_antlr(
                &nn_model(),
                &[NativeNnAntlrInput {
                    document: &document,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: &indices,
                    vectors: &vectors,
                    model_generation: Some(7)
                }],
                &foreign
            ),
            Err(NativeNnError::RequestMismatch)
        ));
        for (copy, path) in [
            (text.to_owned(), vec!["types.db2"]),
            (text.to_owned(), vec!["wrong.db2"]),
        ] {
            let input = PreparedInput::new(
                copy.as_bytes(),
                copy.as_bytes(),
                OriginPlan {
                    segments: &segments,
                    removed_original: &[],
                    anchors: &[],
                },
                &original,
            )?;
            let parsed =
                ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &path, limits())?;
            let copied = parsed.assemble()?;
            assert!(matches!(
                encode_native_nn_antlr(
                    &nn_model(),
                    &[NativeNnAntlrInput {
                        document: &copied,
                        source: NativeNnSource::Indexed(&raw),
                        entity_indices: &indices,
                        vectors: &vectors,
                        model_generation: Some(7)
                    }],
                    &original
                ),
                Err(NativeNnError::InvalidInput)
            ));
        }
        let mut columns = batch.columns().to_vec();
        columns[batch.schema().index_of("grammar")?] =
            Arc::new(StringArray::from(vec!["antlr-plsql"; batch.num_rows()]));
        let invalid = RecordBatch::try_new(batch.schema(), columns)?;
        assert!(matches!(
            validate_native_nn(&invalid, &nn_model(), &original),
            Err(NativeNnError::InvalidRow)
        ));
        Ok(())
    }
    #[tokio::test]
    async fn db2_stored_empty_source_is_available_and_null_is_not() -> TestResult {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let empty = raw(&temp, "empty.lance", "empty.db2", Some(""), &original).await?;
        let null = raw(&temp, "null.lance", "empty.db2", None, &original).await?;
        let IndexedSource::Available(text) = empty.source(&original)? else {
            panic!("stored empty is available");
        };
        assert!(text.is_empty());
        assert!(!matches!(
            null.source(&original)?,
            IndexedSource::Available(_)
        ));
        let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
        let input = PreparedInput::new(
            text.as_bytes(),
            text.as_bytes(),
            OriginPlan {
                segments: &[],
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["empty.db2"], limits())?;
        let document = parsed.assemble()?;
        let encode = |source| {
            encode_native_nn_antlr(
                &nn_model(),
                &[NativeNnAntlrInput {
                    document: &document,
                    source: NativeNnSource::Indexed(source),
                    entity_indices: &[],
                    vectors: &[],
                    model_generation: None,
                }],
                &original,
            )
        };
        let batch = encode(&empty)?;
        assert_eq!(batch.num_rows(), 0);
        validate_native_nn(&batch, &nn_model(), &original)?;
        assert!(matches!(encode(&null), Err(NativeNnError::InvalidInput)));
        Ok(())
    }
}

#[test]
fn db2_written_return_roles_preserve_cast_and_column_order() -> TestResult {
    use AntlrTypeRole::{CastSource, CastTarget, Return, ReturnColumn};
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    type ExpectedReturn<'a> = (AntlrTypeRole, &'a str, Option<&'a str>);
    let cases: Vec<(&str, Vec<ExpectedReturn<'_>>)> = vec![
        (
            "CREATE METHOD METHOD_F(P_F INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
            vec![
                (CastTarget, "VARCHAR()", None),
                (CastSource, "VARCHAR(40)", None),
            ],
        ),
        (
            "CREATE TYPE TYPE_F AS(A_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_F INTEGER) RETURNS VARCHAR() CAST FROM VARCHAR(40) LANGUAGE SQL;",
            vec![
                (CastTarget, "VARCHAR()", None),
                (CastSource, "VARCHAR(40)", None),
            ],
        ),
        (
            "CREATE FUNCTION AGG_F() RETURNS CHAR() CAST FROM CHAR(10) AGGREGATE WITH (DOUBLE) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F;",
            vec![(CastTarget, "CHAR()", None), (CastSource, "CHAR(10)", None)],
        ),
        (
            "CREATE FUNCTION F_CAST() RETURNS INTEGER CAST FROM INTEGER LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.scalar' NO SQL;",
            vec![(CastTarget, "INTEGER", None), (CastSource, "INTEGER", None)],
        ),
        (
            "CREATE FUNCTION F_TABLE(P_F INTEGER) RETURNS TABLE(C_A INTEGER,C_B VARCHAR(40)) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.table' NO SQL;",
            vec![
                (ReturnColumn, "INTEGER", Some("C_A")),
                (ReturnColumn, "VARCHAR(40)", Some("C_B")),
            ],
        ),
        (
            "CREATE FUNCTION F_SQL_RET(P_F INTEGER) RETURNS TABLE(C_D APP.DIST_F,C_S APP.STRUCT_F,C_R REF(APP.STRUCT_F)) LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN SELECT C_D,C_S,C_R FROM SRC_TYPES_F; END;",
            vec![
                (ReturnColumn, "APP.DIST_F", Some("C_D")),
                (ReturnColumn, "APP.STRUCT_F", Some("C_S")),
                (ReturnColumn, "REF(APP.STRUCT_F)", Some("C_R")),
            ],
        ),
        (
            "CREATE FUNCTION F_SQL_RET(P_OBJ TYPE_F) RETURNS ROW(C_A INTEGER,C_B INTEGER) LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN SELECT 1,2 FROM SRC_DUMMY_F; END;",
            vec![
                (ReturnColumn, "INTEGER", Some("C_A")),
                (ReturnColumn, "INTEGER", Some("C_B")),
            ],
        ),
        (
            "CREATE METHOD METHOD_F(P_F INTEGER) RETURNS REF(APP.TYPE_F) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
            vec![(Return, "REF(APP.TYPE_F)", None)],
        ),
        (
            "CREATE FUNCTION F_EXTERNAL_RETURN(P_F INTEGER) RETURNS DATE LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            vec![(Return, "DATE", None)],
        ),
        (
            "CREATE FUNCTION F_EXTERNAL_RETURN(P_F INTEGER) RETURNS REF(APP.TYPE_F) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            vec![(Return, "REF(APP.TYPE_F)", None)],
        ),
        (
            "CREATE FUNCTION F_EXTERNAL_RETURN(P_F INTEGER) RETURNS XML AS CLOB(2M) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            vec![(Return, "XML AS CLOB(2M)", None)],
        ),
        (
            "CREATE FUNCTION F_EXTERNAL_RETURN(P_F INTEGER) RETURNS CLOB(2M) AS LOCATOR LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            vec![(Return, "CLOB(2M)", None)],
        ),
        (
            "CREATE FUNCTION F_EXTERNAL_RETURN(P_F INTEGER) RETURNS APP.TYPE_F LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            vec![(Return, "APP.TYPE_F", None)],
        ),
        (
            "CREATE FUNCTION F_SQL_SCALAR(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN NULL; END;",
            vec![(Return, "INTEGER", None)],
        ),
        (
            "CREATE FUNCTION F_SQL_SCALAR(P_F INTEGER) RETURNS DATE LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN NULL; END;",
            vec![(Return, "DATE", None)],
        ),
        (
            "CREATE FUNCTION F_SQL_SCALAR(P_F INTEGER) RETURNS REF(APP.TYPE_F) LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN NULL; END;",
            vec![(Return, "REF(APP.TYPE_F)", None)],
        ),
    ];
    for (body, expected) in cases {
        let original = context();
        with_input(body, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "returns.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{body}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            assert!(
                model.diagnostics().is_empty(),
                "{body}: {:?}",
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            let function = model
                .entities()
                .iter()
                .find(|e| e.data_key() == "functions")
                .unwrap();
            let roles = &function.contributions()[0].type_roles;
            assert_eq!(roles.len(), expected.len(), "{body}");
            let mut previous = None;
            for (observed, (role, text, name)) in roles.iter().zip(expected) {
                assert_eq!(observed.role, role);
                let AntlrTypeObservation::Present { node_index, raw } = &observed.observation
                else {
                    panic!("native exact type");
                };
                assert_eq!(raw.text(), text);
                assert_eq!(raw.text().as_ptr(), body[raw.start()..].as_ptr());
                assert_eq!(
                    parsed.observations().syntax.nodes[*node_index].span,
                    Some(raw.start()..raw.end())
                );
                if body.starts_with("CREATE FUNCTION F_SQL_SCALAR") {
                    let node = &parsed.observations().syntax.nodes[*node_index];
                    assert_eq!(node.kind, "data_type_1");
                    assert_eq!(
                        parsed.observations().syntax.nodes[node.parent.unwrap()].kind,
                        "create_function_sql_scalar_table_or_row_statement"
                    );
                }
                if body.starts_with("CREATE FUNCTION F_EXTERNAL_RETURN") {
                    let node = &parsed.observations().syntax.nodes[*node_index];
                    assert_eq!(node.kind, "external_scalar_ordinary_return_type");
                    assert_eq!(
                        parsed.observations().syntax.nodes[node.parent.unwrap()].kind,
                        "create_function_external_scalar_statement"
                    );
                }
                if let Some(previous) = previous {
                    assert!(previous < *node_index, "native preorder");
                }
                previous = Some(*node_index);
                assert_eq!(
                    observed.name_node_index.map(|i| body
                        .get(parsed.observations().syntax.nodes[i].span.clone().unwrap())
                        .unwrap()),
                    name
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn db2_native_parameter_type_bindings_preserve_original() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    type ExpectedParameter<'a> = (&'a str, &'a str, &'a str, &'a str);
    let cases: &[(&str, &[ExpectedParameter<'_>])] = &[
        (
            "CREATE FUNCTION F_SQL_BIND(P_SQL INTEGER,P_REF REF(APP.TYPE_F)) RETURNS INTEGER LANGUAGE SQL READS SQL DATA BEGIN ATOMIC RETURN NULL; END;",
            &[
                ("P_SQL", "INTEGER", "data_type_1", "param_decl_2"),
                ("P_REF", "REF(APP.TYPE_F)", "data_type_1", "param_decl_2"),
            ],
        ),
        (
            "CREATE FUNCTION F_EXTERNAL_BIND(P_REF REF(APP.TYPE_F),P_XML XML AS CLOB(2M),P_INT INTEGER,REF(APP.TYPE_F),XML AS CLOB(2M)) RETURNS INTEGER LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            &[
                (
                    "P_REF",
                    "REF(APP.TYPE_F)",
                    "method_reference_type",
                    "ext_scalar_param_decl",
                ),
                (
                    "P_XML",
                    "XML AS CLOB(2M)",
                    "procedure_external_xml_type",
                    "ext_scalar_param_decl",
                ),
                ("P_INT", "INTEGER", "data_type", "ext_scalar_param_decl"),
            ],
        ),
        (
            "CREATE PROCEDURE P_EXTERNAL_BIND(P_XML XML AS CLOB(2M),P_INT INTEGER,XML AS CLOB(2M)) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.proc' NO SQL;",
            &[
                (
                    "P_XML",
                    "XML AS CLOB(2M)",
                    "procedure_external_xml_type",
                    "proc_ext_param",
                ),
                ("P_INT", "INTEGER", "data_type", "proc_ext_param"),
            ],
        ),
    ];
    for (body, expected) in cases {
        let source = format!("-- α\r\n/* β */ {body}");
        let original = context();
        with_input(&source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "bindings.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{body}"
            );
            assert!(model.diagnostics().is_empty(), "{body}");
            let parameters = model
                .entities()
                .iter()
                .filter(|entity| entity.identity().label() == "Parameter")
                .collect::<Vec<_>>();
            assert_eq!(
                parameters.len(),
                expected.len(),
                "anonymous slots are not parameters"
            );
            assert!(
                model
                    .entities()
                    .iter()
                    .all(|entity| entity.identity().label() != "Type")
            );
            for &(name, text, kind, parent) in *expected {
                let parameter = parameters
                    .iter()
                    .find(|entity| entity.name() == name)
                    .unwrap();
                for contribution in parameter.contributions() {
                    let AntlrTypeObservation::Present { node_index, raw } =
                        &contribution.declared_type
                    else {
                        panic!("missing native declared type for {name}: {body}");
                    };
                    assert_eq!(raw.text(), text);
                    assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
                    let node = &parsed.observations().syntax.nodes[*node_index];
                    assert_eq!(node.kind, kind);
                    assert_eq!(node.span, Some(raw.start()..raw.end()));
                    assert_eq!(
                        parsed.observations().syntax.nodes[node.parent.unwrap()].kind,
                        parent
                    );
                    let roles = &contribution.type_roles;
                    assert_eq!(roles.len(), 1);
                    assert_eq!(roles[0].role, AntlrTypeRole::Declared);
                    assert!(roles[0].name_node_index.is_none());
                    let AntlrTypeObservation::Present {
                        node_index: role_node,
                        raw: role_raw,
                    } = &roles[0].observation
                    else {
                        panic!("missing native Declared role for {name}");
                    };
                    assert_eq!(role_node, node_index);
                    assert_eq!(role_raw.text().as_ptr(), raw.text().as_ptr());
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn db2_duplicate_conflicting_spans_nonexact_origins_and_actual_expiry() -> TestResult {
    let normal = "  - {data_key: tables, graph_label: Table, type: entity, pattern: '//create_table_statement/table_name'}\n";
    let bad = "  - {data_key: tables, graph_label: Table, type: entity, pattern: '//create_table_statement', captures: {name: [{xpath: '/table_name'}], span: [{nearest_descendant: 'column_name'}]}}\n";
    for queries in [
        format!("{normal}{normal}"),
        format!("{normal}{bad}"),
        format!("{bad}{normal}"),
    ] {
        let conflicting = queries.contains("span:");
        let draft = custom(&queries);
        let original = context();
        with_input("CREATE TABLE TABLE_F(C INTEGER);", &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "spans.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert_eq!(model.entities().len(), 1);
            assert_eq!(model.entities()[0].contributions().len(), 2);
            if conflicting {
                assert!(!model.entities()[0].conflicts().is_empty());
                assert!(matches!(
                    &model.code_projections()?[0],
                    AntlrProjection::Unavailable(_)
                ));
            } else {
                assert!(model.diagnostics().is_empty());
                assert!(model.entities()[0].conflicts().is_empty());
                assert!(matches!(
                    &model.code_projections()?[0],
                    AntlrProjection::Available(_)
                ));
            }
            Ok(())
        })?;
    }
    let draft = custom(normal);
    let original = context();
    let source = "CREATE TABLE TABLE_F(C INTEGER);";
    let name = source.find("TABLE_F").unwrap();
    let end = name + "TABLE_F".len();
    let pieces = [
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: 0,
                end: name as u64,
            },
            reason: graphit_ast::prepared_input::SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: name as u64,
                end: end as u64,
            },
            original: ByteSpan {
                start: name as u64,
                end: end as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
            reason: graphit_ast::prepared_input::SyntheticReason::Wrapper,
        },
    ];
    let removed = [
        ByteSpan {
            start: 0,
            end: name as u64,
        },
        ByteSpan {
            start: end as u64,
            end: source.len() as u64,
        },
    ];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &pieces,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedAntlrDocument::collect(
        &draft.profiles()[0],
        &input,
        &["src", "nonexact.db2"],
        limits(),
    )?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities().len(), 1);
    assert_eq!(model.entities()[0].name_raw().text(), "TABLE_F");
    assert!(matches!(
        model.entities()[0].extent(),
        AntlrEntityExtent::NonExact(graphit_ast::prepared_input::OriginalProjection::Compound(_))
    ));
    assert!(matches!(
        &model.code_projections()?[0],
        AntlrProjection::Unavailable(_)
    ));
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    let original = SnapshotContext::new(deadline, Cancellation::default());
    with_input(source, &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["src", "expiry.db2"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert!(
            matches!(model.code_projections(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn db2_method_cast_reference_roles_keep_original_and_genuine_owners() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    for body in [
        "CREATE METHOD METHOD_F() RETURNS REF(APP.TYPE_REF_F) CAST FROM REF(APP.TYPE_OTHER_F) FOR TYPE_F EXTERNAL NAME 'pkg.method';",
        "CREATE TYPE TYPE_F AS(ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F() RETURNS REF(APP.TYPE_REF_F) CAST FROM REF(APP.TYPE_OTHER_F) LANGUAGE SQL;",
        "ALTER TYPE TYPE_F ADD METHOD METHOD_F() RETURNS REF(APP.TYPE_REF_F) CAST FROM REF(APP.TYPE_OTHER_F) LANGUAGE SQL;",
    ] {
        let source = format!("-- α\r\n{body}");
        let original = context();
        with_input(&source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "cast-ref.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            assert!(
                model.diagnostics().is_empty(),
                "{body}: {:?}",
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            let method = model
                .entities()
                .iter()
                .find(|e| e.name() == "METHOD_F")
                .unwrap();
            let roles = &method.contributions()[0].type_roles;
            assert_eq!(roles.len(), 2);
            for (typed, role, expected, node_kind) in [
                (
                    &roles[0],
                    AntlrTypeRole::CastTarget,
                    "REF(APP.TYPE_REF_F)",
                    "method_cast_target_type",
                ),
                (
                    &roles[1],
                    AntlrTypeRole::CastSource,
                    "REF(APP.TYPE_OTHER_F)",
                    "method_cast_source_type",
                ),
            ] {
                assert_eq!(typed.role, role);
                let AntlrTypeObservation::Present { node_index, raw } = typed.observation else {
                    panic!("actual typed observation");
                };
                assert_eq!(raw.text(), expected);
                assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
                assert_eq!(
                    parsed.observations().syntax.nodes[node_index].kind,
                    node_kind
                );
                assert_eq!(
                    typed.name_node_index, None,
                    "CAST scalar slots have no paired return-column name"
                );
            }
            assert!(
                !model
                    .entities()
                    .iter()
                    .any(|e| e.name() == "TYPE_REF_F" || e.name() == "TYPE_OTHER_F")
            );
            if body.starts_with("CREATE TYPE") {
                assert!(method.namespace().iter().any(|n| n.name() == "TYPE_F"));
            }
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, AntlrProjection::Available(_)))
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn db2_module_type_variable_stock_subjects_original_namespaces_and_extents() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    for (definition, key, name, declared_type) in [
        (
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES CHECK(VALUE >= 0 AND VALUE <= 100)",
            "types",
            "T_F",
            Some("INTEGER"),
        ),
        (
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES NOT NULL CHECK(VALUE > 0)",
            "types",
            "T_F",
            Some("INTEGER"),
        ),
        (
            "TYPE T_F AS ANCHOR DATA TYPE TO APP.M_A.REF_F ARRAY[]",
            "types",
            "T_F",
            Some("ANCHOR DATA TYPE TO APP.M_A.REF_F"),
        ),
        (
            "TYPE T_F AS INTEGER ARRAY[ANCHOR DATA TYPE TO APP.TABLE_F.C]",
            "types",
            "T_F",
            Some("INTEGER"),
        ),
        (
            "TYPE T_F AS ROW ANCHOR ROW APP.TABLE_F",
            "types",
            "T_F",
            None,
        ),
        (
            "TYPE T_F AS ROW ANCHOR ROW OF SERVER_F.APP.TABLE_F",
            "types",
            "T_F",
            None,
        ),
        (
            "TYPE T_F AS ANCHOR DATA TYPE TO APP.TABLE_F.C",
            "types",
            "T_F",
            Some("ANCHOR DATA TYPE TO APP.TABLE_F.C"),
        ),
        (
            "VARIABLE V_F ANCHOR DATA TYPE TO APP.M_A.REF_F",
            "variables",
            "V_F",
            Some("ANCHOR DATA TYPE TO APP.M_A.REF_F"),
        ),
        (
            "VARIABLE V_F ANCHOR DATA TYPE TO APP.TABLE_F.C",
            "variables",
            "V_F",
            Some("ANCHOR DATA TYPE TO APP.TABLE_F.C"),
        ),
        (
            "VARIABLE V_F ANCHOR ROW OF SERVER_F.APP.TABLE_F",
            "variables",
            "V_F",
            Some("ANCHOR ROW OF SERVER_F.APP.TABLE_F"),
        ),
        (
            "TYPE T_F AS INTEGER WITH WEAK TYPE RULES",
            "types",
            "T_F",
            Some("INTEGER"),
        ),
        (
            "TYPE T_F AS INTEGER ARRAY[]",
            "types",
            "T_F",
            Some("INTEGER"),
        ),
        (
            "TYPE T_F AS APP.ELEMENT_F ARRAY[INTEGER]",
            "types",
            "T_F",
            Some("APP.ELEMENT_F"),
        ),
        ("TYPE T_F AS CURSOR", "types", "T_F", None),
        ("TYPE T_F AS APP.ROW_REF_F CURSOR", "types", "T_F", None),
        (
            "TYPE T_F AS ROW(FIELD_F INTEGER,OTHER_F APP.REF_F)",
            "types",
            "T_F",
            None,
        ),
        (
            "TYPE T_F AS ROW ANCHOR ROW OF APP.TABLE_F",
            "types",
            "T_F",
            None,
        ),
        (
            "TYPE T_F AS INTEGER WITH STRONG TYPE RULES",
            "types",
            "T_F",
            Some("INTEGER"),
        ),
        (
            "VARIABLE V_F APP.TYPE_REF_F",
            "variables",
            "V_F",
            Some("APP.TYPE_REF_F"),
        ),
        (
            "VARIABLE V_F INTEGER DEFAULT NULL",
            "variables",
            "V_F",
            Some("INTEGER"),
        ),
        (
            "VARIABLE V_F CURSOR DEFAULT (CURSOR FOR SELECT C FROM APP.TABLE_F)",
            "variables",
            "V_F",
            Some("CURSOR"),
        ),
        (
            "TYPE \"tipo.pão\" AS INTEGER ARRAY[]",
            "types",
            "tipo.pão",
            Some("INTEGER"),
        ),
        (
            "VARIABLE \"variável.pão\" INTEGER",
            "variables",
            "variável.pão",
            Some("INTEGER"),
        ),
    ] {
        for action in ["ADD", "PUBLISH"] {
            let source =
                format!("-- α\r\n/* β */ ALTER MODULE \"café\".\"MODULE\" {action} {definition};");
            let original = context();
            with_input(&source, &original, |input| {
                let parsed = ParsedAntlrDocument::collect(
                    &draft.profiles()[0],
                    input,
                    &["src", "module.db2"],
                    limits(),
                )?;
                let model = parsed.assemble()?;
                assert!(
                    parsed.observations().syntax.diagnostics.is_empty(),
                    "{source}"
                );
                assert!(
                    model.diagnostics().is_empty(),
                    "{source}: {:?}",
                    model
                        .diagnostics()
                        .iter()
                        .map(|d| &d.issue)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    model.entities().len(),
                    1,
                    "referenced types and MODULE are not declarations: {source}"
                );
                let entity = &model.entities()[0];
                assert_eq!(entity.data_key(), key);
                assert_eq!(entity.name(), name);
                assert_eq!(entity.role(), AntlrDeclarationRole::Declaration);
                assert_eq!(entity.qualified_name().unwrap().components().len(), 1);
                assert_eq!(
                    entity.name_raw().text().as_ptr(),
                    source[entity.name_raw().start()..].as_ptr()
                );
                assert_eq!(entity.namespace().len(), 1);
                let module = &entity.namespace()[0];
                assert_eq!(module.identity().label(), "Module");
                assert_eq!(module.name(), "MODULE");
                assert_eq!(module.anchor().text(), "\"café\".\"MODULE\"");
                assert_eq!(
                    module.anchor().text().as_ptr(),
                    source[module.anchor().start()..].as_ptr()
                );
                let AntlrEntityExtent::Exact(extent) = entity.extent() else {
                    panic!("actual native declaration extent");
                };
                assert_eq!(extent.range().text(), definition);
                assert_eq!(
                    extent.range().text().as_ptr(),
                    source[extent.range().start()..].as_ptr()
                );
                if let Some(expected) = declared_type {
                    let AntlrTypeObservation::Present { node_index, raw } =
                        entity.contributions()[0].declared_type
                    else {
                        panic!("actual written type: {source}");
                    };
                    assert_eq!(raw.text(), expected);
                    assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
                    assert_eq!(
                        parsed.observations().syntax.nodes[node_index].span,
                        Some(raw.start()..raw.end())
                    );
                }
                assert_eq!(
                    model.code_projections()?.len(),
                    usize::from(key == "types"),
                    "stock Variable embed policy is preserved"
                );
                Ok(())
            })?;
        }
    }
    let source = "-- α\r\nALTER MODULE APP.M_A ADD TYPE T_F AS INTEGER ARRAY[];\r\nALTER MODULE APP.M_B PUBLISH TYPE T_F AS INTEGER ARRAY[];\r\nALTER MODULE APP.M_A ADD VARIABLE V_F INTEGER;\r\nALTER MODULE APP.M_B ADD VARIABLE V_F INTEGER DEFAULT NULL;";
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["module.db2"], limits())?;
        let model = parsed.assemble()?;
        assert!(model.diagnostics().is_empty());
        assert_eq!(model.entities().len(), 4);
        let uids = model
            .entities()
            .iter()
            .map(|e| e.identity().uid().as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            uids.len(),
            4,
            "same spelling in distinct MODULE namespaces remains distinct"
        );
        assert_eq!(
            model
                .entities()
                .iter()
                .map(|e| e.namespace()[0].name())
                .collect::<Vec<_>>(),
            ["M_A", "M_B", "M_A", "M_B"]
        );
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, AntlrProjection::Available(_)))
        );
        Ok(())
    })?;

    for (query, expected_issue) in [
        (
            "  - {data_key: modules, graph_label: Module, type: entity, pattern: '//alter_module_statement/module_name'}\n",
            "unsupported",
        ),
        (
            "  - {data_key: modules, graph_label: Custom, type: entity, pattern: '//alter_module_statement', captures: {name: [{xpath: '/module_name'}]}}\n",
            "mismatch",
        ),
    ] {
        let reference_profile = custom(query);
        let source = "ALTER MODULE APP.M ADD TYPE T_F AS INTEGER ARRAY[];";
        let original = context();
        with_input(source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &reference_profile.profiles()[0],
                input,
                &["module.db2"],
                limits(),
            )?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            assert_eq!(
                parsed.observations().occurrences.len(),
                1,
                "actual custom capture"
            );
            let model = parsed.assemble()?;
            assert!(
                model.entities().is_empty(),
                "Entity metadata cannot turn a referenced Module into a canonical declaration"
            );
            assert!(
                model.diagnostics().iter().any(|d| match expected_issue {
                    "unsupported" => matches!(d.issue, AntlrIssue::UnsupportedDeclaration),
                    "mismatch" => matches!(d.issue, AntlrIssue::NameKindMismatch),
                    _ => unreachable!(),
                }),
                "actual diagnostics: {:?}",
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            Ok(())
        })?;
    }

    Ok(())
}

#[test]
fn db2_module_type_variable_recovery_preserves_diagnostics() -> TestResult {
    let source = "-- α\r\nALTER MODULE APP.M ADD TYPE T_F AS INTEGER ARRAY[;";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["broken.db2"], limits())?;
        assert!(!parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        assert!(!model.diagnostics().is_empty());
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, AntlrProjection::Unavailable(_)))
        );
        Ok(())
    })
}

#[test]
fn db2_remaining_function_domains_preserve_typed_roles_and_original() -> TestResult {
    use AntlrTypeRole::{CastSource, CastTarget, Return, ReturnColumn};
    type ExpectedRole<'a> = (AntlrTypeRole, &'a str, Option<&'a str>);
    type ExpectedParameter<'a> = (&'a str, &'a str);
    type Case<'a> = (&'a str, &'a [ExpectedParameter<'a>], &'a [ExpectedRole<'a>]);
    let cases: &[Case<'_>] = &[
        (
            "CREATE FUNCTION F_CAST() RETURNS VARCHAR() CAST FROM DATE LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.fn';",
            &[],
            &[(CastTarget, "VARCHAR()", None), (CastSource, "DATE", None)],
        ),
        (
            "CREATE FUNCTION F_TABLE(P_REF REF(APP.TYPE_F),XML AS CLOB(2M),P_XML XML AS CLOB(2048),P_LOB CLOB(2M) DEFAULT NULL AS LOCATOR) RETURNS TABLE(C_DATE DATE,C_LOB CLOB(2M) AS LOCATOR,C_TYPE APP.TYPE_F,C_REF REF(APP.TYPE_F),C_XML XML AS CLOB(2048)) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;",
            &[
                ("P_REF", "REF(APP.TYPE_F)"),
                ("P_XML", "XML AS CLOB(2048)"),
                ("P_LOB", "CLOB(2M)"),
            ],
            &[
                (ReturnColumn, "DATE", Some("C_DATE")),
                (ReturnColumn, "CLOB(2M)", Some("C_LOB")),
                (ReturnColumn, "APP.TYPE_F", Some("C_TYPE")),
                (ReturnColumn, "REF(APP.TYPE_F)", Some("C_REF")),
                (ReturnColumn, "XML AS CLOB(2048)", Some("C_XML")),
            ],
        ),
        (
            "CREATE FUNCTION F_GENERIC() RETURNS GENERIC TABLE LANGUAGE JAVA PARAMETER STYLE DB2GENERAL EXTERNAL NAME 'pkg.fn';",
            &[],
            &[],
        ),
        (
            "CREATE FUNCTION F_OLE(P_CMD VARCHAR(40) DEFAULT 'cmd',GRAPHIC(10)) RETURNS TABLE(C_DATE DATE,C_DEC DECIMAL(12,2),C_LOB CLOB(2M),C_TYPE APP.TYPE_F) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
            &[("P_CMD", "VARCHAR(40)")],
            &[
                (ReturnColumn, "DATE", Some("C_DATE")),
                (ReturnColumn, "DECIMAL(12,2)", Some("C_DEC")),
                (ReturnColumn, "CLOB(2M)", Some("C_LOB")),
                (ReturnColumn, "APP.TYPE_F", Some("C_TYPE")),
            ],
        ),
        (
            "CREATE FUNCTION F_SOURCE(P_CHAR CHAR(),DECIMAL() DEFAULT 1,P_TS TIMESTAMP()) RETURNS VARCHAR() SOURCE APP.F_TARGET(DECIMAL(),TIMESTAMP(),VARCHAR());",
            &[("P_CHAR", "CHAR()"), ("P_TS", "TIMESTAMP()")],
            &[(Return, "VARCHAR()", None)],
        ),
        (
            "CREATE FUNCTION F_TEMPLATE(P_CHAR CHAR(),DECIMAL(),P_TS TIMESTAMP()) RETURNS VARCHAR(40) AS TEMPLATE;",
            &[("P_CHAR", "CHAR()"), ("P_TS", "TIMESTAMP()")],
            &[(Return, "VARCHAR(40)", None)],
        ),
    ];
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    for &(body, expected_parameters, expected_roles) in cases {
        let source = format!("-- α\r\n/* β /* γ */ δ */ {body}");
        let original = context();
        with_input(&source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["src", "remaining.db2"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{body}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            assert!(model.diagnostics().is_empty(), "{body}");
            let functions = model
                .entities()
                .iter()
                .filter(|e| e.data_key() == "functions")
                .collect::<Vec<_>>();
            assert_eq!(functions.len(), 1, "SOURCE is a reference: {body}");
            assert!(
                model
                    .entities()
                    .iter()
                    .all(|e| e.identity().label() != "Type")
            );
            let parameters = model
                .entities()
                .iter()
                .filter(|e| e.identity().label() == "Parameter")
                .collect::<Vec<_>>();
            assert_eq!(
                parameters.len(),
                expected_parameters.len(),
                "anonymous/SOURCE slots: {body}"
            );
            for &(name, text) in expected_parameters {
                let entity = parameters.iter().find(|e| e.name() == name).unwrap();
                let contribution = &entity.contributions()[0];
                let AntlrTypeObservation::Present { node_index, raw } = &contribution.declared_type
                else {
                    panic!("declared type {body}: {name}");
                };
                assert_eq!(raw.text(), text);
                assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
                assert_eq!(
                    parsed.observations().syntax.nodes[*node_index].span,
                    Some(raw.start()..raw.end())
                );
                assert_eq!(contribution.type_roles.len(), 1);
                assert_eq!(contribution.type_roles[0].role, AntlrTypeRole::Declared);
            }
            let roles = &functions[0].contributions()[0].type_roles;
            assert_eq!(roles.len(), expected_roles.len(), "{body}");
            let mut previous = None;
            for (observed, &(role, text, name)) in roles.iter().zip(expected_roles) {
                assert_eq!(observed.role, role);
                let AntlrTypeObservation::Present { node_index, raw } = &observed.observation
                else {
                    panic!("return type: {body}");
                };
                assert_eq!(raw.text(), text);
                assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
                assert_eq!(
                    parsed.observations().syntax.nodes[*node_index].span,
                    Some(raw.start()..raw.end())
                );
                if let Some(previous) = previous {
                    assert!(previous < *node_index, "native order");
                }
                previous = Some(*node_index);
                assert_eq!(
                    observed.name_node_index.map(|i| source
                        .get(parsed.observations().syntax.nodes[i].span.clone().unwrap())
                        .unwrap()),
                    name
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}
