//! Prepare the Linux signal-inheritance test executable before tests run.
use std::{env, path::PathBuf};

fn main() {
    let source = "tests/support/sigchld_inherit.c";
    println!("cargo::rerun-if-changed={source}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"))
        .join("sigchld-inherit-fixture");
    let status = cc::Build::new()
        .get_compiler()
        .to_command()
        .arg(source)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("compile signal-inheritance fixture");
    assert!(
        status.success(),
        "signal-inheritance fixture compilation failed"
    );
    println!(
        "cargo::rustc-env=GRAPHIT_TEST_SIGCHLD_FIXTURE={}",
        output.to_str().expect("Unicode Cargo output directory")
    );
}
