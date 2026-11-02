use std::{env, path::PathBuf};

fn main() {
    // Cargo's single links="llama" dependency builds and links the engine.
    // Its installed CMake metadata identifies the matching public headers.
    let cmake = PathBuf::from(
        env::var_os("DEP_LLAMA_GGML_CMAKE_DIR")
            .expect("llama-cpp-sys-2 must expose its installed ggml CMake directory"),
    );
    let include = cmake.join("../../include");
    let mut build = cxx_build::bridge("src/native_embedding/bridge.rs");
    build
        .file("src/native_embedding/native.cpp")
        .include(".")
        .include(include)
        .std("c++17")
        .warnings_into_errors(true);
    if build.get_compiler().is_like_msvc() {
        // cxx translates C++ exceptions across this bridge. Keep /WX while
        // enabling the matching MSVC unwind semantics. The bridge's standard
        // getenv reads immutable process configuration; no CRT copy is needed.
        build.flag("/EHsc").define("_CRT_SECURE_NO_WARNINGS", None);
    }
    build.compile("graphit-native-embedding");
    for file in [
        "src/native_embedding/bridge.rs",
        "src/native_embedding/native.h",
        "src/native_embedding/native.cpp",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
}
