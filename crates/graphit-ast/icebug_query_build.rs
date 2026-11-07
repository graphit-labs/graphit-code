//! Compile Graphit's adapter against the same Cargo-sourced engine as lbug.
use std::{env, error::Error, path::PathBuf};
pub fn build() -> Result<(), Box<dyn Error>> {
    let missing = "prepare Ladybug with cargo xtask cargo build/test/check (see docs/specs/rust_ladybug_sdk.md)";
    let source = PathBuf::from(env::var_os("GRAPHIT_LBUG_SOURCE_DIR").ok_or(missing)?);
    let generated = PathBuf::from(env::var_os("LBUG_INCLUDE_DIR").ok_or(missing)?);
    let library = PathBuf::from(env::var_os("LBUG_LIBRARY_DIR").ok_or(missing)?);
    if env::var_os("LBUG_SHARED").is_some() {
        return Err("Graphit's Icebug adapter uses the prepared static Ladybug engine".into());
    }
    for path in [
        source.join("src/include/storage/table/external_table_hooks.h"),
        generated.join("lbug.hpp"),
        generated.join("include/common/system_config.h"),
    ] {
        if !path.is_file() {
            return Err(format!("required Ladybug build input missing: {}", path.display()).into());
        }
    }
    let target_os = env::var("CARGO_CFG_TARGET_OS")?;
    if target_os == "windows"
        && env::var("CARGO_CFG_TARGET_FEATURE")?
            .split(',')
            .any(|feature| feature == "crt-static")
    {
        return Err("official lbug binding uses /MD; crt-static is incompatible".into());
    }
    if !library
        .join(if target_os == "windows" {
            "lbug.lib"
        } else {
            "liblbug.a"
        })
        .is_file()
    {
        return Err("prepared Ladybug library is missing".into());
    }
    let mut build = cxx_build::bridge("src/icebug_query.rs");
    if env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some() {
        build.define("GRAPHIT_QUERY_TEST_SUPPORT", None);
    }
    build
        .define("LBUG_STATIC_DEFINE", None)
        .define("NDEBUG", None)
        .file("native/icebug_query.cpp")
        .include(".")
        .include(source.join("src/include"))
        .include(&generated)
        .include(generated.join("include"))
        .std("c++20");
    for component in [
        "yyjson/src",
        "parquet",
        "thrift",
        "alp/include",
        "fastpfor",
        "nlohmann_json",
    ] {
        build.include(source.join("third_party").join(component));
    }
    build.compile("graphit_icebug_query");
    for path in [
        "src/icebug_query.rs",
        "native/icebug_query.hpp",
        "native/icebug_query.cpp",
        "native/icebug_tables.hpp",
        "icebug_query_build.rs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for variable in [
        "GRAPHIT_LBUG_SOURCE_DIR",
        "LBUG_INCLUDE_DIR",
        "LBUG_LIBRARY_DIR",
        "LBUG_SHARED",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    println!(
        "cargo:rerun-if-changed={}",
        generated.join("include/common/system_config.h").display()
    );
    Ok(())
}
