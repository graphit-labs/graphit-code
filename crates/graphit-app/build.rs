//! Validate build metadata before compiling the executable. Local builds use
//! the development identity in every profile; the public release workflow
//! supplies its explicit product version.

#[path = "src/build_metadata.rs"]
mod build_metadata;

use build_metadata::{Metadata, MetadataError};
use std::{env, fmt};

#[derive(Debug)]
enum BuildError {
    Encoding,
    Metadata(MetadataError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoding => formatter.write_str("invalid build metadata encoding"),
            Self::Metadata(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for BuildError {}

fn public_build_input(name: &str) -> Result<Option<String>, BuildError> {
    env::var_os(name)
        .map(|value| value.into_string().map_err(|_| BuildError::Encoding))
        .transpose()
}

fn build() -> Result<(), BuildError> {
    for asset in ["index.html", "logo.svg", "favicon.svg"] {
        println!("cargo::rerun-if-changed=assets/ui/{asset}");
    }
    println!("cargo::rerun-if-env-changed=GRAPHIT_BUILD_BRAND");
    println!("cargo::rerun-if-env-changed=GRAPHIT_BUILD_VERSION");
    let brand = public_build_input("GRAPHIT_BUILD_BRAND")?;
    let version = public_build_input("GRAPHIT_BUILD_VERSION")?;
    let metadata = Metadata::new(
        brand.as_deref().unwrap_or("graphit"),
        version.as_deref().unwrap_or("dev"),
    )
    .map_err(BuildError::Metadata)?;
    println!(
        "cargo::rustc-env=GRAPHIT_COMPILED_BUILD_BRAND={}",
        metadata.brand().binary_name()
    );
    println!(
        "cargo::rustc-env=GRAPHIT_COMPILED_BUILD_VERSION={}",
        metadata.version()
    );
    Ok(())
}

fn main() {
    if let Err(error) = build() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
