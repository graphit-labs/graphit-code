//! Operational PROJECT membership of the live daemon; it never grants access.
use graphit_core::{
    project_identity::ProjectId, project_paths::ProjectPaths, version_context::VersionContext,
};
use std::{
    io,
    sync::{Arc, RwLock},
};

type Selection = (ProjectPaths, ProjectId, VersionContext);

/// Display metadata only. Callers pass their already authorized context subset;
/// the immutable identity remains the only key used by requests and selections.
#[derive(serde::Serialize)]
pub(crate) struct ProjectName {
    project_id: ProjectId,
    name: String,
}

pub(crate) fn project_names<'a>(
    projects: impl IntoIterator<Item = &'a Selection>,
    request: &graphit_core::file_io::SnapshotContext,
) -> io::Result<Vec<ProjectName>> {
    let mut names = std::collections::BTreeMap::new();
    for (paths, id, _) in projects {
        request.check()?;
        if names.contains_key(id) {
            continue;
        }
        let Some(identity) =
            graphit_core::project_identity::ProjectIdentityStore::at(paths).load()?
        else {
            continue;
        };
        if identity.id() != *id {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        names.insert(*id, identity.metadata().name.clone());
    }
    request.check()?;
    Ok(names
        .into_iter()
        .map(|(project_id, name)| ProjectName { project_id, name })
        .collect())
}

/// Resolve selectors only within captured selections. A root identifies its
/// PROJECT; an explicit version may select another checkout of that same
/// PROJECT and storage namespace. The returned tuple still needs fresh admission.
pub(crate) fn select_project_version(
    projects: &[Selection],
    root: &std::path::Path,
    requested_version: Option<&str>,
) -> Result<usize, &'static str> {
    let mut roots = projects.iter().filter(|(paths, _, _)| paths.root() == root);
    let (origin, id, _) = roots.next().ok_or("selection_unavailable")?;
    if roots.any(|(_, other, _)| other != id) {
        return Err("selection_ambiguous");
    }
    let mut exact = projects
        .iter()
        .enumerate()
        .filter(|(_, (paths, _, version))| {
            paths.root() == root && requested_version.is_none_or(|v| version.version() == v)
        });
    if let Some((index, _)) = exact.next() {
        return if exact.next().is_some() {
            Err("selection_ambiguous")
        } else {
            Ok(index)
        };
    }
    let requested_version = requested_version.ok_or("selection_unavailable")?;
    let mut retained = projects
        .iter()
        .enumerate()
        .filter(|(_, (paths, candidate, version))| {
            candidate == id
                && paths.brand() == origin.brand()
                && paths.global_root() == origin.global_root()
                && version.version() == requested_version
        });
    let (index, _) = retained.next().ok_or("selection_unavailable")?;
    if retained.next().is_some() {
        return Err("selection_ambiguous");
    }
    Ok(index)
}

/// Requests clone one metadata snapshot; no reader or grant lives in this state.
pub(crate) struct LiveProjectSelections(RwLock<Arc<LiveProjectSnapshot>>);

pub(crate) struct LiveProjectSnapshot {
    pub(crate) projects: Arc<Vec<Selection>>,
    pub(crate) current: Option<usize>,
}

impl LiveProjectSelections {
    pub(crate) fn new(projects: Arc<Vec<Selection>>, current: usize) -> Self {
        let current = projects.get(current).map(|_| current);
        Self(RwLock::new(Arc::new(LiveProjectSnapshot {
            projects,
            current,
        })))
    }

    pub(crate) fn snapshot(&self) -> io::Result<Arc<LiveProjectSnapshot>> {
        self.0
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .map_err(|_| io::Error::other("live PROJECT selections poisoned"))
    }

    pub(crate) fn replace(&self, projects: Arc<Vec<Selection>>, current: usize) -> io::Result<()> {
        if !projects.is_empty() && projects.get(current).is_none() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let current = projects.get(current).map(|_| current);
        let mut active = self
            .0
            .write()
            .map_err(|_| io::Error::other("live PROJECT selections poisoned"))?;
        *active = Arc::new(LiveProjectSnapshot { projects, current });
        Ok(())
    }

    pub(crate) fn is_active(
        &self,
        paths: &ProjectPaths,
        id: ProjectId,
        version: &VersionContext,
    ) -> io::Result<bool> {
        let active = self.snapshot()?;
        Ok(active
            .projects
            .iter()
            .any(|(candidate, identity, selected)| {
                candidate.root() == paths.root() && *identity == id && selected == version
            }))
    }

    pub(crate) fn retire_checkout(&self, id: ProjectId, path: &std::path::Path) -> io::Result<()> {
        let mut active = self
            .0
            .write()
            .map_err(|_| io::Error::other("live PROJECT selections poisoned"))?;
        let selected = active.current.and_then(|index| active.projects.get(index));
        let projects: Vec<_> = active
            .projects
            .iter()
            .filter(|(paths, identity, _)| *identity != id || paths.root() != path)
            .cloned()
            .collect();
        let current = selected
            .and_then(|(paths, id, version)| {
                projects.iter().position(|(candidate, identity, selected)| {
                    candidate.root() == paths.root() && identity == id && selected == version
                })
            })
            .or_else(|| (!projects.is_empty()).then_some(0));
        *active = Arc::new(LiveProjectSnapshot {
            projects: Arc::new(projects),
            current,
        });
        Ok(())
    }
}
