//! Captured orchestration switches; authorization and executable availability are separate.
use crate::config::{ConfigKey, ConfigSnapshot, ConfigSource};
use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Module {
    Task,
    Memory,
    Ast,
    Knowledge,
    Hub,
    Daemon,
    Sync,
    Embedding,
    Hooks,
    Agent,
    Dream,
    DaemonUi,
}

impl Module {
    pub const ALL: [Self; 12] = [
        Self::Task,
        Self::Memory,
        Self::Ast,
        Self::Knowledge,
        Self::Hub,
        Self::Daemon,
        Self::Sync,
        Self::Embedding,
        Self::Hooks,
        Self::Agent,
        Self::Dream,
        Self::DaemonUi,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Memory => "memory",
            Self::Ast => "ast",
            Self::Knowledge => "knowledge",
            Self::Hub => "hub",
            Self::Daemon => "daemon",
            Self::Sync => "sync",
            Self::Embedding => "embedding",
            Self::Hooks => "hooks",
            Self::Agent => "agent",
            Self::Dream => "dream",
            Self::DaemonUi => "daemon_ui",
        }
    }

    pub fn key(self) -> ConfigKey {
        ConfigKey::new(&format!("modules.{}", self.name())).expect("builtin module keys are valid")
    }

    pub const fn default_enabled(self) -> bool {
        !matches!(self, Self::Dream | Self::DaemonUi)
    }
}

/// A complete view of the built-in switches, independent of later config changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModuleSettings {
    enabled: [bool; Module::ALL.len()],
}

impl ModuleSettings {
    /// Resolve all known gates or fail. Invalid winning values never become defaults.
    pub fn resolve(snapshot: &ConfigSnapshot) -> io::Result<Self> {
        let mut enabled = Module::ALL.map(Module::default_enabled);
        for module in Module::ALL {
            let key = module.key();
            if let Some(value) = snapshot.get::<bool>(&key).map_err(|_| {
                let kind = match snapshot.source(&key) {
                    Some(ConfigSource::Global | ConfigSource::Project) => {
                        io::ErrorKind::InvalidData
                    }
                    _ => io::ErrorKind::InvalidInput,
                };
                io::Error::new(kind, "module setting requires a boolean")
            })? {
                enabled[module as usize] = value;
            }
        }
        Ok(Self { enabled })
    }

    pub const fn enabled(&self, module: Module) -> bool {
        self.enabled[module as usize]
    }
}
