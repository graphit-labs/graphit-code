# Ladybug redistribution notice

`ladybug.LICENSE` retains the upstream MIT notice for the official Ladybug and
Rust binding. The published lbug0.22.1 crate omits its repository-level license;
its bundled third-party notices are retained separately by the packager.

Graphit builds the unmodified engine from `lbug-src` inside the official Cargo
crate. Cargo.lock records the exact registry source and checksum; there are no
separate native release assets or SDK distribution. The build retains only the
ELF symbol isolation necessary to link bundled ANTLR/C dependencies with the
rest of Graphit. See [Ladybug build](../../../docs/specs/rust_ladybug_sdk.md).
