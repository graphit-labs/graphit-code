use super::{PackageError, PackageKind, invalid};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    v: u32,
    kind: PackageKind,
}

pub(super) fn write(output: impl Write, kind: PackageKind) -> Result<(), PackageError> {
    serde_json::to_writer(
        output,
        &Envelope {
            format: "graphit-native-package".into(),
            v: 1,
            kind,
        },
    )?;
    Ok(())
}
pub(super) fn read(input: impl Read) -> Result<PackageKind, PackageError> {
    let envelope: Envelope = serde_json::from_reader(input)?;
    if envelope.format != "graphit-native-package" || envelope.v != 1 {
        return Err(invalid("unsupported native package format or version"));
    }
    Ok(envelope.kind)
}
