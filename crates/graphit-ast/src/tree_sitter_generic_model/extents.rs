use super::*;
use crate::entity_extent::EntityExtent;
use std::collections::HashSet;

pub enum GenericExtent<'o> {
    Missing {
        configured: bool,
    },
    Exact {
        extent: EntityExtent<'o>,
        node_index: usize,
        capture_indices: Vec<usize>,
    },
    NonExact {
        capture_indices: Vec<usize>,
    },
    Conflict {
        capture_indices: Vec<usize>,
    },
    DoesNotContainName {
        capture_indices: Vec<usize>,
    },
    Recovery {
        capture_index: usize,
        node_index: usize,
    },
}

impl<'o> ParsedGenericTreeSitterDocument<'_, '_, 'o, '_, '_, '_> {
    /// Resolve only explicit span-role occurrences, preserving native multiplicity.
    pub fn capture_extent(
        &self,
        query_index: usize,
        match_index: usize,
        name_slot: usize,
    ) -> Result<GenericExtent<'o>, GenericModelError> {
        let group = self
            .observations()?
            .groups
            .get(query_index)
            .ok_or(GenericModelError::NativeContract("foreign extent query"))?;
        let matched = group
            .matches
            .get(match_index)
            .ok_or(GenericModelError::NativeContract("foreign extent match"))?;
        let CaptureBinding::Present(names) = &matched.bindings.name else {
            return Err(GenericModelError::NativeContract(
                "extent subject has no bound name",
            ));
        };
        if !names.contains(&name_slot) {
            return Err(GenericModelError::NativeContract(
                "extent subject does not bind name role",
            ));
        }
        let name = matched
            .captures
            .get(name_slot)
            .ok_or(GenericModelError::NativeContract("foreign extent subject"))?;
        let slots = match &matched.bindings.span {
            CaptureBinding::NotConfigured => {
                return Ok(GenericExtent::Missing { configured: false });
            }
            CaptureBinding::Missing { .. } => {
                return Ok(GenericExtent::Missing { configured: true });
            }
            CaptureBinding::Present(slots) => slots,
        };
        let mut selected = Vec::new();
        let mut identities = HashSet::new();
        for &slot in slots {
            self.original_context()?;
            let capture = matched
                .captures
                .get(slot)
                .ok_or(GenericModelError::NativeContract("foreign span capture"))?;
            if capture.native.range.start <= name.native.range.start
                && capture.native.range.end >= name.native.range.end
            {
                selected.push(slot);
                identities.insert(capture.native.node_id);
            }
        }
        if selected.is_empty() {
            return Ok(GenericExtent::DoesNotContainName {
                capture_indices: slots.clone(),
            });
        }
        if identities.len() != 1 {
            return Ok(GenericExtent::Conflict {
                capture_indices: selected,
            });
        }
        // All selected occurrences refer to the same native node, not a first-wins choice.
        let slot = selected[0];
        let capture = &matched.captures[slot];
        let node_index = self.node_index(capture.native.node_id)?;
        let mut stack = vec![node_index];
        while let Some(index) = stack.pop() {
            self.original_context()?;
            let node = &self.observations()?.nodes[index];
            if node.error || node.missing {
                return Ok(GenericExtent::Recovery {
                    capture_index: slot,
                    node_index: index,
                });
            }
            stack.extend(self.children[index].iter().copied());
        }
        let OriginalProjection::Exact(raw) = &capture.original else {
            return Ok(GenericExtent::NonExact {
                capture_indices: selected,
            });
        };
        Ok(GenericExtent::Exact {
            extent: EntityExtent::new(&self.source, *raw, self.original_context()?)?,
            node_index,
            capture_indices: selected,
        })
    }
}
