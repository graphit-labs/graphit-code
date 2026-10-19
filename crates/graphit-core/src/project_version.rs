//! Version name validation and collision-free native Lance branch names.

use std::fmt::Write;

pub const DEFAULT_NAME: &str = "main";

pub fn validate(version: &str) -> Result<&str, String> {
    let version = version.trim();
    if version.chars().any(char::is_control) {
        return Err(format!(
            "invalid project version {version:?}: control characters are not allowed"
        ));
    }
    Ok(version)
}

pub fn lance_branch_name(version: &str, default_version: &str) -> String {
    if version.is_empty() || version == default_version {
        return String::new();
    }
    if valid_lance_branch(version) {
        return version.to_owned();
    }
    let mut encoded = String::with_capacity(2 + version.len() * 2);
    encoded.push_str("h-");
    for byte in version.bytes() {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn valid_lance_branch(name: &str) -> bool {
    name != "main"
        && !name.starts_with("h-")
        && !name.contains("..")
        && !name.ends_with(".lock")
        && name.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        })
}
