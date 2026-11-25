use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericOwnedContext {
    pub node_index: usize,
    pub capture_indices: Vec<usize>,
}

impl ParsedGenericTreeSitterDocument<'_, '_, '_, '_, '_, '_> {
    fn context_unit(
        &self,
        query_index: usize,
        match_index: usize,
        name_slot: usize,
        context: usize,
    ) -> Result<Option<usize>, GenericModelError> {
        let mut units = Vec::new();
        if let GenericExtent::Exact { node_index, .. } =
            self.capture_extent(query_index, match_index, name_slot)?
        {
            units.push(node_index);
        }
        if let GenericDeclaration::Native { node_index, .. } =
            self.declaration(query_index, match_index, name_slot)?
            && !units.contains(&node_index)
        {
            units.push(node_index);
        }
        for index in units {
            if self.native_ancestor(index, context)?
                && !self.subtree_recovered(index)?
                && matches!(self.node_origin(index)?, OriginalProjection::Exact(_))
            {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    pub(super) fn capture_contexts(
        &self,
        query_index: usize,
        match_index: usize,
        name_slot: usize,
    ) -> Result<Result<Vec<GenericOwnedContext>, GenericEntityIssue>, GenericModelError> {
        let observations = self.observations()?;
        let group = observations
            .groups
            .get(query_index)
            .ok_or(GenericModelError::NativeContract("foreign context query"))?;
        let matched = group
            .matches
            .get(match_index)
            .ok_or(GenericModelError::NativeContract("foreign context match"))?;
        let CaptureBinding::Present(names) = &matched.bindings.name else {
            return Err(GenericModelError::NativeContract(
                "context has no bound subject",
            ));
        };
        if !names.contains(&name_slot) {
            return Err(GenericModelError::NativeContract(
                "context subject is not a name binding",
            ));
        }
        let slots = match &matched.bindings.context {
            CaptureBinding::NotConfigured => return Ok(Ok(Vec::new())),
            CaptureBinding::Missing { .. } => return Ok(Err(GenericEntityIssue::MissingContext)),
            CaptureBinding::Present(slots) => slots,
        };
        let subject = self.node_index(matched.captures[name_slot].native.node_id)?;
        let mut contexts: Vec<GenericOwnedContext> = Vec::new();
        for &slot in slots {
            self.original_context()?;
            let capture = matched
                .captures
                .get(slot)
                .ok_or(GenericModelError::NativeContract("foreign context capture"))?;
            let node_index = self.node_index(capture.native.node_id)?;
            if self.subtree_recovered(node_index)?
                || !matches!(capture.original, OriginalProjection::Exact(_))
            {
                return Ok(Err(GenericEntityIssue::InvalidContext));
            }
            let mut owners = Vec::new();
            for &name in names {
                if self.capture_name(query_index, match_index, name)?.is_err() {
                    continue;
                }
                if let Some(unit) = self.context_unit(query_index, match_index, name, node_index)?
                    && self.native_ancestor(unit, node_index)?
                {
                    let owner = self.node_index(matched.captures[name].native.node_id)?;
                    if !owners.contains(&owner) {
                        owners.push(owner);
                    }
                }
            }
            if owners.is_empty() {
                return Ok(Err(GenericEntityIssue::InvalidContext));
            }
            if !owners.contains(&subject) {
                continue;
            }
            if owners.len() != 1 {
                return Ok(Err(GenericEntityIssue::AmbiguousContext));
            }
            if let Some(existing) = contexts
                .iter_mut()
                .find(|context| context.node_index == node_index)
            {
                existing.capture_indices.push(slot);
            } else {
                contexts.push(GenericOwnedContext {
                    node_index,
                    capture_indices: vec![slot],
                });
            }
        }
        if contexts.is_empty() {
            Ok(Err(GenericEntityIssue::MissingContext))
        } else {
            Ok(Ok(contexts))
        }
    }

    pub(super) fn declaration_contexts(
        &self,
        declaration: usize,
    ) -> Result<Result<Vec<usize>, Vec<GenericEntityIssue>>, GenericModelError> {
        let observations = self.observations()?;
        let mut contexts = Vec::new();
        let mut issues = Vec::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            if *group.query.value().r#type().value() != crate::profiles::QueryKind::Entity {
                continue;
            }
            for (match_index, matched) in group.matches.iter().enumerate() {
                let CaptureBinding::Present(names) = &matched.bindings.name else {
                    continue;
                };
                for &name_slot in names {
                    if !matches!(self.declaration(query_index,match_index,name_slot)?,GenericDeclaration::Native{node_index,..} if node_index==declaration)
                    {
                        continue;
                    }
                    match self.capture_contexts(query_index, match_index, name_slot)? {
                        Ok(owned) => {
                            for context in owned {
                                if !contexts.contains(&context.node_index) {
                                    contexts.push(context.node_index);
                                }
                            }
                        }
                        Err(issue) => {
                            if !issues.contains(&issue) {
                                issues.push(issue);
                            }
                        }
                    }
                }
            }
        }
        if issues.is_empty() {
            Ok(Ok(contexts))
        } else {
            Ok(Err(issues))
        }
    }
}
