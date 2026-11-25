#[path = "src/antlr_engine/build_bridge.rs"]
mod build_bridge;
#[path = "src/antlr_engine/build_support.rs"]
mod build_support;
mod icebug_query_build;
mod tree_sitter_build;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tree_sitter_build::build()?;
    let runtime = build_support::build_runtime()?;
    build_bridge::build_recognizers(&runtime)?;
    println!(
        "cargo:rustc-link-search=native={}",
        runtime.library_dir.display()
    );
    println!("cargo:rustc-link-lib=static={}", runtime.static_lib_name);
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")?;
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
    if target_os != "windows" {
        println!("cargo:rustc-link-lib=pthread");
    }
    icebug_query_build::build()?;
    Ok(())
}
