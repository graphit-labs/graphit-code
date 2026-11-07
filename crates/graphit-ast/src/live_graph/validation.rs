//! Validate only supplied candidate identities in the current FILE publication.
use super::*;
use crate::entity_search::Entity;
use graphit_data::graph_writer::visit_node_shard;
impl LiveGraph {
    pub fn validate_entities(
        &self,
        path: &str,
        expected: &LiveFileObservation,
        entities: &[&Entity],
        original: &Arc<SnapshotContext>,
    ) -> io::Result<()> {
        original.check()?;
        if path != expected.path {
            return Err(invalid("candidate validation has a different FILE path"));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        let record = state
            .catalog
            .files
            .get(path)
            .filter(|record| &record.observation == expected)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Interrupted,
                    "FILE changed before candidate identity validation",
                )
            })?;
        let sha = expected
            .source_sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut keys = BTreeMap::new();
        for entity in entities {
            original.check()?;
            if entity.path != path
                || entity.raw_version != expected.raw_version
                || entity.source_sha256 != sha
                || entity.entity_key.is_empty()
                || entity.uid.is_empty()
                || entity.data_key.is_empty()
            {
                return Err(invalid("candidate identity differs from its current FILE"));
            }
            let identity = (
                entity.kind.as_str(),
                entity.uid.as_str(),
                entity.data_key.as_str(),
            );
            if keys
                .insert(entity.entity_key.as_str(), identity)
                .is_some_and(|previous| previous != identity)
            {
                return Err(invalid(
                    "conflicting candidate identities for one entity key",
                ));
            }
        }
        for part in &record.nodes {
            if !keys
                .values()
                .any(|(label, _, _)| *label == part.shard.label)
            {
                continue;
            }
            visit_node_shard(
                &portable::owned_path(&self.directory, &part.shard.file)?,
                &part.shard.label,
                part.row_offset
                    ..part
                        .row_offset
                        .checked_add(part.shard.rows)
                        .ok_or_else(|| invalid("FILE physical range overflow"))?,
                original,
                |node| {
                    if let Some((label, uid, data_key)) = keys.remove(node.key.as_str())
                        && (node.label != label
                            || node_text(&node, "uid") != Some(uid)
                            || node_text(&node, "data_key") != Some(data_key))
                    {
                        return Err(invalid("candidate identity differs from its native entity"));
                    }
                    Ok(())
                },
            )?;
        }
        if !keys.is_empty() {
            return Err(invalid(
                "candidate is absent from its native FILE ownership",
            ));
        }
        original.check()
    }
}
