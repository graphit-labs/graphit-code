//! Official SCIP records share the atomic FILE catalogue and remain in Parquet.
use super::*;
use ::scip::types::{Document, SymbolInformation};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use graphit_data::graph_writer::{visit_fact_rows, write_fact_rows};
use protobuf::Message;

pub(super) fn validate_document(
    input: &LiveFileInput<'_>,
    original: &SnapshotContext,
) -> io::Result<()> {
    let Some(bytes) = input.scip_document else {
        return Ok(());
    };
    original.check()?;
    let document = Document::parse_from_bytes(bytes).map_err(io::Error::other)?;
    if document.relative_path != input.path
        || (!document.text.is_empty()
            && <[u8; 32]>::from(Sha256::digest(document.text.as_bytes())) != input.source_sha256)
    {
        return Err(invalid(
            "published SCIP document belongs to another FILE/Source",
        ));
    }
    Ok(())
}
impl LiveGraph {
    pub fn replace_scip_externals(
        &self,
        family: &str,
        symbols: impl IntoIterator<Item = io::Result<SymbolInformation>>,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<u64> {
        if family.is_empty() || family.contains('\0') {
            return Err(invalid("invalid SCIP compiler family"));
        }
        original.check()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        let directory = tempfile::Builder::new()
            .prefix("external-")
            .tempdir_in(self.directory.join("shards"))?
            .keep();
        let staging = tempfile::tempdir()?;
        let names = rusqlite::Connection::open(staging.path().join("symbols.sqlite"))
            .map_err(io::Error::other)?;
        names
            .execute_batch("PRAGMA temp_store=FILE; CREATE TABLE symbols(name TEXT PRIMARY KEY)")
            .map_err(io::Error::other)?;
        let rows = symbols.into_iter().map(|symbol| {
            original.check()?;
            let symbol = symbol?;
            if symbol.symbol.is_empty() || symbol.symbol.contains('\0') {
                return Err(invalid("invalid or duplicate SCIP external symbol"));
            }
            names
                .execute("INSERT INTO symbols VALUES (?1)", [&symbol.symbol])
                .map_err(io::Error::other)?;
            encoded(&(
                symbol.symbol.clone(),
                STANDARD.encode(symbol.write_to_bytes().map_err(io::Error::other)?),
            ))
        });
        write_fact_rows(&directory.join("externals.parquet"), rows, original)?;
        let mut next = state.catalog.clone();
        next.schema.revision = next
            .schema
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("graph revision overflow"))?;
        next.externals.insert(
            family.into(),
            format!(
                "shards/{}/externals.parquet",
                directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| invalid("invalid external shard directory"))?
            ),
        );
        portable::publish(&self.directory, &next, original)?;
        state.catalog = next;
        original.check().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "SCIP family committed at graph revision {}; {error}",
                    state.catalog.schema.revision
                ),
            )
        })?;
        Ok(state.catalog.schema.revision)
    }
    pub fn visit_scip_documents(
        &self,
        original: &Arc<SnapshotContext>,
        mut visitor: impl FnMut(Document) -> io::Result<()>,
    ) -> io::Result<()> {
        original.check()?;
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        for (path, record) in &state.catalog.files {
            original.check()?;
            if !record.scip_document {
                continue;
            }
            let payload =
                portable::read_facts(&self.directory, &record.facts, record.facts_row, original)?
                    .scip_document
                    .ok_or_else(|| invalid("official SCIP document disappeared"))?;
            let document =
                Document::parse_from_bytes(&STANDARD.decode(payload).map_err(io::Error::other)?)
                    .map_err(io::Error::other)?;
            if document.relative_path != *path {
                return Err(invalid("native SCIP document path differs from FILE"));
            }
            visitor(document)?;
        }
        original.check()
    }
    pub fn visit_scip_externals(
        &self,
        original: &Arc<SnapshotContext>,
        mut visitor: impl FnMut(SymbolInformation) -> io::Result<()>,
    ) -> io::Result<()> {
        original.check()?;
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        for file in state.catalog.externals.values() {
            visit_fact_rows(
                &portable::owned_path(&self.directory, file)?,
                ..,
                original,
                |payload| {
                    let (name, payload): (String, String) = decoded(payload)?;
                    let symbol = SymbolInformation::parse_from_bytes(
                        &STANDARD.decode(payload).map_err(io::Error::other)?,
                    )
                    .map_err(io::Error::other)?;
                    if symbol.symbol != name {
                        return Err(invalid("native SCIP external identity differs"));
                    }
                    visitor(symbol)
                },
            )?;
        }
        original.check()
    }
}
