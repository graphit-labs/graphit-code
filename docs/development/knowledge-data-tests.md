---
title: Knowledge and Data test boundaries
description: Offline functional tests, native platform coverage and separate external qualification for documentary storage.
---

# Knowledge and Data test boundaries

Maintainers can run the Knowledge and Data contracts after preparing the locked
Cargo dependencies and native build toolchain described in
[Rust workspace checkout](rust-workspace.md). These packages' ordinary tests use
local SDK publications, private temporary roots and, for imported Knowledge,
signed HTTP peers on ephemeral loopback ports. They do not need Docker,
downloaded models, vendor accounts or an installed service.

```sh
cargo fetch --locked
cargo xtask cargo test --locked --offline -p graphit-knowledge
cargo xtask cargo test --locked --offline -p graphit-data
```

Cargo build parallelism and test parallelism are separate. These commands retain
the normal parallel test harness. The runner supplies the stack required by the
native SDK futures; it does not change a production request deadline. Reuse the
workspace's canonical build directory, as described in the
[Rust checks guide](rust-fast-guard.md).

## Contract coverage

The integration targets below complement the corresponding private unit tests
and documentation examples. An integration fixture proves the boundary it
actually exercises; a vector literal is not model inference, and a signed local
HTTP peer is not an authenticated vendor deployment.

| Journey | Knowledge integration targets |
|---|---|
| Parsing, authored fields and exact bytes | `document`, `document_authored_references`, `compilation_fingerprint`, `publication_metadata`, `native_document_mapping`, `project_document` |
| Capture, initial publication and file lifecycle | `desired_corpus`, `desired_reconciliation`, `authored_first_publication`, `native_index_publication`, `document_republication`, `source_stream`, `branch_materialization`, `ignore_policy` |
| References, domains and freshness | `references`, `native_reference_query`, `document_communities`, `published_lint`, `published_metadata_lint`, `staleness`, `staleness_projection`, `embedding_projection` |
| Read, export and imported sources | `document_source`, `preview`, `markdown_export`, `okf_export`, `imported_knowledge`, `imported_reference_query`, `imported_semantic`, `settings` |

| Boundary | Data integration targets |
|---|---|
| Documentary identity, publication and lifecycle | `knowledge_source_identity`, `knowledge_document_projection`, `knowledge_first_publication`, `knowledge_document_update`, `knowledge_source_stream`, `knowledge_branch_materialization`, `knowledge_retirement` |
| Native reads, references and retrieval | `knowledge_index_page`, `knowledge_module_snapshot`, `knowledge_reference_query`, `knowledge_schema_admission`, `knowledge_search`, `wiki_corpus` |
| Vectors and stream effects | `knowledge_embeddings`, `knowledge_embeddings_sdk`, `knowledge_embedding_stream`, `vector_distance` |
| Lance publication, pins and physical integrity | `lance_branch_store`, `lance_module_store`, `lance_module_creation`, `lance_integrity`, `lance_git_refs`, `lance_native_branch_clone`, `lance_native_clone`, `lance_native_metadata`, `lance_native_snapshot`, `lance_nullable_vector`, `lance_source_stream` |
| Lexical indexes and locators | `lance_code_fts_append`, `lance_fts_index`, `lance_fts_index_state`, `lance_fts_index_store`, `graph_search_sidecar`, `native_storage_uri` |

Data's AST corpus maintenance target has its own ownership and tests alongside
these documentary contracts.

Tests retain real Core captures, Auth admissions and Lance streams. Cancellation
of a pending producer is triggered only after its first poll is observed through
a channel. Deadline tests still exercise the real request clock. Writer-lock
tests assert the actual lock-stage error. File callbacks and joined channels
establish publication and concurrency ordering without assuming that an elapsed
delay proves a stage was reached.

Readers validate the selected operation's complete stream. A page read does not
validate unrelated bodies. A catalogue or export retains its full-operation
validation. Documentary publication and vector publication remain independent;
tests preserve committed prefixes, pending tails, compatible vectors and causal
errors rather than requiring a globally aligned generation. See
[selected page reads](../specs/knowledge_index_page.md) and the
[unified semantic corpus](../architecture/rust-knowledge-semantic-data.md).

## Platform and corruption fixtures

