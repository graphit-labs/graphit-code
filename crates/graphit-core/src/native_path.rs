//! Unicode path data. A decoded path conveys no filesystem authority.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::io;
use std::path::{Component, Path, PathBuf};

/// Maximum UTF-8 size of one path string, before JSON escaping.
pub const MAX_PATH_BYTES: usize = 64 * 1024;

/// An absolute Unicode path without NUL or parent traversal.
/// Construction and decoding validate data, never filesystem authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePath(pub(crate) PathBuf);

impl NativePath {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        valid_path(&path)?;
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl Serialize for NativePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        valid_path(&self.0).map_err(serde::ser::Error::custom)?;
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NativePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(PathBuf::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

pub(crate) fn valid_native(path: &Path) -> io::Result<()> {
    let text = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path is not valid Unicode"))?;
    if text.len() > MAX_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path exceeds the UTF-8 byte limit",
        ));
    }
    if text.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    }
    Ok(())
}

pub(crate) fn absolute_input(path: &Path) -> io::Result<()> {
    valid_native(path)?;
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected an absolute path",
        ));
    }
    Ok(())
}

pub(crate) fn valid_path(path: &Path) -> io::Result<()> {
    absolute_input(path)?;
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains parent traversal",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> &'static str {
        if cfg!(windows) { "C:\\" } else { "/" }
    }

    #[test]
    fn strings_roundtrip_with_unicode_spaces_and_json_escapes() {
        for suffix in [
            "project with spaces/ação/日本語/🦀",
            "quote\"and\\slash\tline\n",
        ] {
            let path = PathBuf::from(format!("{}{suffix}", root()));
            let value = NativePath::new(path.clone()).unwrap();
            let encoded = serde_json::to_value(&value).unwrap();
            assert_eq!(
                encoded,
                serde_json::Value::String(path.to_str().unwrap().into())
            );
            assert_eq!(
                serde_json::from_value::<NativePath>(encoded).unwrap(),
                value
            );
        }
    }

    #[test]
    fn strings_reject_invalid_context_and_types() {
        for path in [
            PathBuf::new(),
            PathBuf::from("relative"),
            PathBuf::from(root()).join(".."),
            PathBuf::from(format!("{}nul\0", root())),
        ] {
            assert!(NativePath::new(path).is_err());
        }
        for value in [
            serde_json::json!(null),
            serde_json::json!(12),
            serde_json::json!([47, 112]),
            serde_json::json!({"path": "/p"}),
        ] {
            assert!(serde_json::from_value::<NativePath>(value).is_err());
        }
        assert!(serde_json::from_str::<NativePath>(r#""\ud800""#).is_err());
    }

    #[test]
    fn utf8_byte_limit_is_preserved() {
        let prefix = root();
        let value = NativePath::new(PathBuf::from(format!(
            "{prefix}{}",
            "x".repeat(MAX_PATH_BYTES - prefix.len())
        )))
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<NativePath>(&serde_json::to_vec(&value).unwrap()).unwrap(),
            value
        );
        assert!(
            NativePath::new(PathBuf::from(format!(
                "{prefix}{}",
                "x".repeat(MAX_PATH_BYTES)
            )))
            .is_err()
        );
        assert!(
            NativePath::new(PathBuf::from(format!(
                "{prefix}{}",
                "é".repeat(MAX_PATH_BYTES / 2)
            )))
            .is_err()
        );
    }

    #[test]
    fn non_unicode_input_is_rejected_without_replacement() {
        #[cfg(unix)]
        let path = {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_vec(b"/fixture/\xff".to_vec()))
        };
        #[cfg(windows)]
        let path = {
            use std::os::windows::ffi::OsStringExt;
            PathBuf::from(std::ffi::OsString::from_wide(&[67, 58, 92, 0xd800]))
        };
        #[cfg(any(unix, windows))]
        {
            assert!(
                NativePath::new(path.clone())
                    .unwrap_err()
                    .to_string()
                    .contains("Unicode")
            );
            // Internal constructors cannot bypass the wire validation.
            assert!(
                serde_json::to_string(&NativePath(path))
                    .unwrap_err()
                    .to_string()
                    .contains("Unicode")
            );
        }
    }
}
