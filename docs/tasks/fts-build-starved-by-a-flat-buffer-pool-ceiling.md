---
title: O build de FTS morria de fome com um teto de buffer pool fixo em 1 GiB
status: done
created: 2026-08-19
updated: 2026-08-19
tags: [ast, search, ladybug, resources, bug-fix]
---

# O build de FTS morria de fome com um teto de buffer pool fixo em 1 GiB

## Objective

`graphit ast index .` sobre um corpus grande (39.429 arquivos, ~2,5 M entidades) abortava com

```
! Completed with 1 error(s) out of 39429 files
  › Timing: discover 0.14s, hash 0.06s, parse 0.00s, write 262.10s
  › ! Write errors: 1 chunk(s)
  • rebuild: search index rebuild: create fts index se_path: ladybug query:
    Buffer manager exception: Unable to allocate memory! The buffer pool is full
    and no memory could be freed!
```

On a machine with 61 GiB of storage, approximately 24 GiB free. The message appears to be a shortage of RAM on the machine and not.
It: The one without memory was the Buffer Pool of the LadybugDB indexer.

The cost of error is not just the message. **INLINE_0** (internal/ast/json_rebuild.go) handles
Build search failure as a reason for NOT publishing - maintains the previous database purposefully.
because publishing an unindexed graph is worse. That's why: the 262 s of writing were thrown away
fora e o projeto continuava com o grafo velho.

## Implementation Details

### A causa

The ``openOnce`` in internal/ast/ladybug.go is replacing the `BufferPoolSize`, which by default is
About 80% of physical RAM — based on `boundedDBBufferPool` (internal/ast/resources.go).
That function was INLINE_0, and the ceiling of 1 GiB is
achieved by any machine with more than 10 GB. On a machine of 61 GB, the indexer
recebia 1 GiB e nunca mais que isso.

The comment that justified the ceiling was that the pool is a maximum of lazy growth.
The graph is small, so having a low ceiling was "free." The first half is true; the second half is not.
The second one doesn't apply. The inline 0 maintains the entire dictionary in the pool, and that's what.
um corpus grande estoura.

Why has never appeared in previous measurements: `TestFTSIndexBuildScaling` opens the store by
`ladybugstore.Open`, que passa `DefaultSystemConfig` — ~80% da RAM, **sem o clamp**. Todas as
measurements of FTS have run with hundreds of gigabytes of pool, while production is running with
One gigabyte. The two halves of the system have never been measured against the same configuration.

Correction

Teto **por papel**, em internal/ast/resources.go:

paper | fraction | ceiling | why
|---|---|---|---|
Writing (Indexer): It is the only one that runs INLINE_0; it is short and serialized by INLINE_1 (INLINE_2 = 1).
Reading:
| Limitation | 0.10 GB | The daemon and the MCP server hold handles for hours at a time, and do not return memory buffers — this ceiling is what limits their RSS throughout the session |

`boundedDBBufferPool` passou a receber `readOnly bool`, e `openOnce` passa
The variable continues to overwrite both roles because it is
Emergency exit recommended by the error message.

Also: `LadybugBackend` writes its true open threshold to `bufferPool`.
(`BufferPoolBytes()`), and `rebuildFTSIndexes` started involving exhaustion of the pool in 2023.
message that says the number and what to do with it (___INLINE_0), instead of
repassar a frase do motor.

Measurement

New probe: `internal/ast/fts_bufferpool_probe_test.go` behind `GRAPHIT_FTS_BUFPOOL=1`.
Writes the production path (`OpenSearchIndex` + `newSearchCopyLoader` + nine)
`CREATE_FTS_INDEX` na ordem real) variando o pool por `GRAPHIT_DB_BUFFER_MB`.

