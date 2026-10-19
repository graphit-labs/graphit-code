//! Compiler documents and original parser batches staged for per-FILE publication.
use super::*;
use ::scip::types::{Document, SymbolInformation};
use graphit_ast::{
    scip_stream::IndexRecord,
    scip_syntax::{AstScipSyntaxInput, produce_scip_syntax_document},
    semantic_graph::{AstGraphParts, scip_document::apply_scip_document},
};
use protobuf::Message;

pub(in crate::foreground_ast_sources) struct ScipIndex {
    project_id: String,
    project_version: String,
}

impl ScipIndex {
    pub(in crate::foreground_ast_sources) fn prepared_parser_file(
        &self,
        writer: &IcebugWriter,
        path: &str,
        original: &SnapshotContext,
    ) -> io::Result<String> {
        serde_json::to_string(&self.staged_file(writer, path, original)?).map_err(err)
    }

    pub(in crate::foreground_ast_sources) fn restore_parser_file(
        &self,
        writer: &IcebugWriter,
        observation: &graphit_ast::live_graph::LiveFileObservation,
        payload: &str,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        let batches: Vec<(
            AstSemanticGraphBatch,
            Option<graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
        )> = serde_json::from_str(payload).map_err(err)?;
        if batches.is_empty() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for (batch, oracle) in batches {
            original.check()?;
            if batch.association().path() != observation.path
                || batch.association().source_sha256() != &observation.source_sha256
                || !matches!(batch.file_observation().properties().get("raw_version"),
                    Some(AstGraphValue::String(version)) if version.parse::<u64>().ok() == Some(observation.raw_version))
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
            self.stage(writer, &batch, oracle.as_ref(), original)?;
        }
        original.check()
    }

    pub(in crate::foreground_ast_sources) fn stage(
        &self,
        writer: &IcebugWriter,
        batch: &AstSemanticGraphBatch,
        oracle: Option<&graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
        original: &SnapshotContext,
    ) -> io::Result<()> {
        original.check()?;
        writer
            .staging_database()?
            .execute(
                "INSERT INTO canonical_ast_parts(path,batch,oracle) VALUES(?1,?2,?3)",
                (
                    batch.association().path(),
                    serde_json::to_string(batch)?,
                    oracle.map(serde_json::to_string).transpose()?,
                ),
            )
            .map_err(err)?;
        original.check()
    }

    pub(in crate::foreground_ast_sources) fn staged_paths(
        &self,
        writer: &IcebugWriter,
        original: &SnapshotContext,
    ) -> io::Result<Vec<String>> {
        original.check()?;
        let mut query = writer
            .staging_database()?
            .prepare("SELECT DISTINCT path FROM canonical_ast_parts ORDER BY path")
            .map_err(err)?;
        let paths = query
            .query_map([], |row| row.get(0))
            .map_err(err)?
            .collect::<Result<Vec<String>, _>>()
            .map_err(err)?;
        original.check()?;
        Ok(paths)
    }

    pub(in crate::foreground_ast_sources) fn staged_file(
        &self,
        writer: &IcebugWriter,
        path: &str,
        original: &SnapshotContext,
    ) -> io::Result<
        Vec<(
            AstSemanticGraphBatch,
            Option<graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
        )>,
    > {
        original.check()?;
        let mut query = writer
            .staging_database()?
            .prepare("SELECT batch,oracle FROM canonical_ast_parts WHERE path=?1 ORDER BY id")
            .map_err(err)?;
        let mut rows = query.query([path]).map_err(err)?;
        let mut batches = Vec::new();
        while let Some(row) = rows.next().map_err(err)? {
            original.check()?;
            let batch: String = row.get(0).map_err(err)?;
            let oracle: Option<String> = row.get(1).map_err(err)?;
            batches.push((
                serde_json::from_str(&batch)?,
                oracle.map(|v| serde_json::from_str(&v)).transpose()?,
            ));
        }
        Ok(batches)
    }

