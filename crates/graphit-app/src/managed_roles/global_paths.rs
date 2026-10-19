//! Native global destinations from the process owner's captured startup inputs.
//! These paths select publication locations; they never grant filesystem or PROJECT access.
use graphit_core::native_path::NativePath;
use graphit_hub::NativeBinding;
use path_clean::PathClean;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

const OVERRIDES: [&str; 7] = [
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "KIRO_HOME",
    "OPENCODE_CONFIG_DIR",
    "XDG_CONFIG_HOME",
    "QWEN_HOME",
    "KIMI_CODE_HOME",
];
/// Only the relevant directory overrides are retained; no provider credentials.
#[derive(Clone)]
pub struct NativeGlobalPaths {
    home: PathBuf,
    cwd: PathBuf,
    overrides: BTreeMap<&'static str, OsString>,
}
pub(crate) struct NativeDirectories {
    pub agents: PathBuf,
    pub skills: PathBuf,
}
impl NativeGlobalPaths {
    pub fn capture(home: &Path, cwd: &Path, environment: &[(OsString, OsString)]) -> Self {
        let overrides = OVERRIDES
            .into_iter()
            .filter_map(|name| {
                environment
                    .iter()
                    .find(|(key, _)| key == OsStr::new(name))
                    .map(|(_, value)| (name, value.clone()))
            })
            .collect();
        Self {
            home: home.clean(),
            cwd: cwd.clean(),
            overrides,
        }
    }
    pub(crate) fn directories(&self, binding: NativeBinding) -> io::Result<NativeDirectories> {
        let cwd = self.cwd.as_path();
        let (agents, skills) = match binding {
            NativeBinding::Codex => (
                self.root("CODEX_HOME", self.home.join(".codex"), cwd, false, false)?
                    .join("agents"),
                self.home.join(".agents/skills"),
            ),
            NativeBinding::Claude => {
                let root = self.root(
                    "CLAUDE_CONFIG_DIR",
                    self.home.join(".claude"),
                    cwd,
                    false,
                    false,
                )?;
                (root.join("agents"), root.join("skills"))
            }
            NativeBinding::Cursor => (
                self.home.join(".cursor/agents"),
                self.home.join(".cursor/skills"),
            ),
            NativeBinding::Qwen => {
                let root = self.root("QWEN_HOME", self.home.join(".qwen"), cwd, true, true)?;
                (root.join("agents"), root.join("skills"))
            }
            NativeBinding::Antigravity => (
                self.home.join(".gemini/config/agents"),
                self.home.join(".gemini/antigravity-cli/skills"),
            ),
            NativeBinding::KiroV3 => {
                let root = self.root("KIRO_HOME", self.home.join(".kiro"), cwd, false, false)?;
                (root.join("agents"), root.join("skills"))
            }
            NativeBinding::OpenCode => {
                let xdg = self.root(
                    "XDG_CONFIG_HOME",
                    self.home.join(".config"),
                    cwd,
                    true,
                    false,
                )?;
                let root = self.root(
                    "OPENCODE_CONFIG_DIR",
                    xdg.join("opencode"),
                    cwd,
                    false,
                    false,
                )?;
                (root.join("agents"), root.join("skills"))
            }
            NativeBinding::Kimi => {
                let root = self.root(
                    "KIMI_CODE_HOME",
                    self.home.join(".kimi-code"),
                    cwd,
                    false,
                    false,
                )?;
                (root.join("agents"), root.join("skills"))
            }
        };
        NativePath::new(agents.clone())?;
        NativePath::new(skills.clone())?;
        Ok(NativeDirectories { agents, skills })
    }
    /// A stable narrow set lets all active bindings share the same owned journal,
    /// including two hosts intentionally configured to use one physical directory.
    pub(crate) fn allowed_roots(&self) -> io::Result<Vec<PathBuf>> {
        let mut roots = Vec::with_capacity(NativeBinding::ALL.len() * 2);
        for binding in NativeBinding::ALL {
            let paths = self.directories(binding)?;
            roots.extend([paths.agents, paths.skills]);
        }
        roots.sort();
        roots.dedup();
        Ok(roots)
    }
    fn root(
        &self,
        name: &'static str,
        default: PathBuf,
        cwd: &Path,
        empty_unset: bool,
        expand_home: bool,
    ) -> io::Result<PathBuf> {
        let Some(raw) = self
            .overrides
            .get(name)
            .filter(|value| !empty_unset || !value.is_empty())
        else {
            return Ok(default);
        };
        if raw.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native global directory override is empty",
            ));
        }
        let path = Path::new(raw);
        let path = if expand_home {
            if path == Path::new("~") {
                self.home.clone()
            } else if let Ok(rest) = path.strip_prefix("~") {
                self.home.join(rest)
            } else {
                path.to_owned()
            }
        } else {
            path.to_owned()
        };
        let resolved = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
        .clean();
        NativePath::new(resolved.clone())?;
        Ok(resolved)
    }
}