| entidades | pool | resultado |
|---|---|---|
| 400 k | 1 GiB | MARGINAL — falhou em `se_tri` numa rodada, construiu os nove na seguinte |
The operation failed in `se_tri`, and each `CHECKPOINT` took 0.00 seconds.
Four hundred kilobytes, eight gigabytes, nine indexes, twenty point three seconds.
| 1,0 M | 1 GiB | falha em `se_doc` |
| 1,0 M | 1,5 GiB | falha em `sf_source` |
| 1,0 M | 2 GiB | falha em `se_tri` |
One million | 3 gigabytes | nine indices, forty-four seconds
Two and a half megabytes | Six gigabytes | Segfault inside the engine in `se_tri`, with the machine at approximately 14 gigabytes available and swap full.
Two and a half million | Eight gigabytes | Nine indices, one hundred twenty-nine point four seconds

Three things that the table establishes:

The demand grows with the corpus, ~3 GB of pool per million entities.
The second one, "QUAL index dies not informing anything" – that's the one that runs after the pool is already set.
Full of it. The field reported `se_path`; the probe reports `se_tri`; it's the same failure.
3. **Inline 0 is not the correction.** The hypothesis that dirty pages accumulate between
Indices were not caused by this factor: with `CHECKPOINT` between each `CREATE_FTS_INDEX`.
The failure is identical, and checkpoints return instantly.

## Use Cases

UC-01: Full indexing of a large corpus on a machine with remaining RAM
- **Actor**: desenvolvedor rodando `graphit ast index .`, ou o daemon reindexando
- **Preconditions**: corpus grande o bastante para o build de FTS passar de 1 GiB de pool
In practice, starting from around 400 entities; machine with more than 16 GB of effective RAM.
- **Main Flow**:
  1. `openOnce` resolve o pool por `boundedDBBufferPool(def, readOnly=false)` — 25% do limite
Effective Memory, Topping at 8 GiB
  2. `RebuildFromCache` carrega as linhas e chama `rebuildFTSIndexes`
  3. Os nove `CREATE_FTS_INDEX` completam
  4. `RebuildFromJSON` publica com `AtomicSwapDB`
- **Alternative Flows**:
Set: Wins both rounds and ceilings for two roles
Machine with its default library bug setting already at or below (256 MiB): the default is
    devolvido intacto, sem inflar
- **Error Scenarios**:
  - Corpus grande demais para o teto (ex.: 2,5 M entidades num host de 16 GiB, que rende
    4 GiB): `ftsBuildError` devolve o pool em MiB e manda subir `GRAPHIT_DB_BUFFER_MB`
(~3072 entities per million entities), and logs the same at the Error level
Exhaustion at an extremely high scale may manifest as SIGSEGV within the engine rather than an error.
There's nothing to observe at 2.5 MB with 6 GB and an available memory-less machine.
    capturar nesse caso
Postconditions: graph and index published together by the same INLINE_0, or database
  anterior preservado
- **Affected Files**: `internal/ast/resources.go`, `internal/ast/ladybug.go`,
  `internal/ast/search_index.go`, `internal/ast/json_rebuild.go`

UC-02: The Daemon and MCP Server Maintaining Predictable RSS Feeds
- **Actor**: daemon global, servidor MCP
- **Preconditions**: handle read-only sobre o grafo, vivo por horas
- **Main Flow**:
  1. `NewLadybugDBReadOnly` marca `cfg.ReadOnly`
  2. `openOnce` resolve o pool por `boundedDBBufferPool(def, readOnly=true)` — 10%, teto 1 GiB
The lengthy process is confined to the reading level ceiling, which was what it was before this change.
- **Alternative Flows**:
A read-only caller reuses an already open read-write handle in the process.
The (`acquireDatabase`) inherits the writer's pool, which is exactly what was desired: it is the same pool.
- **Error Scenarios**:
The precious read query could still overflow 1 GB. It happened once with the query
Default of Explorer, and it was fixed reducing the query load – not increasing the pool.
- **Postconditions**: RSS do processo longo continua limitado como antes
- **Affected Files**: `internal/ast/resources.go`, `internal/ast/ladybug.go`

