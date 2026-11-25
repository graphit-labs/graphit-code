//! Public build metadata admission, shared by the build script and executable.

use graphit_core::brand::Brand;
use std::fmt;

pub const MAX_BRAND_BYTES: usize = 64;
pub const MAX_VERSION_BYTES: usize = 128;

pub(crate) struct Metadata<'a> {
    brand: Brand,
    version: &'a str,
}

impl<'a> Metadata<'a> {
    pub(crate) fn new(brand: &str, version: &'a str) -> Result<Self, MetadataError> {
        // Admit raw bytes before Core's canonicalization: trimming must never
        // turn an invalid explicit build input into an accepted namespace.
        if brand.is_empty()
            || brand.len() > MAX_BRAND_BYTES
            || !brand.bytes().all(|byte| {
                byte.is_ascii() && !byte.is_ascii_whitespace() && !byte.is_ascii_control()
            })
        {
            return Err(MetadataError::InvalidBrand);
        }
        if version.is_empty()
            || version.len() > MAX_VERSION_BYTES
            || !version.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
        {
            return Err(MetadataError::InvalidVersion);
        }
        let brand = Brand::new(brand).map_err(|_| MetadataError::InvalidBrand)?;
        Ok(Self { brand, version })
    }

    pub(crate) fn brand(&self) -> &Brand {
        &self.brand
    }

    pub(crate) fn version(&self) -> &str {
        self.version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MetadataError {
    InvalidBrand,
    InvalidVersion,
}

impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidBrand => "invalid build brand",
            Self::InvalidVersion => "invalid build version",
        })
    }
}

impl std::error::Error for MetadataError {}

#[cfg(test)]
mod tests {
    use super::{MAX_BRAND_BYTES, MAX_VERSION_BYTES, Metadata, MetadataError};

    #[test]
    fn accepts_exact_byte_budgets() {
        let brand = "a".repeat(MAX_BRAND_BYTES);
        let version = "v".repeat(MAX_VERSION_BYTES);
        let metadata = Metadata::new(&brand, &version).expect("exact budgets are valid");
        assert_eq!(metadata.brand().binary_name(), brand);
        assert_eq!(metadata.version(), version);
    }

    #[test]
    fn rejects_one_byte_over_each_budget() {
        assert!(matches!(
            Metadata::new(&"a".repeat(MAX_BRAND_BYTES + 1), "dev"),
            Err(MetadataError::InvalidBrand)
        ));
        assert!(matches!(
            Metadata::new("graphit", &"v".repeat(MAX_VERSION_BYTES + 1)),
            Err(MetadataError::InvalidVersion)
        ));
    }

    #[test]
    fn rejects_empty_whitespace_controls_and_non_ascii_brand() {
        for brand in [
            "",
            " graphit",
            "graphit ",
            "graph it",
            "graph\tit",
            "graph\nit",
            "graph\rit",
            "graph\r\nit",
            "graph\0it",
            "graph\u{1f}it",
            "graph\u{7f}it",
            "gráphit",
        ] {
            assert!(matches!(
                Metadata::new(brand, "dev"),
                Err(MetadataError::InvalidBrand)
            ));
        }
    }

    #[test]
    fn rejects_empty_whitespace_controls_and_non_ascii_version() {
        for version in [
            "",
            " dev",
            "dev ",
            "de v",
            "de\tv",
            "de\nv",
            "de\rv",
            "de\r\nv",
            "de\0v",
            "de\u{1f}v",
            "de\u{7f}v",
            "dév",
        ] {
            assert!(matches!(
                Metadata::new("graphit", version),
                Err(MetadataError::InvalidVersion)
            ));
        }
    }

    #[test]
    fn uses_core_canonicalization_and_preserves_version_token() {
        let metadata = Metadata::new("Graphit-Cli", "v1.2.3+BUILD~preview")
            .expect("Core namespace and printable version token are valid");
        assert_eq!(metadata.brand().binary_name(), "graphit-cli");
        assert_eq!(metadata.version(), "v1.2.3+BUILD~preview");
    }

    #[test]
    fn admits_all_printable_ascii_version_bytes() {
        let version: String = (0x21_u8..=0x7e).map(char::from).collect();
        let metadata = Metadata::new("graphit", &version).expect("printable ASCII is admitted");
        assert_eq!(metadata.version(), version);
    }

    #[test]
    fn delegates_reserved_and_invalid_namespaces_to_core() {
        for brand in [
            "git",
            "GITIGNORE",
            "gitattributes",
            "gitmodules",
            "CON",
            "nul",
            "PRN",
            "AUX",
            "COM1",
            "lpt9",
            "-graphit",
            "graphit-",
            "graph_it",
            "graph.it",
            "graph/it",
        ] {
            assert!(matches!(
                Metadata::new(brand, "dev"),
                Err(MetadataError::InvalidBrand)
            ));
        }
    }

    #[test]
    fn errors_have_only_static_categories_and_messages() {
        for (error, display, debug) in [
            (
                MetadataError::InvalidBrand,
                "invalid build brand",
                "InvalidBrand",
            ),
            (
                MetadataError::InvalidVersion,
                "invalid build version",
                "InvalidVersion",
            ),
        ] {
            assert_eq!(error.to_string(), display);
            assert_eq!(format!("{error:?}"), debug);
            assert!(std::error::Error::source(&error).is_none());
        }
    }
}
