//! Observed extents and contribution conflicts shared by concrete semantic models.

use crate::{entity_extent::EntityExtent, prepared_input::OriginalProjection};

pub enum ExtentObservation<'doc, 'o> {
    Exact(EntityExtent<'o>),
    NonExact(&'doc OriginalProjection<'o>),
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributionConflictField {
    Extent,
    Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContributionConflict {
    pub field: ContributionConflictField,
    pub first_contribution: usize,
    pub second_contribution: usize,
}