The SDK integration suites are enabled on Linux, macOS and Windows, matching the
production modules. Unix symlink cases remain Unix-specific. Linux additionally
tests actual non-Unicode directory entries and checkout descriptor retention
through `/proc/self/fd`. On macOS, APFS rejects creation of those raw filenames;
the same non-Unicode selection is still rejected as `InvalidInput` before I/O.
This does not claim physical invalid-filename support on APFS.

The BOM/CRLF fixture is a Rust literal with explicit escapes, independent of Git
newline conversion. Its 35 bytes and body range `27..35` remain the
[project preview contract](../architecture/rust-knowledge-project-preview.md).
Local corpus observations are native paths; tests do not assume they are
`file:` URLs.

Corrupt-corpus tests first publish with the real SDK, then invalidate a protobuf
tag while preserving its length and footer. They assert causal failure and no
reader repair. Arbitrary truncated metadata can panic inside the pinned Lance
SDK; those fixtures do not claim upstream protection that the SDK does not
provide.

## App consumers and external qualification

App's documentary suites exercise the compiled CLI, Source, Browser, export,
retirement, semantic admission and Live lifecycle. Source executable fixtures
wait for positive publication evidence before inspecting export contents, and
preserve the owned daemon's logs and shutdown. Knowledge protocol and process-custody
fixtures use Cargo-produced native peers selected by the test runner. Linux checkout
journeys still use Git with private configuration; provider peers do not require an
interpreter, vendor CLI or account.

The App Knowledge/Hub integration inventory is:

| Boundary | App integration targets |
|---|---|
| Compiled Ask, MCP and literal prompt/wire delivery | `knowledge_ask_ingress`, `knowledge_ask_stdio`, `knowledge_question` |
| Documentary reads, export and lifecycle | `knowledge_source`, `knowledge_source_cli`, `knowledge_export`, `knowledge_retirement`, `knowledge_semantic` |
| Native package, HTTP effects and checkout retirement | `native_install`, `native_install_executable`, `native_install_http`, `project_retirement` |

The matching private units cover foreground Knowledge publication, AskService and
semantic callbacks; authored resources; FILE publication; Hub publication and
unpublication; local and remote installation; catalogues, credentials and HTTP
lifecycle. Their fixtures retain actual Auth, Core, registry, Process and SDK
boundaries. ObjectStore's in-memory implementation supplies private storage to
those SDK tests. It does not replace admission or publication rules.

Compiled CLI waits drain stdout/stderr concurrently under a watchdog. A watchdog
failure kills and waits for the owned child. STDIO children retain kill-on-drop
protection while their live transport is controlled separately. Retirement tests
retain the fixture's release sender outside the combined operation/driver future,
so a failed observation releases the fixture and attempts bounded supervisor shutdown
before reporting failure.

The native HTTP lifecycle fixture holds a real registry kernel lock to occupy the
SDK blocking queue. A private `cfg(test)` signal observes the owner's actual
response-closed cancellation branch before the fixture releases that queue.
Shutdown and admission tests explicitly poll the wait or observe the owner's stop
signal. Expiry uses the actual policy interval or captured deadline. The observer is absent from release builds and adds no public API. The retained-worker and
partial-effect contract remains the
[Native installation ingress](../specs/native_installation_ingress.md).

`knowledge_export` and `knowledge_source` are enabled on Linux, macOS and Windows,
matching their product modules. Knowledge retirement remains Linux-only with its
current producer; compiled POSIX process and checkout fixtures keep their declared
platform boundary. Enabling a target is not native-platform execution evidence.

The public semantic regression
`null_vector_with_retained_identity_is_embedded_once_and_then_reused` removes
and restores the nullable vector field through the SDK in a private corpus.
It preserves the document and identity metadata, observes one real pipeline
callback to fill the missing vector, then zero callbacks and an unchanged
semantic generation on recapture. The callback returns controlled vectors;
this proves publication and reuse, not model inference. The Data stream suite
separately preserves rejection of a different value for an existing non-null
vector in `Attach`, along with explicit replacement and CAS behavior.

The semantic identifier-limit test uses a captured, hashed metadata descriptor
and a deliberately refused local transport. The cap below the identifier size
must fail before invoking or polling that transport; the exact cap must reach
the real transport error. It never executes inference. Real BF16/F16 inference,
installed model-server, external Broker/S3 and vendor protocol qualification
remain explicitly selected in the
[native qualification workflow](rust-fast-guard.md#native-qualification).

Local Linux results do not qualify macOS or Windows. The native CI matrix
executes the same ordinary suites on those platforms.
