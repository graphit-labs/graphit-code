//! A USER-only recipe cursor bound to real per-operation admissions.
use super::{AppliedOperation, recipe};
use graphit_auth::{UserReadAdmission, UserWriteAdmission};
use graphit_core::{artifact_store::StoreOwner, brand::Brand, file_io::SnapshotContext};
use graphit_records::{
    memory_store::{AutomaticDeleteOutcome, MemoryStoreError, MemoryStoreWriteLimits},
    user_memory_store::NativeUserMemoryStore,
};
use std::sync::Arc;

/// Owns recipe data and its logical USER identity, not an admission or principal.
pub struct UserApplication {
    recipe: recipe::PreparedRecipe,
    owner: StoreOwner,
    brand: Brand,
    next: usize,
    attempted: bool,
    receipts: Vec<AppliedOperation>,
}
impl UserApplication {
    pub fn new(
        recipe: recipe::PreparedRecipe,
        read: &UserReadAdmission<'_>,
    ) -> Result<Self, MemoryStoreError> {
        let user = read.user()?;
        if !matches!(user.owner(), StoreOwner::User { .. }) {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(Self {
            recipe,
            owner: user.owner().clone(),
            brand: user.brand().clone(),
            next: 0,
            attempted: false,
            receipts: Vec::new(),
        })
    }
    pub fn receipts(&self) -> &[AppliedOperation] {
        &self.receipts
    }
    pub fn is_complete(&self) -> bool {
        self.next == self.recipe.operations.len()
    }
    fn check(
        &self,
        original: &Arc<SnapshotContext>,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
    ) -> Result<(), MemoryStoreError> {
        original.check()?;
        read.recheck()?;
        write.recheck()?;
        let r = read.user()?;
        let w = write.user()?;
        if r.owner() != &self.owner
            || w.owner() != &self.owner
            || r.brand() != &self.brand
            || w.brand() != &self.brand
            || !Arc::ptr_eq(read.request()?.snapshot(), original)
            || !Arc::ptr_eq(write.request()?.snapshot(), original)
        {
            return Err(MemoryStoreError::WrongAdmission);
        }
        Ok(())
    }
    /// The SDK observes a first attempt under its writer before any mutation.
    /// A retained attempt is reobserved under fresh legitimate USER admissions.
    /// A refusal or receipt advances the cursor; an uncertain effect retains it.
    pub async fn apply(
        &mut self,
        original: &Arc<SnapshotContext>,
        read: &UserReadAdmission<'_>,
        write: &UserWriteAdmission<'_>,
        limits: MemoryStoreWriteLimits,
    ) -> Result<(), MemoryStoreError> {
        self.check(original, read, write)?;
        if limits.max_input_bytes < self.recipe.maximum {
            return Err(MemoryStoreError::Invalid(
                "recipe exceeds captured write policy",
            ));
        }
        while let Some(operation) = self.recipe.operations.get(self.next) {
            self.check(original, read, write)?;
            let reobserve = self.attempted;
            self.attempted = true;
            let receipt = match operation {
                recipe::Operation::Fold(input) => {
                    let observed = if reobserve {
                        NativeUserMemoryStore::reobserve_automatic_fold(
                            original,
                            read,
                            write,
                            input.clone(),
                            limits,
                        )
                        .await?
                    } else {
                        None
                    };
                    AppliedOperation::Fold(match observed {
                        Some(receipt) => receipt,
                        None => {
                            NativeUserMemoryStore::automatic_fold(
                                original,
                                read,
                                write,
                                input.clone(),
                                limits,
                            )
                            .await?
                        }
                    })
                }
                recipe::Operation::Delete(id, input) => {
                    let observed = if reobserve {
                        NativeUserMemoryStore::reobserve_automatic_delete(
                            original,
                            read,
                            write,
                            *id,
                            input.clone(),
                            limits,
                        )
                        .await?
                    } else {
                        None
                    };
                    AppliedOperation::Delete(match observed {
                        Some(effect) => AutomaticDeleteOutcome::Deleted(effect),
                        None => {
                            NativeUserMemoryStore::automatic_delete(
                                original,
                                read,
                                write,
                                *id,
                                input.clone(),
                                limits,
                            )
                            .await?
                        }
                    })
                }
            };
            self.receipts.push(receipt);
            self.next += 1;
            self.attempted = false;
        }
        self.check(original, read, write)
    }
}
