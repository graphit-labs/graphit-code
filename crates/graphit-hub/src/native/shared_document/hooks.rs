//! Native command hooks and Kimi project references use the same document transaction.
use super::*;
use crate::native::{document::HookMatch, hooks::NativeHookPlan};
use graphit_core::project_identity::ProjectId;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandHookEntry {
    identifier: String,
    owner: Owner,
    binding: NativeBinding,
    event: String,
    path: Vec<String>,
    group: Option<TupleFields>,
    fields: TupleFields,
    projects: Vec<ProjectId>,
}
impl SharedDocumentWriter<'_, '_> {
    /// Let the global caller retain shared MCP configuration until hook references end.
    pub fn command_hook_has_references(&mut self, plan: &NativeHookPlan) -> io::Result<bool> {
        self.recover()?;
        self.request.remaining()?;
        if plan.binding != NativeBinding::Kimi
            || !matches!(&self.owner.scope, StoreOwner::Installation { .. })
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        match self.persistence.state.entries.iter().find(|entry| {
            matches!(entry, Entry::CommandHook(e) if e.binding == plan.binding && e.identifier == plan.identifier)
        }) {
            Some(Entry::CommandHook(entry)) if entry.owner == self.owner => Ok(!entry.projects.is_empty()),
            Some(_) => Err(conflict()),
            None => Ok(false),
        }
    }
    /// Kimi requires an Installation store and a captured PROJECT reference.
    /// Other command hosts use their selected PROJECT store and no global reference.
    pub fn sync_command_hook(
        &mut self,
        plan: &NativeHookPlan,
        project: Option<&ProjectId>,
    ) -> io::Result<SharedDocumentChange> {
        self.apply_command_hook(plan, project, false)
    }
    /// Publish a complete command-hook set under the document's existing lock and journal.
    pub fn sync_command_hooks(
        &mut self,
        plans: &[NativeHookPlan],
        project: Option<&ProjectId>,
    ) -> io::Result<Vec<SharedDocumentChange>> {
        self.apply_command_hooks(plans, project, false)
    }
    pub fn remove_command_hook(
        &mut self,
        plan: &NativeHookPlan,
        project: Option<&ProjectId>,
    ) -> io::Result<SharedDocumentChange> {
        self.apply_command_hook(plan, project, true)
    }
    /// Release only private ownership/reference data; never alter a foreign document.
    pub fn forget_command_hook(
        &mut self,
        plan: &NativeHookPlan,
        project: Option<&ProjectId>,
    ) -> io::Result<SharedDocumentChange> {
        self.recover()?;
        self.command_hook_owner(plan, project)?;
        let Some(index) = self.persistence.state.entries.iter().position(|entry| {
            matches!(entry, Entry::CommandHook(e) if e.binding == plan.binding && e.identifier == plan.identifier)
        }) else { return Ok(SharedDocumentChange::Unchanged) };
        let Entry::CommandHook(entry) = &mut self.persistence.state.entries[index] else {
            unreachable!()
        };
        if entry.owner != self.owner {
            return Err(conflict());
        }
        if let Some(project) = project {
            if !entry.projects.contains(project) {
                return Ok(SharedDocumentChange::Unchanged);
            }
            entry.projects.retain(|p| p != project);
        }
        if entry.projects.is_empty() {
            self.persistence.state.entries.remove(index);
        }
        self.persistence.save()?;
        Ok(SharedDocumentChange::Forgotten)
    }
    fn command_hook_owner(
        &self,
        plan: &NativeHookPlan,
        project: Option<&ProjectId>,
    ) -> io::Result<()> {
        self.request.remaining()?;
        if !matches!(
            (plan.binding, &self.owner.scope, project),
            (
                NativeBinding::Kimi,
                StoreOwner::Installation { .. },
                Some(_)
            )
        ) && !(plan.binding != NativeBinding::Kimi
            && matches!(&self.owner.scope, StoreOwner::Project { .. })
            && project.is_none())
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(())
    }
    fn apply_command_hook(
        &mut self,
        plan: &NativeHookPlan,
        project: Option<&ProjectId>,
        remove: bool,
    ) -> io::Result<SharedDocumentChange> {
        Ok(self.apply_command_hooks(std::slice::from_ref(plan), project, remove)?[0])
    }
    fn apply_command_hooks(
        &mut self,
        plans: &[NativeHookPlan],
        project: Option<&ProjectId>,
        remove: bool,
    ) -> io::Result<Vec<SharedDocumentChange>> {
        self.recover()?;
        let Some(first) = plans.first() else {
            return Ok(Vec::new());
        };
        for plan in plans {
            self.command_hook_owner(plan, project)?;
            if plan.binding != first.binding || plan.format() != first.format() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
        }
        let observed = self.parent.read(
            self.persistence.state.name.as_ref(),
            self.request.snapshot(),
        )?;
        let restore = !remove && observed.bytes().is_none();
        if restore {
            // A deleted file can be rebuilt from current plans only when they
            // cover all private ownership entries. Keep shared PROJECT references.
            for entry in &self.persistence.state.entries {
                let Entry::CommandHook(entry) = entry else {
                    return Err(conflict());
                };
                if entry.owner != self.owner
                    || !plans.iter().any(|plan| {
                        plan.binding == entry.binding && plan.identifier == entry.identifier
                    })
                {
                    return Err(conflict());
                }
            }
        }
        let input = observed
            .bytes()
            .unwrap_or(if first.format() == DocumentFormat::Toml {
                b""
            } else {
                b"{}"
            });
        let mut document = NativeDocument::parse(first.format(), input, self.limits, self.request)
            .map_err(codec)?;
        let mut after = self.persistence.state.entries.clone();
        let mut changes = Vec::with_capacity(plans.len());
        for plan in plans {
            changes.push(self.edit_command_hook(
                &mut document,
                &mut after,
                plan,
                project,
                remove,
                restore,
            )?);
        }
        let change = if changes.contains(&SharedDocumentChange::Updated) {
            SharedDocumentChange::Updated
        } else if changes.contains(&SharedDocumentChange::Created) {
            SharedDocumentChange::Created
        } else if changes.contains(&SharedDocumentChange::Removed) {
            SharedDocumentChange::Removed
        } else {
            SharedDocumentChange::Unchanged
        };
        self.publish_document(&observed, input, &document, after, change)?;
        Ok(changes)
    }
    fn edit_command_hook(
        &self,
        document: &mut NativeDocument<'_, '_>,
        after: &mut Vec<Entry>,
        plan: &NativeHookPlan,
        project: Option<&ProjectId>,
        remove: bool,
        restore: bool,
    ) -> io::Result<SharedDocumentChange> {
        self.request.remaining()?;
        let index = after.iter().position(|entry| {
            matches!(entry, Entry::CommandHook(e) if e.binding == plan.binding && e.identifier == plan.identifier)
        });
        if remove && index.is_none() {
            return Ok(SharedDocumentChange::Unchanged);
        }
        if let Some(index) = index {
            let Entry::CommandHook(entry) = &after[index] else {
                unreachable!()
            };
            if entry.owner != self.owner {
                return Err(conflict());
            }
            if entry.event != plan.event || entry.path != plan.path || entry.group != plan.group {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if let Some(project) = project {
                if remove && !entry.projects.contains(project) {
                    return Ok(SharedDocumentChange::Unchanged);
                }
                if !remove
                    && entry.fields != plan.fields
                    && entry.projects.iter().any(|p| p != project)
                {
                    return Err(conflict());
                }
            }
        }
        if plan.binding == NativeBinding::Cursor {
            match document.get(&["version"]).map_err(codec)? {
                Some(FieldValue::Integer(1)) => {}
                None if index.is_none() || restore => document
                    .set(&["version"], FieldValue::Integer(1))
                    .map_err(codec)?,
                _ => return Err(conflict()),
            }
        }
        if plan.binding == NativeBinding::Antigravity
            && index.is_none()
            && document.get(&[plan.identifier()]).map_err(codec)?.is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "foreign native hook block exists",
            ));
        }
        let path = refs(&plan.path);
        let position = if let Some(index) = index {
            let Entry::CommandHook(entry) = &after[index] else {
                unreachable!()
            };
            match document
                .hook_match(&path, entry.group.as_ref(), &entry.fields)
                .map_err(codec)?
            {
                HookMatch::Unique(position) => Some(position),
                HookMatch::Missing if restore => None,
                _ => return Err(conflict()),
            }
        } else {
            None
        };
        let change;
        if let Some(index) = index {
            let Entry::CommandHook(entry) = &mut after[index] else {
                unreachable!()
            };
            if remove {
                if let Some(project) = project {
                    entry.projects.retain(|p| p != project);
                }
                if entry.projects.is_empty() {
                    document
                        .edit_hook(
                            &path,
                            plan.group.as_ref(),
                            position,
                            &plan.tuple,
                            &plan.fields,
                            true,
                        )
                        .map_err(codec)?;
                    after.remove(index);
                }
                change = SharedDocumentChange::Removed;
            } else {
                if let Some(project) = project
                    && !entry.projects.contains(project)
                {
                    if entry.projects.len() >= self.limits.references {
                        return Err(io::ErrorKind::FileTooLarge.into());
                    }
                    entry.projects.push(*project);
                }
                if entry.fields == plan.fields && !restore {
                    change = SharedDocumentChange::Unchanged;
                } else {
                    match document
                        .hook_match(&path, plan.group.as_ref(), &plan.fields)
                        .map_err(codec)?
                    {
                        HookMatch::Missing => {}
                        HookMatch::Unique(p) if Some(p) == position => {}
                        _ => return Err(conflict()),
                    }
                    document
                        .edit_hook(
                            &path,
                            plan.group.as_ref(),
                            position,
                            &plan.tuple,
                            &plan.fields,
                            false,
                        )
                        .map_err(codec)?;
                    entry.fields = plan.fields.clone();
                    change = SharedDocumentChange::Updated;
                }
            }
        } else {
            if after
                .iter()
                .filter(|e| matches!(e, Entry::Hook(_) | Entry::CommandHook(_)))
                .count()
                >= self.limits.hook_tuples
            {
                return Err(io::ErrorKind::FileTooLarge.into());
            }
            if document
                .hook_match(&path, plan.group.as_ref(), &plan.fields)
                .map_err(codec)?
                != HookMatch::Missing
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "foreign native hook exists",
                ));
            }
            document
                .edit_hook(
                    &path,
                    plan.group.as_ref(),
                    None,
                    &plan.tuple,
                    &plan.fields,
                    false,
                )
                .map_err(codec)?;
            after.push(Entry::CommandHook(CommandHookEntry {
                identifier: plan.identifier.clone(),
                owner: self.owner.clone(),
                binding: plan.binding,
                event: plan.event.clone(),
                path: plan.path.clone(),
                group: plan.group.clone(),
                fields: plan.fields.clone(),
                projects: project.into_iter().copied().collect(),
            }));
            change = SharedDocumentChange::Created;
        }
        Ok(change)
    }
}
