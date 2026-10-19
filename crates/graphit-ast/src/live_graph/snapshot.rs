//! Export streams one committed FILE at a time into the ordinary artifact writer.
use super::*;
use graphit_data::graph_writer::IcebugWriter;
impl LiveGraph {
    pub fn write_snapshot(
        &self,
        mut writer: IcebugWriter,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<IcebugWriter> {
        original.check()?;
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        for record in state.catalog.files.values() {
            original.check()?;
            let nodes = portable::read_nodes(&self.directory, record, original)?;
            let facts = portable::read_facts(&self.directory, &record.facts, original)?;
            writer = writer.push_file_ref(&nodes, &facts.edges)?;
        }
        original.check()?;
        writer.retain_resolved_edges()
    }
}
