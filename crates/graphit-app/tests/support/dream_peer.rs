//! Exact Cargo-built peer supplied by the guarded test runner.
use std::{io, path::Path};

pub fn install(destination: &Path) -> io::Result<()> {
    let executable = std::env::var_os("GRAPHIT_TEST_APP_DREAM_FIXTURE")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "run cargo xtask cargo test so the workspace app-dream-fixture artifact is supplied",
            )
        })?;
    if !executable.is_absolute() || !executable.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "GRAPHIT_TEST_APP_DREAM_FIXTURE must name the emitted workspace fixture executable",
        ));
    }
    std::fs::copy(executable, destination)?;
    Ok(())
}