    pub(in crate::foreground_ast_sources) fn syntax(
        &self,
        parts: AstGraphParts,
        text: &str,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<(
        graphit_ast::semantic_graph::scip_document::AstScipGraphDocument,
        Vec<u8>,
    )> {
        let document = produce_scip_syntax_document(
            AstScipSyntaxInput {
                graph: &parts,
                source: text,
                project_id: &self.project_id,
                project_version: &self.project_version,
            },
            original,
        )
        .map_err(err)?;
        let bytes = document.write_to_bytes().map_err(err)?;
        Ok((
            apply_scip_document(parts, &document, text, original).map_err(err)?,
            bytes,
        ))
    }

    pub(in crate::foreground_ast_sources) fn publish_externals(
        &self,
        writer: &IcebugWriter,
        family: &str,
        live: &graphit_ast::live_graph::LiveGraph,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        original.check()?;
        let mut query = writer
            .staging_database()?
            .prepare("SELECT payload FROM canonical_scip_external WHERE family=?1 ORDER BY symbol")
            .map_err(err)?;
        let rows = query
            .query_map([family], |row| row.get::<_, Vec<u8>>(0))
            .map_err(err)?;
        let symbols = rows.map(|row| {
            original.check()?;
            SymbolInformation::parse_from_bytes(&row.map_err(err)?).map_err(err)
        });
        live.replace_scip_externals(family, symbols, original)?;
        original.check()
    }

    pub(in crate::foreground_ast_sources) fn compiler_document(
        &self,
        writer: &IcebugWriter,
        path: &str,
        original: &SnapshotContext,
    ) -> io::Result<Option<Document>> {
        original.check()?;
        let mut query = writer.staging_database()?.prepare(
            "SELECT compiler FROM canonical_scip_documents WHERE path=?1 AND compiler IS NOT NULL",
        ).map_err(err)?;
        let mut rows = query.query([path]).map_err(err)?;
        let document = rows
            .next()
            .map_err(err)?
            .map(|row| {
                let bytes: Vec<u8> = row.get(0).map_err(err)?;
                Document::parse_from_bytes(&bytes).map_err(err)
            })
            .transpose()?;
        original.check()?;
        Ok(document)
    }

    pub(in crate::foreground_ast_sources) fn new(
        writer: &IcebugWriter,
        selected: &graphit_daemon::supervisor::ProjectContext,
    ) -> io::Result<Self> {
        writer.staging_database()?.execute_batch(
            "CREATE TABLE canonical_ast_parts (id INTEGER PRIMARY KEY,path TEXT NOT NULL,batch TEXT NOT NULL,oracle TEXT);
             CREATE INDEX canonical_ast_path ON canonical_ast_parts(path,id);
             CREATE TABLE canonical_scip_documents (
               path TEXT PRIMARY KEY, family TEXT, source_sha BLOB, compiler BLOB);
             CREATE TABLE canonical_scip_external (
               family TEXT NOT NULL,symbol TEXT NOT NULL,payload BLOB NOT NULL,
               PRIMARY KEY(family,symbol));"
        ).map_err(err)?;
        Ok(Self {
            project_id: selected.id.to_string(),
            project_version: selected.version.version().to_owned(),
        })
    }

    pub(in crate::foreground_ast_sources) fn import_compiler(
        &self,
        writer: &IcebugWriter,
        family: &str,
        output: &super::super::scip::ScipOutput,
        inputs: &super::super::scip::CapturedInputs,
        policy: &HubResourcePolicy,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        let db = writer.staging_database()?;
        db.execute_batch("SAVEPOINT compiler_scip").map_err(err)?;
        let result = output.visit(policy, original, |record| {
            original.check()?;
            match record {
                IndexRecord::Document(document) => {
                    // Compiler-generated or ignored paths do not enlarge the admitted Raw corpus.
                    let Some(hash) = inputs.source_sha256(&document.relative_path, original)? else {
                        return Ok(());
                    };
                    db.execute(
                        "INSERT INTO canonical_scip_documents(path,family,source_sha,compiler) VALUES(?1,?2,?3,?4)",
                        (&document.relative_path, family, hash.as_slice(), document.write_to_bytes().map_err(graphit_ast::scip::AstScipError::Protobuf)?),
                    ).map_err(|error| graphit_ast::scip::AstScipError::Operation(err(error)))?;
                }
                IndexRecord::ExternalSymbol(symbol) => {
                    db.execute(
                        "INSERT INTO canonical_scip_external(family,symbol,payload) VALUES(?1,?2,?3)",
                        (family, &symbol.symbol, symbol.write_to_bytes().map_err(graphit_ast::scip::AstScipError::Protobuf)?),
                    ).map_err(|error| graphit_ast::scip::AstScipError::Operation(err(error)))?;
                }
            }
            Ok(())
        });
        match result {
            Ok(_) => db.execute_batch("RELEASE compiler_scip").map_err(err)?,
            Err(error) => {
                db.execute_batch("ROLLBACK TO compiler_scip; RELEASE compiler_scip")
                    .map_err(err)?;
                original.check()?;
                return Err(error);
            }
        }
        original.check()
    }

    pub(in crate::foreground_ast_sources) fn consume(
        &self,
        writer: &IcebugWriter,
        parts: AstGraphParts,
        text: &str,
        nn_source: Option<(
            graphit_ast::native_nn::NativeNnSource<'_>,
            graphit_ast::symbols::IdentityLimits,
        )>,
        oracle_rows: Option<&graphit_ast::semantic_graph::oracle::AstOracleGraphRows>,
        live: &mut super::super::progressive::FilePublication,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<AstGraphParts> {
        original.check()?;
        let db = writer.staging_database()?;
        let compiler = {
            let mut query = db.prepare("SELECT source_sha,compiler FROM canonical_scip_documents WHERE path=?1 AND compiler IS NOT NULL").map_err(err)?;
            let mut rows = query.query([parts.association.path()]).map_err(err)?;
            rows.next()
                .map_err(err)?
                .map(|row| {
                    Ok::<_, io::Error>((
                        row.get::<_, Vec<u8>>(0).map_err(err)?,
                        row.get::<_, Vec<u8>>(1).map_err(err)?,
                    ))
                })
                .transpose()?
        };
        let syntax = || {
            produce_scip_syntax_document(
                AstScipSyntaxInput {
                    graph: &parts,
                    source: text,
                    project_id: &self.project_id,
                    project_version: &self.project_version,
                },
                original,
            )
            .map_err(err)
        };
        let document = match compiler {
            Some((hash, bytes)) => {
                if hash.as_slice() != parts.association.source_sha256() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "SCIP compiler source no longer matches selected Raw",
                    ));
                }
                let document = Document::parse_from_bytes(&bytes).map_err(err)?;
                if document.position_encoding.value() == 0 {
                    if matches!(parts.file_observation.properties().get("grammar"),
                        Some(AstGraphValue::String(grammar)) if grammar == "scip-compiler")
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "SCIP compiler omitted column encoding and no syntax grammar is available",
                        ));
                    }
                    eprintln!(
                        "AST SCIP {} has ambiguous column encoding; using syntax SCIP",
                        document.relative_path
                    );
                    syntax()?
                } else {
                    document
                }
            }
            None => syntax()?,
        };
        let consumed = apply_scip_document(parts, &document, text, original).map_err(err)?;
        if let Some((source, limits)) = nn_source.filter(|_| consumed.parts.nodes.iter().any(|node| {
            matches!(node.properties().get("data_key"),Some(AstGraphValue::String(key)) if key=="scip_definitions")
        })) {
            let prepared = graphit_ast::native_nn::prepare_native_nn_scip(
                &consumed, &document, source, limits, original,
            )
            .map_err(err)?;
            if prepared.num_rows() != 0 {
                live.projections.push(prepared);
            }
        }
        let bytes = document.write_to_bytes().map_err(err)?;
        original.check()?;
        live.scip_document = Some(bytes);
        live.definitions.extend(consumed.definitions);
        let oracle_outcomes = oracle_rows
            .into_iter()
            .flat_map(|rows| rows.references())
            .map(|reference| {
                let outcome = reference.outcome();
                (
                    Some(outcome.relation_index()),
                    outcome.source_index(),
                    outcome.capture_index(),
                )
            })
            .collect::<BTreeSet<_>>();
        live.references
            .extend(consumed.references.into_iter().filter(|reference| {
                !oracle_outcomes.contains(&(
                    reference.relation_index,
                    reference.source_index,
                    reference.capture_index,
                ))
            }));
        Ok(consumed.parts)
    }
}
