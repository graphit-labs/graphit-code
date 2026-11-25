//! PROJECT document selection over ordinary filesystem paths.
mod selection;
pub use selection::{
    ProjectIgnoreCriteria, ProjectSelectionCriteria, ProjectSelectionLimits,
    selected_project_changed_paths, selected_project_paths,
};
pub(super) use selection::{
    SelectionCoverage, capture, capture_changes_metadata, capture_metadata,
};