## Test Cases & Acceptance Criteria

### Feature: teto de buffer pool por papel
Ref: UC-01, UC-02

The big machine produces a large pool for the writer.
```gherkin
With a machine whose effective memory limit is 16 GB or more
When `boundedDBBufferPool` is called with readOnly set to false
The result is at least 4 GB.
  And nunca passa de 8 GiB
```

Scenario: The Large Machine Limits the Reader
```gherkin
With a machine whose effective memory limit is 16 GB or more
When `boundedDBBufferPool` is called with readOnly set to true
The result is nothing more than 1 GB.
And it is strictly smaller than the write-off pool of the same machine
```

Scenario: The default minimum is not inflated.
```gherkin
With a default of 128 MiB from the liblbug library, already below the 256 MiB threshold.
When `boundedDBBufferPool` is called with this default
The result is exactly 128 MiB.
And that applies to both roles.
```

Scenario: The environment variable wins over both roles
```gherkin
Given GRAPHIT_DB_BUFFER_MB = 128
When `boundedDBBufferPool` is called with a default of 16 GiB
The result is 128 MiB.
  And isso vale para readOnly = true e readOnly = false
```

### Feature: build de FTS sob pool limitado
Ref: UC-01

Scenario Outline: The Minimum Pool That Builds the Nine Indices
```gherkin
Given "loaded entities" loaded from the production write path
When os nove CREATE_FTS_INDEX rodam com um pool de "<pool>"
The result is "<outcome>"

Examples:
  | entities | pool    | outcome                |
Four hundred thousand | 1 gigabyte | marginal, occasionally fails
Four hundred thousand | Eight gigabytes | Nine indices
  | 1000000  | 2 GiB   | falha                   |
One million | 3 gigabytes | nine indices
Two hundred thousand and fifty thousand bytes, eight gigabytes, nine indices.
```

The exhaustion of the pool is reported in an actionable manner.
```gherkin
Given um corpus grande demais para o pool com que o handle foi aberto
When CREATE_FTS_INDEX falha com "Buffer manager exception"
Then o erro nomeia o pool em MiB com que o handle foi aberto
And recommends GRAPHIT_DB_BUFFER_MB with an order of magnitude (approximately 3,072 per million entities)
And the previous bank is preserved, not replaced by an index-free graph
```

## Files Changed

| File | Change | Reason |
|---|---|---|
The inline comment reads:

| `internal/ast/resources.go` | Modified | The roof and fraction of the buffer pool by paper; the registered measurement in the note |
| `internal/ast/ladybug.go` | Modified | `openOnce` passa `ReadOnly`; backend grava o pool efetivo, exposto por `BufferPoolBytes()` |
Here is the translation:

"_`internal/ast/search_index.go`_ | Modified | _`ftsBuildError`_ - exhaustion of the pool now says the number and output"

This appears to be describing a modification or change in a system, possibly related to an algorithm or process involving pools. The text suggests that when the pool's exhaustion is reached, it starts reporting both the number and the output.
_______ | Modified | contract by paper: large writer, limited reader, default small intact, expires v.
Brazilian Portuguese to idiomatic English:

"Created | Sondage | Measures minimum pool size by corpus size"
| `docs/tasks/fts-build-starved-by-a-flat-buffer-pool-ceiling.md` | Created | este registro |

## Trade-offs & Decisions

Ceiling by paper instead of just a number. Raising the ceiling from 8 GiB would resolve the issue.
Indexing and optimization of the daemon: A process that lives for hours would now be able to grow up to eight times larger.
The buffer pool does not return memory. Two headers cost one `bool` in the signature and one `if`.

"Fraction of the TOTAL RAM, not available." Twenty-five percent of 61 GiB equals 15.25 GiB, truncated at the ceiling for
Allocate memory accordingly.
It would be more secure and unstable: the number changes between calculation and use. All of it.
The remainder of the file (_`AntlrHeapBudget`) is already budgeted for the entire amount; maintaining the same base will be more predictable.
"Of course, let's aim for accuracy."

