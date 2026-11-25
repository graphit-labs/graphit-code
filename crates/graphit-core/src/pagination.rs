//! Opaque pagination bound to a canonical JSON query.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write;

pub const DEFAULT_PAGE_SIZE: i64 = 20;
pub const MAX_PAGE_SIZE: i64 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec<T = serde_json::Value> {
    #[serde(default)]
    pub page_size: i64,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub default_page_size: i64,
    pub bind: T,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub offset: usize,
    pub page_size: usize,
    pub fetch_limit: usize,
    total: usize,
    fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub results: Vec<T>,
    pub next_cursor: String,
}

#[derive(Serialize)]
struct Binding<'a, T> {
    bind: &'a T,
    page_size: i64,
    total: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    offset: usize,
    fingerprint: String,
}

pub fn open<T: Serialize>(spec: &Spec<T>) -> Result<Window, String> {
    if spec.total < 0 {
        return Err("top_k/limit cannot be negative".into());
    }
    if spec.page_size < 0 {
        return Err("page_size cannot be negative".into());
    }
    let mut size = spec.page_size;
    if size == 0 {
        size = if spec.default_page_size > 0 {
            spec.default_page_size.min(MAX_PAGE_SIZE)
        } else {
            DEFAULT_PAGE_SIZE
        };
        if spec.total > 0 {
            size = size.min(spec.total);
        }
    }
    if size > MAX_PAGE_SIZE {
        return Err(format!("page_size cannot exceed {MAX_PAGE_SIZE}"));
    }
    let mut binding = serde_json::to_value(&Binding {
        bind: &spec.bind,
        page_size: size,
        total: spec.total,
    })
    .map_err(|error| format!("encoding pagination binding: {error}"))?;
    binding.sort_all_objects();
    let bytes = serde_json::to_vec(&binding).map_err(|error| error.to_string())?;
    let mut fingerprint = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(fingerprint, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let offset = if spec.cursor.is_empty() {
        0
    } else {
        let (offset, binding) = decode(&spec.cursor)?;
        if binding != fingerprint {
            return Err("cursor does not belong to this query".into());
        }
        offset
    };
    let total = spec.total as usize;
    if total > 0 && offset >= total {
        return Err("cursor is past the top_k/limit boundary".into());
    }
    if offset > (isize::MAX as usize).saturating_sub(size as usize + 1) {
        return Err("pagination cursor offset is too large".into());
    }
    let take = if total > 0 {
        (size as usize).min(total - offset)
    } else {
        size as usize
    };
    let fetch = offset + take;
    Ok(Window {
        offset,
        page_size: take,
        fetch_limit: fetch + usize::from(total == 0 || fetch < total),
        total,
        fingerprint,
    })
}

fn decode(cursor: &str) -> Result<(usize, String), String> {
    let invalid = || "invalid pagination cursor".to_owned();
    let bytes = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| invalid())?;
    let value: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if value.fingerprint.is_empty() {
        return Err(invalid());
    }
    Ok((value.offset, value.fingerprint))
}

impl Window {
    /// Consume a ranked prefix. The result borrows it instead of copying each row.
    pub fn finish<'a, T>(&self, prefix: &'a [T]) -> Page<&'a T> {
        if self.offset >= prefix.len() {
            return Page {
                results: Vec::new(),
                next_cursor: String::new(),
            };
        }
        let end = (self.offset + self.page_size).min(prefix.len());
        Page {
            results: prefix[self.offset..end].iter().collect(),
            next_cursor: self.next_cursor(end, end < prefix.len()),
        }
    }

    /// Complete rows already fetched starting at offset, without rebuilding a prefix.
    pub fn finish_fetched<'a, T>(&self, fetched: &'a [T]) -> Page<&'a T> {
        let end = self.page_size.min(fetched.len());
        Page {
            results: fetched[..end].iter().collect(),
            next_cursor: self.next_cursor(self.offset + end, end < fetched.len()),
        }
    }

    fn next_cursor(&self, next: usize, has_more: bool) -> String {
        if !has_more || (self.total > 0 && next >= self.total) {
            return String::new();
        }
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&Cursor {
                offset: next,
                fingerprint: self.fingerprint.clone(),
            })
            .expect("cursor only contains finite scalar values"),
        )
    }
}
