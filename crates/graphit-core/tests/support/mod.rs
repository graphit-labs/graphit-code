use std::fs;
use std::path::{Path, PathBuf};

pub struct Fixture(PathBuf);

impl Fixture {
    pub fn new() -> Self {
        #[cfg(target_os = "macos")]
        let parent = Path::new("/tmp").canonicalize().unwrap();
        #[cfg(not(target_os = "macos"))]
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = tempfile::Builder::new()
            .prefix("g")
            .tempdir_in(parent)
            .unwrap()
            .keep();
        Self(root)
    }

    pub fn root(&self) -> &Path {
        &self.0
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    #[allow(dead_code)]
    pub fn installation(&self) -> graphit_core::project_paths::InstallationPaths {
        graphit_core::project_paths::InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            self.root(),
            None,
            Some(&self.path("installation")),
        )
        .unwrap()
    }

    #[allow(dead_code)]
    pub fn project(&self) -> graphit_core::project_paths::ProjectPaths {
        graphit_core::project_paths::ProjectPaths::at(self.root(), &self.installation()).unwrap()
    }

    #[allow(dead_code)]
    pub fn discover(
        &self,
        directory: impl AsRef<Path>,
    ) -> std::io::Result<graphit_core::project_paths::ProjectPaths> {
        graphit_core::project_paths::ProjectPaths::discover(directory, &self.installation())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
