//! Private Cargo build fragment for the official ANTLR C++ runtime.

use std::any::Any;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct RuntimeInstall {
    pub include_dir: PathBuf,
    pub library_dir: PathBuf,
    pub static_lib_name: String,
    pub cpp_definitions: &'static [(&'static str, &'static str)],
}

// Darwin and Windows link Ladybug's ANTLR alongside ours. Give the runtime and every
// translation unit that consumes its headers the same private C++ names.
const PRIVATE_ANTLR_DEFINITIONS: &[(&str, &str)] = &[
    ("antlr4", "graphit_ast_antlr4"),
    ("antlrcpp", "graphit_ast_antlrcpp"),
    ("XPathLexer", "GraphitAstXPathLexer"),
];

pub enum BuildError {
    Environment {
        name: &'static str,
        cause: env::VarError,
    },
    Configuration(String),
    Io {
        path: PathBuf,
        cause: io::Error,
    },
    NativeBuild(Box<dyn Any + Send>),
}

impl fmt::Debug for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment { name, cause } => write!(f, "Cargo environment {name}: {cause}"),
            Self::Configuration(message) => f.write_str(message),
            Self::Io { path, cause } => write!(f, "{}: {cause}", path.display()),
            Self::NativeBuild(payload) => {
                if let Some(message) = payload.downcast_ref::<String>() {
                    write!(f, "ANTLR native build: {message}")
                } else if let Some(message) = payload.downcast_ref::<&str>() {
                    write!(f, "ANTLR native build: {message}")
                } else {
                    f.write_str("ANTLR native build failed; original panic payload retained")
                }
            }
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Environment { cause, .. } => Some(cause),
            Self::Io { cause, .. } => Some(cause),
            Self::Configuration(_) | Self::NativeBuild(_) => None,
        }
    }
}

fn cargo_environment(name: &'static str) -> Result<String, BuildError> {
    env::var(name).map_err(|cause| BuildError::Environment { name, cause })
}

fn require_file(path: &Path) -> Result<(), BuildError> {
    let metadata = fs::metadata(path).map_err(|cause| BuildError::Io {
        path: path.to_owned(),
        cause,
    })?;
    if !metadata.is_file() {
        return Err(BuildError::Configuration(format!(
            "Required ANTLR file is not a regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Build and install the vendored official static runtime under Cargo's OUT_DIR.
///
/// The caller publishes the returned library and platform link dependencies.
/// CMake/compiler diagnostics remain in the Cargo build log. A cmake-rs panic
/// becomes NativeBuild without replacing its original payload.
pub fn build_runtime() -> Result<RuntimeInstall, BuildError> {
    let manifest = PathBuf::from(cargo_environment("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(cargo_environment("OUT_DIR")?);
    let host = cargo_environment("HOST")?;
    let target = cargo_environment("TARGET")?;
    let target_os = cargo_environment("CARGO_CFG_TARGET_OS")?;
    let target_features = cargo_environment("CARGO_CFG_TARGET_FEATURE")?;
    let source = manifest.join("src/antlr_engine/cpp/vendor/antlr4");
    require_file(&source.join("CMakeLists.txt"))?;
    require_file(&source.join("runtime/src/Version.h"))?;
    let version_path = source.join("VERSION");
    let version = fs::read_to_string(&version_path).map_err(|cause| BuildError::Io {
        path: version_path,
        cause,
    })?;
    if version.trim() != "4.13.2" {
        return Err(BuildError::Configuration(format!(
            "ANTLR runtime version must be 4.13.2, found {version:?}"
        )));
    }
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-env-changed=CMAKE_TOOLCHAIN_FILE");
    let toolchain = env::var_os("CMAKE_TOOLCHAIN_FILE").map(PathBuf::from);
    if host != target && toolchain.is_none() {
        return Err(BuildError::Configuration(format!(
            "ANTLR cross compilation {host} -> {target} requires CMAKE_TOOLCHAIN_FILE"
        )));
    }
    if let Some(path) = &toolchain {
        require_file(path)?;
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let static_crt = target_features
        .split(',')
        .any(|feature| feature == "crt-static");
    let cpp_definitions = if target_os == "macos" || target_os == "windows" {
        PRIVATE_ANTLR_DEFINITIONS
    } else {
        &[]
    };
    let install = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut config = cmake::Config::new(&source);
        config
            .out_dir(out.join("antlr-runtime"))
            .host(&host)
            .target(&target)
            .profile("Release")
            .static_crt(static_crt)
            .define("CMAKE_CXX_STANDARD", "17")
            .define("CMAKE_CXX_EXTENSIONS", "OFF")
            .define("CMAKE_INSTALL_LIBDIR", "lib")
            .define("ANTLR_BUILD_STATIC", "ON")
            .define("ANTLR_BUILD_SHARED", "OFF")
            .define("ANTLR_BUILD_CPP_TESTS", "OFF")
            .define("ANTLR4_INSTALL", "ON")
            .define("WITH_DEMO", "OFF")
            .define("WITH_STATIC_CRT", if static_crt { "ON" } else { "OFF" });
        if target.ends_with("-msvc") {
            config.cxxflag("/EHsc");
        }
        for (name, value) in cpp_definitions {
            config.cxxflag(format!("-D{name}={value}"));
        }
        if let Some(path) = &toolchain {
            config.define("CMAKE_TOOLCHAIN_FILE", path);
        }
        // cmake-rs builds the official install target, including antlr4_static.
        config.build()
    }))
    .map_err(BuildError::NativeBuild)?;
    let include_dir = install.join("include/antlr4-runtime");
    let library_dir = install.join("lib");
    let static_lib_name = if target_os == "windows" {
        "antlr4-runtime-static"
    } else {
        "antlr4-runtime"
    }
    .to_owned();
    let archive = if target.contains("msvc") {
        format!("{static_lib_name}.lib")
    } else {
        format!("lib{static_lib_name}.a")
    };
    require_file(&include_dir.join("antlr4-runtime.h"))?;
    require_file(&library_dir.join(archive))?;
    Ok(RuntimeInstall {
        include_dir,
        library_dir,
        static_lib_name,
        cpp_definitions,
    })
}
