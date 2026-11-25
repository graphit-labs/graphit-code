//! Cargo-produced native wire peer; scenarios contain data, never executable scripts.
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn install(program: &Path, scenario: Value) {
    let binary = PathBuf::from(
        std::env::var_os("GRAPHIT_TEST_APP_AGENT_FIXTURE")
            .expect("cargo xtask cargo test must emit the app-agent-fixture target"),
    );
    assert!(binary.is_absolute() && binary.is_file());
    std::fs::copy(binary, program).unwrap();
    std::fs::write(
        program.with_extension("fixture.json"),
        serde_json::to_vec(&scenario).unwrap(),
    )
    .unwrap();
}
