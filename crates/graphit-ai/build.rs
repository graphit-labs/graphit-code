use std::{env, path::PathBuf};

fn main() {
    // Cargo's single links="llama" dependency builds and links the engine.
    // Its installed CMake metadata identifies the matching public headers.
    let cmake = PathBuf::from(
        env::var_os("DEP_LLAMA_GGML_CMAKE_DIR")
            .expect("llama-cpp-sys-2 must expose its installed ggml CMake directory"),
    );
    let include = cmake.join("../../include");
    cxx_build::bridge("src/native_embedding/bridge.rs")
        .file("src/native_embedding/native.cpp")
        .include(".")
        .include(include)
        .std("c++17")
        .warnings_into_errors(true)
        .compile("graphit-native-embedding");
    for file in [
        "src/native_embedding/bridge.rs",
        "src/native_embedding/native.h",
        "src/native_embedding/native.cpp",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
}