Without automatic retries with a larger pool. **INLINE_0** has a retry (`cache_embeddings`)
It does not have.
Equivalent. The only possible retry is to reopen the database with a larger pool and re-build it, which
It means recharge the lines. Left out: the actionable message covers the case, and the
The machine that needs this is the one without memory to give it.

The measurement and rejection of indices were made, not rejected by reasoning.

## Technical Debt

The remaining 80% of physical RAM continues passing through INLINE_1 without interruption.
Clamp and not respecting cgroups opens up the wikis' and memory's store entrances.
Also because the previous FTS probes measured a world that production does not live in.
A **clamp** would close the inconsistency.
The high-scale exhaustion can manifest as a SIGSEGV error instead of an error in the liblbug library.
"6 GB, memory-less machine). A GoCG crash can't be caught from within; what does"
It's estimating requirements before you start—knowing entity counts beforehand
build - fail early with an actionable message instead of late with a core dump.
- [ ] INLINE 0 is the most expensive index (51.7 seconds in 2.5 MB on the database, ~270 seconds projected into the corpus]
Cutting it remains the cheapest way forward for both of them.
The cost of incremental is not measured against `TestSearchIndexQualityFloor`.
Two writers, each with 8 GB of data on an available memory-less machine, crash both.
Measured during this very session: `go test ./internal/ast/` running alongside another
Real indexing with the machine at ~11 GiB available and swap full resulted in SIGSEGV;
The same selection of tests passes in 94.9 seconds with approximately 22 GB available.
Serializes the pipelines INSIDE the daemon and does not cover an _INLINE_0_ terminal.
      concorrendo com o daemon, nem `go test`. O teto por papel reduziu o risco no processo
Long, not here. A gateway between processes (lock file in memory) or overcommitting against memory
Available would resolve it; neither one was done.

## System Knowledge

The `CREATE_FTS_INDEX` maintains the dictionary of terms in the buffer pool. It is the consumer.
Dominant in the writing pool and the only one that scales with the corpus.
The failure was not deterministic at the border. The transfer of 400 KB/1 GB failed once and then succeeded again.
In the same machine, in minutes of difference. Near the limit, "passed" is not evidence.
"`ladybugstore` and `internal/ast` open LadybugDB with different configurations." Any
Measurement made by the first does not describe the behavior of the second. It was exactly what...
They concealed the previous measurements of FTS.
- **`acquireDatabase` compartilha um `*lbug.Database` por caminho no processo, com a config
  do primeiro que abriu.** Um leitor que reaproveita o handle do escritor herda o pool do
Writer; an author who finds only a read-only handle, opens a private handle with the configuration.
  dele.
The build for search failed, and nothing was published. INLINE_0 retains the previous database.
Purpose - The cost of an error here is a full re-indexing, not a degraded index.
The roof of writing is by process, and nothing coordinates processes. Refer to above for debt.
The sum of the tops is what the machine can handle, and no one calculates it.

## Progress Log

### 2026-08-19

Reproduced the failure with a probe on the production write path: 400k entities already.
They exploded at 1 GB, which rules out "only happens in a huge corpus."
Rejected `CHECKPOINT` based on measurement indices, not by reasoning.
Found the reason why none of the previous measurements saw this: `ladybugstore.Open` not
  aplica o clamp, e era por ali que as sondas de FTS abriam o store.
The ceiling has become paper-thin. The suite of `internal/ast` green in 94.9 seconds on a machine
Loaded; the SIGSEGV seen before was contention with a concurrent real indexing.
- **Verificado contra o caso real que originou o relato**: 39.429 arquivos, `write 728,43s`,
zero errors, inline 0 = 39.429 and hybrid search returning
Results. Before this change, the same indexing would abort in `create fts index se_path`.
It remains open: no barrier between processes (overdue).
