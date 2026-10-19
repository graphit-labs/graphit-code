//! Finite PROJECT birth-root DATA. Decoding never restores scope authority.
use super::*;
use crate::native_file_identity::PhysicalId;
use std::fmt::Write;

#[derive(Clone, Copy)]
pub struct OwnerRootBindingLimits {
    max_brand_bytes: usize,
    max_wire_bytes: usize,
}
impl OwnerRootBindingLimits {
    pub fn new(max_brand_bytes: usize, max_wire_bytes: usize) -> io::Result<Self> {
        if max_brand_bytes == 0 || max_wire_bytes == 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            max_brand_bytes,
            max_wire_bytes,
        })
    }
    fn size(&self, brand: &str, identity: &PhysicalId) -> io::Result<usize> {
        if brand.len() > self.max_brand_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let physical = match identity {
            PhysicalId::Unix { .. } => 17,
            PhysicalId::Windows { .. } => 33,
        };
        let mode = match identity {
            PhysicalId::Unix { .. } => 5,
            PhysicalId::Windows { .. } => 2,
        };
        let bytes = 8_usize
            .checked_add(2 + 1 + 26 + 4 + physical + 1 + mode)
            .and_then(|n| n.checked_add(brand.len()))
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if bytes > self.max_wire_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(bytes)
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum OwnerRootIdentity {
    Unix {
        device: u64,
        inode: u64,
    },
    Windows {
        volume: u64,
        index: [u8; 16],
        created: u64,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OwnerRootMode {
    Unix(u32),
    Windows { readonly: bool },
}

/// Bounded observation data, including full native identity. No paths or grants.
///
/// ```compile_fail
/// use graphit_core::artifact_store::{CapturedPublicationScope, OwnerRootBindingRecord};
/// fn restore(record: OwnerRootBindingRecord) -> CapturedPublicationScope { record.into() }
/// ```
pub struct OwnerRootBindingRecord {
    project_id: ProjectId,
    brand: String,
    identity: PhysicalId,
    mode: OwnerRootMode,
    wire: Vec<u8>,
}
impl OwnerRootBindingRecord {
    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }
    pub fn brand(&self) -> &str {
        &self.brand
    }
    pub fn root_identity(&self) -> OwnerRootIdentity {
        match self.identity {
            PhysicalId::Unix { device, inode } => OwnerRootIdentity::Unix { device, inode },
            PhysicalId::Windows {
                volume,
                index,
                created,
            } => OwnerRootIdentity::Windows {
                volume,
                index,
                created,
            },
        }
    }
    pub fn root_kind(&self) -> graphit_fs::NodeKind {
        graphit_fs::NodeKind::Directory
    }
    pub fn root_mode(&self) -> OwnerRootMode {
        self.mode
    }
    pub fn wire(&self) -> &[u8] {
        &self.wire
    }

    /// Decode bounded DATA only. Even valid bytes cannot recover native authority.
    pub fn decode(bytes: &[u8], limits: OwnerRootBindingLimits) -> io::Result<Self> {
        if bytes.len() > limits.max_wire_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut input = Input { bytes, offset: 0 };
        if input.take(8)? != b"GOROOT01"
            || input.take(2)? != 1_u16.to_le_bytes()
            || input.byte()? != 1
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let project_id = std::str::from_utf8(input.take(26)?)
            .map_err(|_| io::ErrorKind::InvalidData)?
            .parse::<ProjectId>()?;
        let length = u32::from_le_bytes(
            input
                .take(4)?
                .try_into()
                .map_err(|_| io::ErrorKind::InvalidData)?,
        ) as usize;
        if length == 0 || length > limits.max_brand_bytes || length > 64 {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let brand =
            std::str::from_utf8(input.take(length)?).map_err(|_| io::ErrorKind::InvalidData)?;
        if Brand::new(brand)?.key() != brand {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let identity = match input.byte()? {
            1 => PhysicalId::Unix {
                device: input.u64()?,
                inode: input.u64()?,
            },
            2 => PhysicalId::Windows {
                volume: input.u64()?,
                index: input
                    .take(16)?
                    .try_into()
                    .map_err(|_| io::ErrorKind::InvalidData)?,
                created: input.u64()?,
            },
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        let valid = match &identity {
            PhysicalId::Unix { inode, .. } => *inode != 0,
            PhysicalId::Windows { index, created, .. } => {
                index.iter().any(|byte| *byte != 0) && *created != 0
            }
        };
        if !valid {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if input.byte()? != 1 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mode = match (input.byte()?, &identity) {
            (1, PhysicalId::Unix { .. }) => {
                let mode = u32::from_le_bytes(
                    input
                        .take(4)?
                        .try_into()
                        .map_err(|_| io::ErrorKind::InvalidData)?,
                );
                if mode > 0o777 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                OwnerRootMode::Unix(mode)
            }
            (2, PhysicalId::Windows { .. }) => match input.byte()? {
                0 => OwnerRootMode::Windows { readonly: false },
                1 => OwnerRootMode::Windows { readonly: true },
                _ => return Err(io::ErrorKind::InvalidData.into()),
            },
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        if input.offset != bytes.len() || limits.size(brand, &identity)? != bytes.len() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Self::materialize(project_id, brand, identity, mode, limits)
    }
    fn materialize(
        project_id: ProjectId,
        brand: &str,
        identity: PhysicalId,
        mode: OwnerRootMode,
        limits: OwnerRootBindingLimits,
    ) -> io::Result<Self> {
        let size = limits.size(brand, &identity)?;
        let mut wire = Vec::new();
        wire.try_reserve_exact(size)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        wire.extend_from_slice(b"GOROOT01");
        wire.extend_from_slice(&1_u16.to_le_bytes());
        wire.push(1);
        let mut id = FixedId {
            bytes: [0; 26],
            length: 0,
        };
        write!(&mut id, "{project_id}").map_err(|_| io::ErrorKind::InvalidData)?;
        if id.length != 26 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        wire.extend_from_slice(&id.bytes);
        let length = u32::try_from(brand.len()).map_err(|_| io::ErrorKind::FileTooLarge)?;
        wire.extend_from_slice(&length.to_le_bytes());
        wire.extend_from_slice(brand.as_bytes());
        match &identity {
            PhysicalId::Unix { device, inode } => {
                wire.push(1);
                wire.extend_from_slice(&device.to_le_bytes());
                wire.extend_from_slice(&inode.to_le_bytes());
            }
            PhysicalId::Windows {
                volume,
                index,
                created,
            } => {
                wire.push(2);
                wire.extend_from_slice(&volume.to_le_bytes());
                wire.extend_from_slice(index);
                wire.extend_from_slice(&created.to_le_bytes());
            }
        }
        wire.push(1);
        match mode {
            OwnerRootMode::Unix(mode) => {
                wire.push(1);
                wire.extend_from_slice(&mode.to_le_bytes());
            }
            OwnerRootMode::Windows { readonly } => {
                wire.push(2);
                wire.push(u8::from(readonly));
            }
        }
        let mut copied_brand = String::new();
        copied_brand
            .try_reserve_exact(brand.len())
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        copied_brand.push_str(brand);
        Ok(Self {
            project_id,
            brand: copied_brand,
            identity,
            mode,
            wire,
        })
    }
}
struct FixedId {
    bytes: [u8; 26],
    length: usize,
}
impl fmt::Write for FixedId {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.length..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}
struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Input<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(io::ErrorKind::InvalidData)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(io::ErrorKind::InvalidData)?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| io::ErrorKind::InvalidData)?,
        ))
    }
}
impl CapturedPublicationScope {
    pub fn project_owner_binding(
        &self,
        limits: OwnerRootBindingLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<OwnerRootBindingRecord> {
        self.require_project()?;
        request.check()?;
        let _admission = super::super::native_source::RootReadAdmission::new(self, request)?;
        let directories = self
            .inner
            .directories
            .try_lock()
            .map_err(|error| match error {
                std::sync::TryLockError::WouldBlock => io::ErrorKind::WouldBlock,
                std::sync::TryLockError::Poisoned(_) => io::ErrorKind::Other,
            })?;
        let node = directories
            .get(&self.inner.owner_root)
            .ok_or(io::ErrorKind::InvalidData)?;
        self.native_anchor().matches_receipt(node, request)?;
        if !node.content.directory() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let StoreOwner::Project { project_id } = &self.inner.owner else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        let brand = self.inner.brand.key();

        let mode = match node.mode {
            tree::Mode::Unix(mode) => OwnerRootMode::Unix(mode),
            tree::Mode::Windows(readonly) => OwnerRootMode::Windows { readonly },
        };
        request.check()?;
        let value =
            OwnerRootBindingRecord::materialize(*project_id, brand, node.id.clone(), mode, limits)?;
        request.check()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
