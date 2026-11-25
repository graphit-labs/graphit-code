//! Pending compiler payloads remain separate from the mounted public graph.
use super::*;
impl LiveGraph {
    pub fn pending_compiler_file(
        &self,
        path: &str,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Option<(LiveFileObservation, String)>> {
        original.check()?;
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        let Some(record) = state
            .catalog
            .files
            .get(path)
            .filter(|record| record.pending_compiler)
        else {
            return Ok(None);
        };
        let facts =
            portable::read_facts(&self.directory, &record.facts, record.facts_row, original)?;
        Ok(Some((
            record.observation.clone(),
            facts
                .pending_compiler
                .ok_or_else(|| invalid("pending compiler payload disappeared"))?,
        )))
    }
    pub fn pending_compiler_files(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Vec<LiveFileObservation>> {
        original.check()?;
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        state
            .catalog
            .files
            .values()
            .filter(|record| record.pending_compiler)
            .map(|record| {
                original.check()?;
                Ok(record.observation.clone())
            })
            .collect()
    }
}
