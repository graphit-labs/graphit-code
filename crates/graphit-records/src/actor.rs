pub(crate) fn valid_identifier(value: &str) -> bool {
    value.len() == 26
        && ulid::Ulid::from_string(value).is_ok_and(|identifier| identifier.to_string() == value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_accepts_only_canonical_ulid128() {
        for identifier in [
            ulid::Ulid::nil(),
            ulid::Ulid::from(1u128),
            ulid::Ulid::from(u128::MAX),
        ] {
            assert!(valid_identifier(&identifier.to_string()));
        }
        let canonical = ulid::Ulid::from(u128::MAX).to_string();
        for invalid in [
            String::new(),
            "a".repeat(64),
            canonical.to_lowercase(),
            format!(" {canonical}"),
            "8ZZZZZZZZZZZZZZZZZZZZZZZZZ".into(),
            "IIIIIIIIIIIIIIIIIIIIIIIIII".into(),
            canonical[..25].to_owned(),
        ] {
            assert!(!valid_identifier(&invalid));
        }
    }
}
