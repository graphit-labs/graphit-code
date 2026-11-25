//! Deterministic, complete JSON/TOON documents from an explicitly supplied public value.
//!
//! Callers own projection, secret redaction, response envelopes and sinks. This module
//! never consults the environment or omits fields. Object keys sort lexicographically;
//! array order stays intact. TOON uses specification 4.1 with its default comma delimiter
//! and two-space indentation. Integral floats (including negative zero) normalize to
//! their numeric value; JSON number spelling and floating-point sign bits are not a
//! TOON data contract.
//!
//! Shape and compact JSON input size are checked before rendering. JSON output has a
//! bounded, fallibly reserved buffer. TOON's upstream encoder materializes a complete
//! string and intermediate trees: its output limit controls publication, **not peak
//! allocation**. Input limits bound the admitted document, not allocator overhead or
//! all transient memory. No partial document escapes on error.

use std::{cell::Cell, fmt, io};

use serde::{
    Serialize, Serializer,
    ser::{Error as _, SerializeMap, SerializeSeq},
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Toon,
}

/// Required caller budgets. The root has depth zero; every value, including a
/// container, counts as one node. Keys count toward input bytes, not node count.
/// Byte budgets may be zero; depth must be at most 64 and node count nonzero.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}

/// A complete document. Payload access is explicit and Debug never prints it.
pub struct EncodedDocument {
    text: String,
    format: OutputFormat,
}

impl EncodedDocument {
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub fn into_string(self) -> String {
        self.text
    }
    pub fn format(&self) -> OutputFormat {
        self.format
    }
    pub fn len(&self) -> usize {
        self.text.len()
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

impl fmt::Debug for EncodedDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedDocument")
            .field("format", &self.format)
            .field("bytes", &self.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputError {
    InvalidLimits,
    Depth,
    Nodes,
    InputBytes,
    OutputBytes,
    Encoding,
    Allocation,
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid structured output limits",
            Self::Depth => "structured input exceeds depth limit",
            Self::Nodes => "structured input exceeds node limit",
            Self::InputBytes => "structured input exceeds byte limit",
            Self::OutputBytes => "structured output exceeds byte limit",
            Self::Encoding => "structured output encoding failed",
            Self::Allocation => "structured output allocation failed",
        })
    }
}

impl std::error::Error for OutputError {}

pub fn encode(
    value: &Value,
    format: OutputFormat,
    limits: Limits,
) -> Result<EncodedDocument, OutputError> {
    if limits.max_depth > 64 || limits.max_nodes == 0 {
        return Err(OutputError::InvalidLimits);
    }
    visit(value, 0, &mut 0, limits)?;
    // Key order does not change serialized byte length. This pass needs neither
    // sorted-key allocations nor a retained serialized copy of the input.
    let mut counter = ByteCounter {
        bytes: 0,
        limit: limits.max_input_bytes,
        exceeded: false,
    };
    if serde_json::to_writer(&mut counter, value).is_err() {
        return Err(if counter.exceeded {
            OutputError::InputBytes
        } else {
            OutputError::Encoding
        });
    }
    let allocation_failed = Cell::new(false);
    let canonical = Canonical {
        value,
        allocation_failed: &allocation_failed,
    };
    let text = match format {
        OutputFormat::Json => {
            let mut buffer = BoundedBuffer {
                bytes: Vec::new(),
                limit: limits.max_output_bytes,
                failure: None,
            };
            if serde_json::to_writer(&mut buffer, &canonical).is_err() {
                return Err(buffer
                    .failure
                    .unwrap_or_else(|| encoding_error(&allocation_failed)));
            }
            String::from_utf8(buffer.bytes).map_err(|_| OutputError::Encoding)?
        }
        OutputFormat::Toon => {
            let text = toon_format::encode_default(&canonical)
                .map_err(|_| encoding_error(&allocation_failed))?;
            if text.len() > limits.max_output_bytes {
                return Err(OutputError::OutputBytes);
            }
            text
        }
    };
    Ok(EncodedDocument { text, format })
}

