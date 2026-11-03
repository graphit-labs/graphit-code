//! Read a service-issued opaque cursor without exposing its internal fields as CLI options.
use serde_json::Value;
use std::{
    io::{self, Read},
    path::Path,
};

pub fn parse(text: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if !value.is_object() {
        return Err("cursor must be the exact JSON object returned by the service".into());
    }
    Ok(value)
}

pub fn read_text_file(path: &Path, max_bytes: usize) -> io::Result<String> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cursor file must be a regular file",
        ));
    }
    #[cfg(unix)]
    let file = std::fs::File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    #[cfg(not(unix))]
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cursor file must be a regular file",
        ));
    }
    let bound = u64::try_from(max_bytes)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut bytes = Vec::new();
    file.take(bound).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cursor exceeds the request input limit",
        ));
    }
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}

pub fn read_file(path: &Path, max_bytes: usize) -> io::Result<Value> {
    parse(&read_text_file(path, max_bytes)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_cursor_file_preserves_fields_and_rejects_invalid_or_oversized_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cursor.json");
        let text = r#"{"id":"ação \"","pin":[1,2],"optional":null}"#;
        std::fs::write(&path, text).unwrap();
        assert_eq!(read_file(&path, text.len()).unwrap(), parse(text).unwrap());
        assert_eq!(
            read_file(&path, text.len() - 1).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(parse("[]").is_err());
        assert!(parse("null").is_err());
        std::fs::write(&path, [0xff]).unwrap();
        assert!(read_file(&path, 8).is_err());
        assert!(read_text_file(dir.path(), 8).is_err());
        #[cfg(unix)]
        {
            let fifo = dir.path().join("cursor.fifo");
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&fifo)
                    .status()
                    .unwrap()
                    .success()
            );
            assert_eq!(
                read_text_file(&fifo, 8).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
