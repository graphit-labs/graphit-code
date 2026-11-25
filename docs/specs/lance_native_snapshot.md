# Lance native snapshot opener

Status: SDK component implemented; remote Hub mounting remains pending.

`LanceModuleStore::open_native_snapshot(uri, schema, Arc<Session>, dataset_version)` opens the exact Lance SDK version with the caller's session. It returns the store and the SDK's actual `ManifestLocation`. The existing full Arrow schema equality check applies, and snapshots retain the dataset through `Arc`. SDK errors retain their cause. Opening reads does not create a missing version.

The caller configures the session's `ObjectStoreRegistry` using the official SDK provider interface. The opener does not discover AWS profiles, mutate environment variables, construct credentials or authorize the dataset. For remote use, the caller must provide an admitted S3 client, check its original authorization before I/O and delivery, and validate the published native root metadata separately. Manifest locations, versions and hashes are data.

The native S3 contract and returned implementation tasks are in the refinement packet (`../../.build/rust-hub-native-s3-opener-refinement-20261011/contract.md`; historical local artifact unavailable). Hub key/hash primitives are documented in [remote artifact storage](remote_artifact_storage.md). Native root/locator delivery `tsk-b86e`, Icebug range-reader delivery `tsk-f6ba`, Auth HubRead `tsk-6873` and the actual endpoint tasks still to be returned by Broker refinement `tsk-8cc0` remain pending. This component does not qualify STS or an installed remote Source/Knowledge query.

Tests in [lance_native_snapshot.rs](../../crates/graphit-data/tests/lance_native_snapshot.rs) use a real file-backed Lance dataset and a session-specific official file provider. They check two published versions, distinct manifest locations, retained old-reader rows, complete schema mismatch and the SDK cause for a missing version. This proves SDK session/snapshot behavior, not S3 authorization or performance gains.

Implementation: [lance_module_store.rs](../../crates/graphit-data/src/lance_module_store.rs). The existing local open/create/publication APIs and physical schema are preserved.

The earlier Linux qualification of `lance_native_snapshot` and `lance_module_store` passed all eight affected tests (two new and six existing). Their current focused command is `cargo xtask cargo test --locked --offline -p graphit-data --test integration -- lance_native_snapshot:: lance_module_store::`. Scoped Clippy for the library and those two original targets passed with `--no-deps -- -D warnings`; rustfmt and diff checks passed. Logs are in the refinement packet directory. No remote mount or STS permission result is claimed.

## Branch owner and repeated reads

`LanceBranchStore::into_target(self, encoded_branch)` consumes a newly opened
store and selects its initial Main observation. `capture_target(&self, ...)`
continues to refresh a retained store before selection. Both APIs validate Main
and selected branch against the expected schema; neither materializes a missing
named branch. Memory uses the consuming form after its guarded local or remote
open. A later operation opens the latest published head independently of older pins.

Exact named historical pins reuse their `BranchSnapshot` owner's native Lance
Session and current transport. Branch provenance is still read before and after
capture, and callers still compare it when continuing a cursor. The Session's
metadata/index caches do not grant authority or supply a global corpus registry.
Transport renewal continues to bind the same corpus and exact version with the
new admitted parameters. Physical manifest checks remain a separate responsibility
of the caller; native SDK reuse does not replace Core's physical observations.
