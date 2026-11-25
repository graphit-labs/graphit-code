//! Process-owned private runtime secret at a configured conventional path.
use crate::{
    file_io::{RequestIdentity, SnapshotContext},
    file_lock::FileLock,
    private_document,
};
use secrecy::zeroize::Zeroizing;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::RwLock,
};

pub const RUNTIME_PRIVATE_NAMES: [&str; 2] = ["mcp.key", "mcp.owner.lock"];
const KEY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKeyError {
    Invalid,
    Limit,
    Conflict,
    Busy,
    Cancelled,
    Deadline,
    Io,
}
impl std::fmt::Display for RuntimeKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid runtime key or private access",
            Self::Limit => "runtime key limit exceeded",
            Self::Conflict => "runtime key content changed",
            Self::Busy => "runtime key owner busy",
            Self::Cancelled => "runtime key operation cancelled",
            Self::Deadline => "runtime key deadline exceeded",
            Self::Io => "runtime key I/O failed",
        })
    }
}
impl std::error::Error for RuntimeKeyError {}
impl From<io::Error> for RuntimeKeyError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::Interrupted => Self::Cancelled,
            io::ErrorKind::TimedOut => Self::Deadline,
            io::ErrorKind::FileTooLarge => Self::Limit,
            io::ErrorKind::WouldBlock => Self::Busy,
            io::ErrorKind::InvalidData | io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound => {
                Self::Conflict
            }
            io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput => Self::Invalid,
            _ => Self::Io,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKeyEffect {
    NotApplied,
    Applied,
}

struct KeyState {
    generation: u64,
    present: bool,
}
pub struct RuntimeKeyOwner {
    root: PathBuf,
    key_path: PathBuf,
    _owner_lock: FileLock,
    secret: Zeroizing<[u8; KEY]>,
    state: RwLock<KeyState>,
}

/// A borrow of the actual process secret, bound to its owner generation and request.
pub struct RuntimeKeyBorrow<'a> {
    owner: &'a RuntimeKeyOwner,
    generation: u64,
    request: RequestIdentity,
}
impl RuntimeKeyBorrow<'_> {
    pub fn root(&self) -> &Path {
        self.owner.root()
    }
    pub fn recheck(&self, original: &SnapshotContext) -> Result<(), RuntimeKeyError> {
        original.check()?;
        if !self.request.matches(original) {
            return Err(RuntimeKeyError::Conflict);
        }
        let state = self.owner.state.read().map_err(|_| RuntimeKeyError::Io)?;
        if self.generation != state.generation {
            return Err(RuntimeKeyError::Conflict);
        }
        self.owner.verify(&state, original)
    }
    pub fn secret(&self, original: &SnapshotContext) -> Result<&[u8; KEY], RuntimeKeyError> {
        self.recheck(original)?;
        Ok(&self.owner.secret)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl RuntimeKeyOwner {
    pub fn start(root: PathBuf, original: &SnapshotContext) -> Result<Self, RuntimeKeyError> {
        original.check()?;
        private_document::private_directory(&root)?;
        let directory = root.join("daemon");
        match graphit_fs::create_private_directory(&directory) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        private_document::private_directory(&directory)?;
        let owner_lock = private_document::try_lock(&directory.join("mcp.owner.lock"), original)?
            .ok_or(RuntimeKeyError::Busy)?;
        let key_path = directory.join("mcp.key");
        let previous = private_document::read(&key_path, KEY, original)?;
        let secret = entropy()?;
        private_document::write_atomic(
            &key_path,
            previous.as_deref().map(Vec::as_slice),
            &secret[..],
            KEY,
            original,
        )?;
        Ok(Self {
            root,
            key_path,
            _owner_lock: owner_lock,
            secret,
            state: RwLock::new(KeyState {
                generation: 1,
                present: true,
            }),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn verify(&self, state: &KeyState, original: &SnapshotContext) -> Result<(), RuntimeKeyError> {
        original.check()?;
        if !state.present {
            return Err(RuntimeKeyError::Conflict);
        }
        let current = private_document::read(&self.key_path, KEY, original)?;
        if current.as_deref().map(Vec::as_slice) != Some(&self.secret[..]) {
            return Err(RuntimeKeyError::Conflict);
        }
        Ok(())
    }
    pub fn check(
        &self,
        original: &SnapshotContext,
    ) -> Result<RuntimeKeyBorrow<'_>, RuntimeKeyError> {
        original.check()?;
        let state = self.state.read().map_err(|_| RuntimeKeyError::Io)?;
        self.verify(&state, original)?;
        Ok(RuntimeKeyBorrow {
            owner: self,
            generation: state.generation,
            request: original.request_identity(),
        })
    }
    pub fn maintain(
        &self,
        original: &SnapshotContext,
    ) -> Result<RuntimeKeyEffect, RuntimeKeyError> {
        original.check()?;
        let mut state = self.state.write().map_err(|_| RuntimeKeyError::Io)?;
        if !state.present {
            return Err(RuntimeKeyError::Conflict);
        }
        let current = private_document::read(&self.key_path, KEY, original)?;
        if current.is_some() {
            self.verify(&state, original)?;
            return Ok(RuntimeKeyEffect::NotApplied);
        }
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(RuntimeKeyError::Limit)?;
        private_document::write_atomic(&self.key_path, None, &self.secret[..], KEY, original)?;
        state.generation = generation;
        Ok(RuntimeKeyEffect::Applied)
    }
    pub fn remove(&self, original: &SnapshotContext) -> Result<RuntimeKeyEffect, RuntimeKeyError> {
        original.check()?;
        let mut state = self.state.write().map_err(|_| RuntimeKeyError::Io)?;
        self.verify(&state, original)?;
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(RuntimeKeyError::Limit)?;
        original.check()?;
        fs::remove_file(&self.key_path)?;
        state.present = false;
        state.generation = generation;
        Ok(RuntimeKeyEffect::Applied)
    }
}

fn entropy() -> Result<Zeroizing<[u8; KEY]>, RuntimeKeyError> {
    let mut random = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *random).map_err(|_| RuntimeKeyError::Io)?;
    let mut secret = Zeroizing::new([0; KEY]);
    let hex = b"0123456789abcdef";
    for (index, byte) in random.iter().enumerate() {
        secret[2 * index] = hex[usize::from(byte >> 4)];
        secret[2 * index + 1] = hex[usize::from(byte & 15)];
    }
    Ok(secret)
}
#[cfg(test)]
mod tests;
