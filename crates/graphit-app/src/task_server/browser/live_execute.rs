//! Current server configuration for authenticated Live work.
use super::*;
use graphit_core::{
    config::ConfigSnapshot,
    file_io::SnapshotContext,
    project_paths::{InstallationPaths, ProjectPaths},
};

impl TaskServer {
    /// Call after admitting the selected storage scope; configuration is data, not a grant.
    pub(crate) fn live_configuration(
        &self,
        project: Option<&ProjectPaths>,
        original: &Arc<SnapshotContext>,
    ) -> std::io::Result<(
        InstallationPaths,
        ConfigSnapshot,
        graphit_runtime::instructions::InstructionLimits,
    )> {
        original.check()?;
        let (installation, policy) = self
            .0
            .project_configuration
            .read()
            .map_err(|_| std::io::Error::other("PROJECT configuration lock poisoned"))?
            .as_ref()
            .cloned()
            .ok_or(std::io::ErrorKind::Unsupported)?;
        let configuration = policy.selected_configuration(&installation, project, original)?;
        original.check()?;
        Ok((installation, configuration, policy.instruction_limits()))
    }
}
