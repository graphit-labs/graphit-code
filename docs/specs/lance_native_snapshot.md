# Lance native snapshot opener

Status: SDK component implemented; remote Hub mounting remains pending.

`LanceModuleStore::open_native_snapshot(uri, schema, Arc<Session>, dataset_version)` opens the exact Lance SDK version with the caller's session. It returns the store and the SDK's actual `ManifestLocation`. The existing full Arrow schema equality check applies, and snapshots retain the dataset through `Arc`. SDK errors retain their cause. Opening reads does not create a missing version.

The caller configures the session's `ObjectStoreRegistry` using the official SDK provider interface. The opener does not discover AWS profiles, mutate environment variables, construct credentials or authorize the dataset. For remote use, the caller must provide an admitted S3 client, check its original authorization before I/O and delivery, and validate the published native root metadata separately. Manifest locations, versions and hashes are data.

The native S3 contract and returned implementation tasks are in the refinement packet (`../../.build/rust-hub-native-s3-opener-refinement-20261011/contract.md`; historical local artifact unavailable). Hub key/hash primitives are documented in [remote artifact storage](remote_artifact_storage.md). Native root/locator delivery `tsk-b86e`, Icebug range-reader delivery `tsk-f6ba`, Auth HubRead `tsk-6873` and the actual endpoint tasks still to be returned by Broker refinement `tsk-8cc0` remain pending. This component does not qualify STS or an installed remote Source/Knowledge query.

Tests in [lance_native_snapshot.rs](../../crates/graphit-data/tests/lance_native_snapshot.rs) use a real file-backed Lance dataset and a session-specific official file provider. They check two published versions, distinct manifest locations, retained old-reader rows, complete schema mismatch and the SDK cause for a missing version. This proves SDK session/snapshot behavior, not S3 authorization or performance gains.

Implementation: [lance_module_store.rs](../../crates/graphit-data/src/lance_module_store.rs). The existing local open/create/publication APIs and physical schema are preserved.

Linux qualification: `cargo test --locked --offline -p graphit-data --test lance_native_snapshot --test lance_module_store -j1` passed all eight affected tests (two new and six existing). Scoped Clippy for the library and those two targets passed with `--no-deps -- -D warnings`; rustfmt and diff checks passed. Logs are in the refinement packet directory. No remote mount or STS permission result is claimed.
