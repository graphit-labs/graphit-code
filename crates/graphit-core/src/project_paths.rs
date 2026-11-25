//! Owned bootstrap and project paths, resolved once from explicit inputs.

use crate::brand::{Brand, portable_segment};
use crate::local_files::{canonical_directory, directory_exists, invalid_input};
use crate::project_identity::ProjectId;
use crate::project_root::discover_from_canonical;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct InstallationPaths {
    brand: Brand,
    start: PathBuf,
    root: PathBuf,
    home: Option<PathBuf>,
}

impl InstallationPaths {
    /// Bootstrap input must be captured by the caller before resolving normal config.
    pub fn resolve(
        brand: Brand,
        start: impl AsRef<Path>,
        home: Option<&Path>,
        global_override: Option<&Path>,
    ) -> io::Result<Self> {
        for path in home.into_iter().chain(global_override) {
            crate::native_path::valid_native(path)?;
        }
        let start = canonical_directory(start.as_ref())?;
        let global_override = global_override
            .and_then(Path::to_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(Path::new);
        let root = if let Some(path) = global_override {
            if path.is_absolute() {
                path.to_owned()
            } else {
                start.join(path)
            }
        } else {
            let home = home.filter(|home| home.is_absolute()).ok_or_else(|| {
                invalid_input("an absolute home directory or global override is required")
            })?;
            home.join(brand.directory())
        };
        if !root.is_absolute() {
            return Err(invalid_input(
                "global root must resolve to an absolute path",
            ));
        }
        Ok(Self {
            brand,
            start,
            root,
            home: home.filter(|path| path.is_absolute()).map(Path::to_owned),
        })
    }

    pub fn brand(&self) -> &Brand {
        &self.brand
    }
    pub fn start_directory(&self) -> &Path {
        &self.start
    }
    /// Optional home captured by the bootstrap owner, independent of GlobalCache.
    pub fn home_directory(&self) -> Option<&Path> {
        self.home.as_deref()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn global_path(&self, parts: &[&str]) -> io::Result<PathBuf> {
        append_segments(&self.root, parts)
    }
    pub fn framework_runtime(&self, distribution_version: &str) -> io::Result<PathBuf> {
        self.global_path(&["runtime", distribution_version])
    }
    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }
    pub fn auth_file(&self) -> PathBuf {
        self.root.join("auth.json")
    }
    pub fn models_directory(&self) -> PathBuf {
        self.root.join("models")
    }

    fn guard_project(&self, root: &Path) -> io::Result<()> {
        match fs::canonicalize(&self.root) {
            Ok(installation) if installation == root => {
                Err(invalid_input("installation directory is not a project"))
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPaths {
    root: PathBuf,
    global_root: PathBuf,
    brand: Brand,
}

impl ProjectPaths {
    /// Bind an explicit root; use `discover` when the caller starts in a subdirectory.
    pub fn at(root: impl AsRef<Path>, installation: &InstallationPaths) -> io::Result<Self> {
        let root = canonical_directory(root.as_ref())?;
        Self::from_canonical(root, installation)
    }

    fn from_canonical(root: PathBuf, installation: &InstallationPaths) -> io::Result<Self> {
        installation.guard_project(&root)?;
        directory_exists(&root.join(installation.brand.directory()))?;
        Ok(Self {
            root,
            global_root: installation.root.clone(),
            brand: installation.brand.clone(),
        })
    }

    pub fn discover(
        directory: impl AsRef<Path>,
        installation: &InstallationPaths,
    ) -> io::Result<Self> {
        let directory = canonical_directory(directory.as_ref())?;
        installation.guard_project(&directory)?;
        let root = discover_from_canonical(directory, installation.brand())?;
        Self::from_canonical(root, installation)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn brand(&self) -> &Brand {
        &self.brand
    }
    pub fn brand_directory(&self) -> PathBuf {
        self.root.join(self.brand.directory())
    }
    pub fn identity_file(&self) -> PathBuf {
        self.brand_directory().join("project.json")
    }
    pub fn global_root(&self) -> &Path {
        &self.global_root
    }
    /// Local module storage is shared by checkouts of the same captured PROJECT.
    /// Versions are selected by the module's native branch, not the checkout path.
    pub fn project_runtime(&self, project_id: ProjectId, parts: &[&str]) -> io::Result<PathBuf> {
        append_segments(
            &self
                .global_root
                .join("projects")
                .join(project_id.to_string()),
            parts,
        )
    }

    pub(crate) fn same_scope(&self, other: &Self) -> bool {
        self.root == other.root
            && self.global_root == other.global_root
            && self.brand.key() == other.brand.key()
    }
}

fn append_segments(root: &Path, parts: &[&str]) -> io::Result<PathBuf> {
    let mut path = root.to_owned();
    for part in parts {
        portable_segment(part)?;
        path.push(part);
    }
    Ok(path)
}
