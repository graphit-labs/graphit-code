//! Exact Cargo-built Runtime fixture supplied by the guarded test runner.
use std::{io, path::Path};

pub fn install(bin: &Path, config: Option<serde_json::Value>, names: &[&str]) -> io::Result<()> {
    let executable = std::env::var_os("GRAPHIT_TEST_APP_RUNTIME_FIXTURE").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "GRAPHIT_TEST_APP_RUNTIME_FIXTURE is required; run cargo xtask cargo test",
        )
    })?;
    if let Some(config) = config {
        std::fs::write(bin.join("peer.json"), serde_json::to_vec(&config)?)?;
    }
    for name in names {
        std::os::unix::fs::symlink(&executable, bin.join(name))?;
    }
    Ok(())
}
