---
title: Lance branch delta producer
type: reference
references:
  - target: {type: task, id: tsk-994a}
    relation: implements
  - target: {type: task, id: tsk-8a17}
    relation: implements
  - target: {type: task, id: tsk-8a70}
    relation: derived_from
  - target: {type: task, id: tsk-9e41}
    relation: uses
  - target: {type: task, id: tsk-72ce}
    relation: supports
  - target: {type: task, id: tsk-eb40}
    relation: implements
---

# Records branch delta producer

The Main module `graphit_data::lance_branch_store` owns Lance SDK 14.0.2 branch snapshots and incremental publication. `LanceBranchStore::open(uri, expected_schema, params)` validates the pinned SDK URI selector, obtains its atomic commit handler, and installs that same handler when opening the dataset. It accepts local paths and the SDK selectors file, file-object-store, s3, gs, az, abfss, memory, oss, tos, shared-memory, goosefs and cos. Unsupported schemes are rejected before opening. There is no arbitrary Dataset/handler constructor.

`capture_target(None)` captures the current default head. A named encoded Core branch needs valid BranchContents and a readable SDK head. Only RefNotFound yields a default snapshot inherited for reads; those reads use the normal SDK Scanner and do not create a branch. An inherited snapshot refuses patches until `materialize(self)` forks its captured default version N. The Clone child starts at N even when default has since advanced, and the first delta publishes N+1. SDK versions are storage fences, independent of record revisions.

Branch creation writes a Clone manifest and then reference metadata. Metadata is the truth of the branch reference. Fork and delta are separate operations: a valid branch remains after a failed delta. Scoped observations distinguish valid metadata plus readable head (CreationConflict), readable orphan manifest plus RefNotFound (BootstrapIncomplete), and inconclusive evidence (BootstrapUnknown). The original SDK error and observation errors remain available. The producer never adopts a winner, retries creation, deletes orphan data or repairs references. A new service operation must obtain fresh admission and fences.

## Historical reads and consumer provenance

`BranchSnapshot::at_version(version)` reads an exact positive version of its
captured physical branch. It preserves `inherited_target` on an inherited
receiver, even if that child has since materialized; historical read does not
materialize the target or enable writes.

`at_version_with_provenance(version)` returns
`(BranchSnapshot, Option<BranchProvenance>)`. Default and still-inherited
receivers retain their physical pin and return no named proof. For a named
branch, the SDK captures its reference identity before and after fresh data
capture and rejects a changed reference. Consumers retain and compare the
returned proof for continuation, including detecting namespace replacement
after retire/recreate.

`at_inherited_version(version)` returns `(BranchSnapshot, BranchProvenance)`
for a materialized child whose direct parent is default and whose fork satisfies
`0 < version <= fork_version`. It validates the child reference around data
capture. The returned default pin retains the child's inherited target and
refuses parent writes; its proof identifies that child, not an independently
selected parent. This is not arbitrary ancestry traversal.

`BranchProvenance` is SDK namespace identity that consumers can serialize and
compare, not a signature, authority or GC-retention lease. The consumer still
derives the current URI and version/branch from its actual admission and verified
scope, retains the Original, and rechecks authorization and captures before
delivery. Cursor data never authorizes another URI or branch. These APIs add no
snapshot cache or lease.

