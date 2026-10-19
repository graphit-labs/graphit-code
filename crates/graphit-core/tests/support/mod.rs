use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Fixture(PathBuf);

impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "graphit-rust-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&root).unwrap();
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
