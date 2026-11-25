use super::*;
use crate::entity_extent::EntityExtent;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericProjectionIssue {
    LabelNotEmbedded,
    ExtentUnavailable,
    Conflict,
    OwnerExtentUnavailable,
    Recovery,
}

pub enum GenericCodeProjection<'doc, 'o> {
    Available {
        entity_index: usize,
        extent: &'doc EntityExtent<'o>,
    },
    Unavailable {
        entity_index: usize,
        reasons: Vec<GenericProjectionIssue>,
    },
}

impl<'o> GenericSemanticDocument<'_, '_, 'o, '_, '_> {
    pub fn code_projections(
        &self,
    ) -> Result<Vec<GenericCodeProjection<'_, 'o>>, GenericModelError> {
        self.original.check()?;
        let mut output = Vec::new();
        for (index, entity) in self.entities.iter().enumerate() {
            self.original.check()?;
            let mut reasons = Vec::new();
            if !self
                .observations
                .profile
                .embed_labels()
                .value()
                .iter()
                .any(|label| label.value() == entity.identity.label())
            {
                reasons.push(GenericProjectionIssue::LabelNotEmbedded);
            }
            if !entity.conflicts.is_empty() {
                reasons.push(GenericProjectionIssue::Conflict);
            }
            if self
                .owner_frames(index)?
                .any(|owner| owner.extent.is_none() || !owner.conflicts.is_empty())
            {
                reasons.push(GenericProjectionIssue::OwnerExtentUnavailable);
            }
            if matches!(self.complexities[index], GenericComplexity::Recovery { .. }) {
                reasons.push(GenericProjectionIssue::Recovery);
            }
            let extent = entity.extent();
            if extent.is_none() {
                reasons.push(GenericProjectionIssue::ExtentUnavailable);
            }
            if let Some(extent) = extent
                && reasons.is_empty()
            {
                output.push(GenericCodeProjection::Available {
                    entity_index: index,
                    extent,
                });
            } else {
                output.push(GenericCodeProjection::Unavailable {
                    entity_index: index,
                    reasons,
                });
            }
        }
        Ok(output)
    }
}