On 2026-10-24 the three Records `task_store get_pages::` integration tests passed
with zero failures and 59 filtered tests, in 2.42 seconds of harness runtime.
Their consumer coverage includes retained snapshots across writes, inherited
parent writes and child materialization, post-fork forgery, same-public-generation
namespace replacement after retire/recreate, and closed nested provenance.
The [Records qualification](native_task_store.md#selected-get-linux-qualification)
records the command, build/total times and separate 10-test protocol result.
This qualifies the selected Records consumer paths, not a new execution of the
entire Data SDK suite. On 2026-10-25, [eight App backend selections](rust_task_mcp.md#task-get-app-backend-qualification)
passed with zero failures for bounded encoding, confirmed writes, actual
MCP/native HTTP, local managed-role control and cancellation/delivery. Records
was not rerun, and no new full Data SDK run is implied. The principal executable
CLI case then passed completely in [PASS02 with a private corrected caller](rust_task_mcp.md#task-get-executable-cli-qualification),
including its unchanged `stale_main` gate. FAIL01 remains historical evidence;
the earlier parallel two-Root CLI pass was not repeated. Browser cookie/socket
and React qualification remain pending;
installed App delivery is not established. Historical SDK/publication evidence
below retains its original scope; Cargo totals are not product RAM, RSS or
latency measurements.

## Public observed capture and complete overwrite

`BranchSnapshot::capture_observed(uri, encoded_branch, params)` captures the selected physical branch and its complete observed Arrow schema. Unlike the schema-first `LanceBranchStore::open` selector, it does not require a named branch to have the default branch's vector width or metadata. A missing named branch still returns the captured default pin with `inherited_target`; capture does not materialize it. Caller-derived URI and encoded branch remain data selectors, not Auth or Original authority.

`BranchSnapshot::prepare_overwrite(Vec<RecordBatch>)` prepares a complete image with the first batch's schema. Every batch must have that schema; an empty vector is `EmptyPatch` and mixed schemas are `SchemaMismatch`. To publish zero rows, pass `vec![RecordBatch::new_empty(schema)]` explicitly. This preserves a complete schema for the empty corpus, including a changed vector width. An inherited target rejects preparation until the caller separately materializes its captured version. The API keeps its existing batches signature and has no implicit empty-schema fallback.

The opaque public `PreparedOverwrite` retains the actual captured SDK handle and transaction. Save `recovery_key()` before consuming it with `commit()`. Preparation may write uncommitted files but does not advance the published version. Commit uses SDK Overwrite, zero retries and no automatic cleanup; SDK strict overwrite publishes only the captured base plus one, without rebasing a stale image. A failure retains the real SDK cause and recovery key for the existing scoped `resolve` API. Neither preparation, commit nor recovery manufactures caller authority: consumers retain and recheck actual admission, capture and Original through delivery. Materialization is a separate effect, and an indeterminate SDK result is not a rollback promise.

The external integration cases in [lance_branch_store.rs](../../crates/graphit-data/tests/lance_branch_store.rs) exercise named/default widths and metadata, explicit empty overwrite, unpublished preparation, retained older pins, stale CAS and recovery, plus invalid empty/mixed/inherited inputs. The existing [Knowledge embedding consumer](../../crates/graphit-data/tests/knowledge_embeddings.rs) supplies explicit empty batches during `ReplaceComplete` when the final source slug disappears; its zero-row reindex also changes width from two to three. These fixtures qualify this publication primitive only; AST NN and App composition remain separate consumers.

Linux qualification for `tsk-eb40` passed the two external public API cases (`../../.build/rust-data-branch-public/public-01.log`; historical local artifact unavailable) and the existing final-source-slug-empty consumer case (`../../.build/rust-data-branch-public/replace-empty-01.log`; historical local artifact unavailable): three distinct tests, zero failures or ignored cases. Scoped Clippy (`../../.build/rust-data-branch-public/strict-01.log`; historical local artifact unavailable) passed for Data lib and those two integration targets with `--no-deps -- -D warnings`; the two changed Rust files passed `rustfmt --check`. This visibility promotion changes no publication implementation, dependency or consumer authority.

## Incremental delta and recovery

`prepare_merge(batches, join_columns)` accepts exact physical schemas and nonempty distinct UTF8 key columns. Key values cannot be null, and composite patch keys must be unique across all batches. Zero rows return EmptyPatch before SDK I/O. Rust validates only the supplied patch; it does not load or deduplicate the target corpus. SDK MergeInsert uses UpdateAll, InsertAll, Keep, SourceDedupeBehavior::Fail and use_index(true). Heads, immutable history, audit and references may be supplied together and publish in one transaction. Consumers enforce immutable identifiers/history and their own policy.

`PreparedDelta::commit(self)` retains the same Arc Dataset/session cache, original transaction and affected rows. A narrow handler guard checks the SDK branch_location path, physical branch and checked base+1 before both commit and commit_after. Every other public CommitHandler method delegates explicitly, including resolver/list/absence policy, predecessor identities and propagation of post-publication errors. The real handler retains conditional create or rename-if-not-exists. max_retries(0) and skip_auto_cleanup(true) prevent producer retries and automatic cleanup. Initial SDK rebase work or preparatory files can still occur, but a higher version cannot pass the publication guard.

Save a serialized recovery key before commit. The private serde fields identify exact corpus URI, physical branch, base version and transaction UUID. `BranchSnapshot::resolve(&key)` validates the receiver's URI and physical branch independently before I/O, refuses inherited snapshots and checks a nonempty UUID and base+1 overflow. The exact branch transaction UUID yields Applied or Conflicted; a present version without a transaction is MissingTransaction. Only SDK DatasetNotFound/VersionNotFound from the scoped version-manifest path, under the effective definitive delegate policy and without unknown-commit status, gives Unresolved. A missing external transaction object under a readable manifest preserves its SDK read error; it cannot be reported as an absent version. Bootstrap checkout absence is evaluated at its manifest-only boundary; generic nested I/O NotFound is not accepted as evidence. SDK transport/unknown errors retain their causes. Dropping or cancelling a commit future does not establish rollback; recovery performs no retry or cleanup.

The service caller derives corpus URI below its verified project scope and passes the already encoded VersionContext branch (convert an empty lance_branch to None). It classifies the operation internally and revalidates original authorization, version binding, admission/fences, cancellation and deadline before publication. Paths, snapshots and recovery keys grant no authority. Forks copy record state, revisions, claims, runs and epochs as stored; they do not reset Running rows, adopt dispatch or invoke a scheduler. One Task/one Coordination per actor is scoped to the native version/branch store; generic SDK storage does not enforce claim policy. These producers do not integrate Task/Memory/Dream consumers.

A Scanner can project and filter directly in SDK. Indexed MergeInsert lookup requires every join key to have an exact equality index covering fragments; absent or incomplete indices can trigger an SDK scan. Consumer/Runtime owners maintain indices. Retained Arc handles share SDK caches; they are not GC pins. External maintenance remains responsible for snapshot retention and garbage collection.

`prepare_scalar_append(column, name)` prepares a compatible native BTree on the captured dataset's column and schema. One call indexes all uncovered fragments and merges comparable existing segments through the SDK; consolidation reads index entries rather than rescanning stored Source bodies. Normal SDK scans include unindexed fragments and preserve updates and deletions, so consumers can publish Source and Graph before scheduling this maintenance. A covered, consolidated index is a no-op; a fully covered inherited snapshot also remains a read-only no-op without materialization. A name bound to another column or index type is an explicit error. Preparation does not publish a version: the resulting `PreparedIndex` retains strict branch/base+1 CAS and the existing recovery key. Replacement metadata is published atomically, preserving committed input files for older manifests; only this preparation's private intermediate merge files are removed. Physical reclamation remains with the existing SDK cleanup and its snapshot/read-lease policy. A lower active segment count does not prove that old index directories are collectible. The filesystem [scalar append cases](../../crates/graphit-data/tests/lance_scalar_append.rs) qualify deferred lookup, batched coverage, consolidation, named/default isolation, retained history, no-op and competing commit recovery; the shared helper is also exercised by the existing [code FTS cases](../../crates/graphit-data/tests/lance_code_fts_append.rs).

The exact coordinated dependency fragment disables default cloud features for lance, lance-table and lance-io. Those selector names are admitted by the safe-selector check, but unavailable providers still return their SDK errors; no cloud feature or cloud execution is qualified here. SDK memory creates a fresh store for each provider opening, so the memory journey uses a public ObjectStoreParams wrapper retaining one real InMemory store across creation and open, with the factory-selected conditional handler unchanged.

Pinned direct dependencies are lance/lance-table/lance-core/lance-io 14.0.2, Arrow array/schema 58.3.0, object_store 0.14.2, async-trait 0.1.92, url 2.5.8, serde 1.0.229 and futures 0.3.34. Test fixtures use tokio 1.53.2, tempfile 3.27.0, serde_json 1.0.151 and the sibling `lance_module_store` module. ObjectStore public wrappers inject transport failures around real SDK writes and readback; they do not replace the commit protocol.

The local SDK selects conditional put on Unix and atomic rename on Windows.
The transport fixture intercepts both publication operations and counts the
actual injected manifest fault. A pre-publication failure keeps the original
branch and unresolved recovery key; a lost acknowledgement after publication
keeps the committed state and is resolved through a fresh read. An explicit
official `RenameCommitHandler` fixture also exercises that transport on Unix.

AST corpus cleanup is qualified by a reduction in obsolete versions, unchanged
branch heads and rows, retained active snapshots, and working FTS/vector reads
on default and named branches. The SDK may discard a fork manifest while
retaining files referenced by descendants; the number of retained manifests
and the number of internal maintenance commits are not fixed test contracts.

Portable qualification commands:

```text
cargo xtask cargo test --locked --offline -p graphit-data --test integration -- lance_module_store:: lance_branch_store::
cargo xtask cargo test --locked --offline -p graphit-data --doc -- lance_
cargo xtask cargo clippy --locked --offline -p graphit-data --lib --test integration -- -D warnings
```

The original private qualification (`tsk-8a70`) executed eight SDK fixtures and one compile-only no_run Rustdoc, strict Clippy and fmt. Main qualification for this promotion passed the eight SDK fixtures, its compile-only Rustdoc, formatting of the promoted Rust paths and strict Clippy of the affected targets. Logs are recorded separately. Linux filesystem and memory fixtures are the executed targets; cloud selectors, Windows and macOS require their own execution before those platforms can be claimed. Root integrated the global manifests/lock in `tsk-8dc3` and coordinates the SDK cache. Data `--all-targets` Clippy failed in the separate `wiki_corpus` test while Knowledge exports and Auth/Hub dependencies were still pending; this broad check did not pass. These modules confer no consumer authorization.

The compile-checked Rustdoc example in crates/graphit-data/src/lance_branch_store.rs uses this workflow:

```rust
use std::sync::Arc;
use arrow_array::{RecordBatch, StringArray};
use arrow_schema::{Schema, Field, DataType};
use graphit_data::lance_branch_store::{LanceBranchStore, DeltaResolution};

async fn workflow() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Utf8, false)]));
    let store = LanceBranchStore::open("/authorized/task.lance", schema.clone(), None).await?;
    let branch = store.capture_target(Some("encoded-feature")).await?.materialize().await?;
    let patch = RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec!["event-1"]))])?;
    let pending = branch.prepare_merge(vec![patch], vec!["id".into()]).await?;
    let saved = serde_json::to_vec(pending.recovery_key())?;
    let next = pending.commit().await?;
    let key = serde_json::from_slice(&saved)?;
    assert!(matches!(next.resolve(&key).await?, DeltaResolution::Applied {..}));
    Ok(())
}
```

## Preparação por stream de branch

`prepare_overwrite_stream(schema, stream)` prepara uma imagem inteira usando
`InsertBuilder::execute_uncommitted_stream` do Lance13. O schema explícito
pode mudar em relação ao pin; o schema declarado pelo stream e cada batch
precisam coincidir com ele. `prepare_merge_stream(stream, join_columns,
delete_unmatched)` exige o schema completo do pin e colunas de chave UTF8
existentes, distintas e não vazias. As duas APIs recusam snapshots herdados
antes do SDK; a materialização continua uma operação separada.

Essas entradas recebem o `SendableRecordBatchStream` real reexportado em
`graphit_data::lance_module_store`, junto com `RecordBatchStreamAdapter`.
`graphit_data::lance_branch_store` reexporta `Expr`, `col` e `lit` reais para
o caller construir o predicado de exclusão sem uma dependência própria de
DataFusion. Não há tipo de stream ou expressão compatível inventado.

Merge usa o executor `execute_uncommitted(stream)` do SDK, com UpdateAll,
InsertAll, source dedupe Fail, use_index(true) e conflict_retries(0).
`delete_unmatched: None` usa Keep. `Some(expr)` usa DeleteIf sobre linhas do
target sem correspondente no source; o predicado limita o que pode desaparecer.
Por exemplo, `Some(col("path").eq(lit("removed.rs")))` com um batch explícito
vazio remove somente esse path existente. Um delta com UpdateAll/InsertAll e
Keep sozinho não apaga paths ausentes no source.

A validação percorre os batches sem guardar o corpus. NULL nas colunas de
join causa erro durante consumo; um conjunto de chaves de join rejeita
repetições, inclusive entre batches, sem conservar corpos ou batches.
O SDK Fail permanece habilitado para múltiplas linhas source que
correspondam ao mesmo registro target; sozinho ele aceita chaves novas
repetidas, por isso não substitui a validação do patch. Erros tardios do source e schemas incompatíveis de batches
retornam a causa pela cadeia SDK, sem devolver uma preparação publicável.
Schema declarado incompatível e colunas de join inválidas são erros diretos
`SchemaMismatch` e `InvalidJoinColumns`. Um stream sem batches é erro; imagens
ou deltas de zero linhas usam um batch explícito com schema. O método baseado
em `Vec` conserva seu contrato de `EmptyPatch` para zero linhas.

Após EOF as APIs retornam `PreparedOverwrite` ou `PreparedDelta` existentes.
Chaves UUID/base/branch reais, publicação uma vez, strict CAS sem retries e
recuperação permanecem iguais. Preparação pode deixar arquivos não publicados;
a API não promete rollback, cleanup, atomicidade entre datasets ou autorização.
O caller mantém e revalida seus controles, conserva a chave antes do commit e
aguarda efeitos iniciados. Noop é uma decisão do consumidor antes de preparar.

O target [lance_source_stream.rs](../../crates/graphit-data/tests/lance_source_stream.rs)
exercita SDK real para Create/Overwrite/Keep/DeleteIf, source vazio versus NULL,
zero linhas explícitas, schemas, NULL e duplicação de chave, falha tardia sem
publicação, pins retidos, CAS stale e recuperação. Comandos focais:

~~~text
cargo xtask cargo test --locked --offline -p graphit-data --test integration -- lance_source_stream::
cargo xtask cargo clippy --locked --offline -p graphit-data --lib --test integration --no-deps -- -D warnings
~~~

A integração do produtor por FILE com o supervisor Ast pertence ao consumidor
App; estas fixtures qualificam somente as preparações Data. Resultados de
execução pertencem ao Task `tsk-994a`; Windows/macOS não foram executados.
