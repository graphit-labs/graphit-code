use super::*;
use crate::profiles::QueryKind;
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericDeclaration {
    NotConfigured,
    Missing,
    Native {
        node_index: usize,
        capture_indices: Vec<usize>,
    },
    Conflict {
        capture_indices: Vec<usize>,
    },
    Invalid {
        capture_index: usize,
        reason: &'static str,
    },
}

impl ParsedGenericTreeSitterDocument<'_, '_, '_, '_, '_, '_> {
    pub fn declaration(
        &self,
        query_index: usize,
        match_index: usize,
        name_slot: usize,
    ) -> Result<GenericDeclaration, GenericModelError> {
        let observations = self.observations()?;
        let group =
            observations
                .groups
                .get(query_index)
                .ok_or(GenericModelError::NativeContract(
                    "foreign declaration query",
                ))?;
        let matched = group
            .matches
            .get(match_index)
            .ok_or(GenericModelError::NativeContract(
                "foreign declaration match",
            ))?;
        let CaptureBinding::Present(names) = &matched.bindings.name else {
            return Err(GenericModelError::NativeContract(
                "declaration has no bound name",
            ));
        };
        if !names.contains(&name_slot) {
            return Err(GenericModelError::NativeContract(
                "declaration subject does not bind name",
            ));
        }
        let name = matched
            .captures
            .get(name_slot)
            .ok_or(GenericModelError::NativeContract(
                "foreign declaration subject",
            ))?;
        let slots = match &matched.bindings.declaration {
            CaptureBinding::NotConfigured => return Ok(GenericDeclaration::NotConfigured),
            CaptureBinding::Missing { .. } => return Ok(GenericDeclaration::Missing),
            CaptureBinding::Present(slots) => slots,
        };
        let mut selected = Vec::new();
        let mut nodes = HashSet::new();
        for &slot in slots {
            self.original_context()?;
            let capture = matched
                .captures
                .get(slot)
                .ok_or(GenericModelError::NativeContract(
                    "foreign declaration capture",
                ))?;
            if capture.native.range.start > name.native.range.start
                || capture.native.range.end < name.native.range.end
            {
                continue;
            }
            let index = self.node_index(capture.native.node_id)?;
            let node = &observations.nodes[index];
            if !observations
                .profile
                .declaration_types()
                .value()
                .iter()
                .any(|kind| kind.value() == &node.kind)
            {
                return Ok(GenericDeclaration::Invalid {
                    capture_index: slot,
                    reason: "captured unit is not a configured declaration kind",
                });
            }
            selected.push(slot);
            nodes.insert(index);
        }
        if selected.is_empty() {
            return Ok(GenericDeclaration::Invalid {
                capture_index: slots
                    .first()
                    .copied()
                    .ok_or(GenericModelError::NativeContract(
                        "empty present declaration binding",
                    ))?,
                reason: "declaration does not contain its subject",
            });
        }
        if nodes.len() != 1 {
            return Ok(GenericDeclaration::Conflict {
                capture_indices: selected,
            });
        }
        let node_index = self.node_index(matched.captures[selected[0]].native.node_id)?;
        Ok(GenericDeclaration::Native {
            node_index,
            capture_indices: selected,
        })
    }

    pub(super) fn declaration_boundaries(&self) -> Result<HashSet<usize>, GenericModelError> {
        let observations = self.observations()?;
        let mut result = HashSet::new();
        for (query_index, group) in observations.groups.iter().enumerate() {
            self.original_context()?;
            if *group.query.value().r#type().value() != QueryKind::Entity
                || group.query.value().graph_label().value().is_empty()
            {
                continue;
            }
            for (match_index, matched) in group.matches.iter().enumerate() {
                if let CaptureBinding::Present(names) = &matched.bindings.name {
                    for &name_slot in names {
                        if let GenericDeclaration::Native { node_index, .. } =
                            self.declaration(query_index, match_index, name_slot)?
                        {
                            result.insert(node_index);
                        }
                    }
                }
            }
        }
        Ok(result)
    }
}
