---
type: specification
title: Captured Icebug publication metadata
description: Borrowed file references from the existing validated graph capture.
sources:
  - resource: crates/graphit-data/src/lib.rs
  - resource: crates/graphit-data/src/native_graph_metadata_tests.rs
---

`CapturedLocalGraph::schema()` checks the original operation and revalidates captured content before returning the existing borrowed `GraphSchema`. The view exposes two metadata methods:

| Method | Result |
| --- | --- |
| `schema_file()` | The validated graph-relative schema filename. |
| `referenced_files()` | Borrowed declared filenames in validated manifest order: `icebug.json`, schema, node tables in canonical label order, then relationship groups in canonical kind order with forward and reverse CSR members, indices before indptr. |

The removed `GraphGeneration` digest is not reconstructed from the namespace, revision or Core fingerprint. Native manifest format fields remain validated internally; they do not bind cursors or align other corpora. `relationships()` exposes forward logical members only; reverse storage appears in file references and supports incoming adjacency without adding logical relationships.

These methods allocate no corpus or owned manifest. The view cannot outlive its graph. References express declarations, and aliases accepted by the existing manifest parser remain repeated references. The parser validates `schema_file` as a relative path; capture does not require that file to exist. A caller opening a missing schema receives the existing captured IO error.

Open each referenced file through `CapturedLocalGraph::open_file()` and consume its retained Core stream with the same original operation. Metadata does not grant access, reconstruct a reader, execute schema text or certify publication. A borrowed view does not independently recheck authorization on each getter; the caller must keep genuine source admission and the original operation live during IO and before its final effect/response.

The native publication caller hashes actual `icebug.json` bytes and records that digest alongside its path, without copying a generation field into the descriptor. Its PROJECT/HubWrite/Broker STS admission, unique unpublished uploads, conditional descriptor/catalog commits and shutdown remain separate obligations. This Data API does not qualify an S3 or compiled CLI journey.

The focused metadata fixtures use the existing file-backed Parquet graph, including reverse CSR, captured stream opens, missing schema and allowed aliases. Existing schema tests cover changed content, cancellation and deadline. See the [borrowed schema contract](rust_icebug_schema.md) and [native root codec](remote_native_root.md).