fn encoding_error(allocation_failed: &Cell<bool>) -> OutputError {
    if allocation_failed.get() {
        OutputError::Allocation
    } else {
        OutputError::Encoding
    }
}

fn visit(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    limits: Limits,
) -> Result<(), OutputError> {
    if depth > limits.max_depth {
        return Err(OutputError::Depth);
    }
    *nodes = nodes.checked_add(1).ok_or(OutputError::Nodes)?;
    if *nodes > limits.max_nodes {
        return Err(OutputError::Nodes);
    }
    match value {
        Value::Array(values) => {
            for child in values {
                visit(child, depth + 1, nodes, limits)?;
            }
        }
        Value::Object(values) => {
            for child in values.values() {
                visit(child, depth + 1, nodes, limits)?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct Canonical<'a> {
    value: &'a Value,
    allocation_failed: &'a Cell<bool>,
}

impl Serialize for Canonical<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.value {
            Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&Canonical {
                        value,
                        allocation_failed: self.allocation_failed,
                    })?;
                }
                sequence.end()
            }
            Value::Object(values) => {
                let mut entries = Vec::new();
                entries.try_reserve_exact(values.len()).map_err(|_| {
                    self.allocation_failed.set(true);
                    S::Error::custom("structured output allocation failed")
                })?;
                entries.extend(values.iter());
                entries.sort_unstable_by_key(|(key, _)| *key);
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(
                        key,
                        &Canonical {
                            value,
                            allocation_failed: self.allocation_failed,
                        },
                    )?;
                }
                map.end()
            }
            _ => self.value.serialize(serializer),
        }
    }
}

struct ByteCounter {
    bytes: usize,
    limit: usize,
    exceeded: bool,
}

impl io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.bytes.checked_add(bytes.len()) {
            Some(total) if total <= self.limit => {
                self.bytes = total;
                Ok(bytes.len())
            }
            _ => {
                self.exceeded = true;
                Err(io::ErrorKind::FileTooLarge.into())
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
    failure: Option<OutputError>,
}

impl BoundedBuffer {
    fn reserve(&mut self, total: usize) -> io::Result<()> {
        if total > self.bytes.capacity() {
            // Amortize tiny serializer writes without requesting capacity above
            // the publication budget. Allocator rounding remains allocator-owned.
            let target = total.max(self.bytes.capacity().saturating_mul(2).min(self.limit));
            self.bytes
                .try_reserve_exact(target - self.bytes.len())
                .map_err(|_| {
                    self.failure = Some(OutputError::Allocation);
                    io::Error::from(io::ErrorKind::OutOfMemory)
                })?;
        }
        Ok(())
    }
}

impl io::Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(total) = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|total| *total <= self.limit)
        else {
            self.failure = Some(OutputError::OutputBytes);
            return Err(io::ErrorKind::FileTooLarge.into());
        };
        self.reserve(total)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn fallible_buffer_reports_allocation_and_retains_no_partial_document() {
        let mut buffer = BoundedBuffer {
            bytes: Vec::new(),
            limit: usize::MAX,
            failure: None,
        };
        assert!(buffer.reserve(usize::MAX).is_err());
        assert_eq!(buffer.failure, Some(OutputError::Allocation));
        assert!(buffer.bytes.is_empty());
    }

    #[test]
    fn buffer_does_not_accept_bytes_beyond_budget() {
        let mut buffer = BoundedBuffer {
            bytes: Vec::new(),
            limit: 3,
            failure: None,
        };
        buffer.write_all(b"abc").unwrap();
        assert!(buffer.write_all(b"d").is_err());
        assert_eq!(buffer.failure, Some(OutputError::OutputBytes));
        assert_eq!(buffer.bytes, b"abc");
    }
}
