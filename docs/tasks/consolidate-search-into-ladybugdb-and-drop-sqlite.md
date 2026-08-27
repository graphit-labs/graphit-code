---
title: SQLite exits binary format - arrays and vectors now reside in LadybugDB
status: done-with-caveat
created: 2026-08-16
updated: 2026-08-16
tags: [ast, wiki, memory, ladybug, sqlite, search, storage, incremental]
---

SQLite exits binary format - indexes and vectors move to LadybugDB

Origin: Engineer's instruction — "there are so many ladybugs for the graph as well as sqlite for fts/bm25 and semantic search, the problem is that it weighs too much on disk and processing with two databases. I want to be just the ladybug."

A previous attempt was made on July 26, 2026 (reverted commits `354a32c` and `c90e73f`). This did not reuse that code — it was written from scratch, using only the changelogs of that batch as a map of traps.

---

Why now, having failed before

The three July blockages were rechecked on this date. Two remain true; only the baseline has changed, not the engine.

The July blockage | State as of August 16, 2026 |
|---|---|
| Index FTS is not maintained during insert — 25 lines invisible | **Continues** `TestLadybugFTSPerRowInsertIsReliable` (purposefully inverted) passed 12/12. `go-ladybug v0.17.0` remains the newest. |
| Cascade failure in incremental | Already resolved in design: drop indices before mutating. |
| "Incremental 5,3 s against ~330 ms" | **Baseline obsolete** Those 330 ms were measured with in-place write. |
| Intermittent string corruption | Continues and is anterior to this change — already affecting the production graph. |

The number that killed migration compared the cost of Ladybug against an incremental **in-place**.
This arrangement ceased to be production: `inPlaceIncrementalEnabled()` only linked with `GRAPHIT_INPLACE_INCREMENTAL=1`, and the default — copy+swap — already copies the entire directory of the database and already pays 215 ms-5.0 s just closing the mutated copy. The DROP+CREATE of FTS indices passes inside this copy, which is already O(corpus).

And the disk measurement that motivated everything on this laptop is project INLINE 6:

| | size |
|---|---|
| INLINE_7 (the graph) | 833 MB |
| INLINE_8 (the index) | **2.3 GB** |

The index was 2.8 times the graph he described.

---

What has changed

The inplace path was removed; copy+swap is the only option.

Engineering Request. **Attention to Inversion:** The literal request was "only the possibility of `GRAPHIT_INPLACE_INCREMENTAL=1` should exist," but this flag LIGA (L) in-place — the opposite of "the production database is always read-only," which is how he described the system in his first message. Presented with the measurement, he chose copy+swap:

```
escritor                                          leituras ok  abertura falhou  crashou
in-place, commit + CHECKPOINT                        43/60           11            6
Copy and Swap, never written production                60/60            0            0
```

The 6 are SIGSEGV inside cgo, in `open`: the CHECKPOINT rewrites pages underneath a reader that is opening the same file. It cannot be retried – the process MCP follows.

Removed: `incrementalInPlace`, `inPlaceIncrementalEnabled`, `deleteFileDataChecked`, a flag, `internal/ast/incremental_mode_test.go` and `TestLadybugInPlaceWritesUnderCrossProcessReaders`. The measurement survives in the comment of `IncrementalRebuild`.

This translation preserves the original structure and technical terms while converting the Portuguese text into idiomatic English.

The index of the AST is inside the graph database.

`internal/ast/fts_sqlite.go` (1159 linhas) removido. No lugar:

Arquivo | Paper
--- | ---
__INLINE_18__ | schema, rebuild, incremental |
__INLINE_19__ | lexical search, semantic analysis, hybrid, reading from source |
__INLINE_20__ | what depends on the engine: tokenization, split, trigrams, sorting |

Tables **INLINE_21** and **INLINE_22** within the same **INLINE_23**, not in a sibling, is the decision that closes the design: an index outside the file does not enter swap without a second copy itself, and updating it there — which is what SQLite did, concurrently with readers reading — left the atomic publication for search outside of the published corpus. A crash between them would describe two different corpora in the graph and index. Now **INLINE_24** publishes both or none.

Consequence: The inline 25 does not open its own database. It lends the ___INLINE_26__ already opened (___INLINE_27__), because the engine blocks a second handle RW on the same database. In the incremental, it is built upon the ___INLINE_28__ — the COPY —, never on production. Readers use ___INLINE_29__.

3. The wiki (knowledge + memory) also

`internal/wiki/fts.go` (1667 linhas) removido. No lugar `internal/wiki/store.go`,
`internal/wiki/store_query.go` e `internal/wiki/search_text.go`. Tabelas `WikiChunk`,
`WikiXRef`, `WikiMeta`, `WikiSyncLog`.

The __INLINE_38__ has started receiving __INLINE_39__ instead of a serialized blob, and
the __INLINE_40__ became __INLINE_41__: the shard format on disk (___INLINE_43__) remains unchanged — it's now an archive format with ownership in ___INLINE_44__.

### 4. `internal/ladybugstore`, novo

Thin Layer Shared. Exists because __INLINE_46__ cannot import __INLINE_47__, and
both now need the same primitives: open/close, load extension, execute, coerce result of graph.

5. SQLite exits from binary

Removed __INLINE_48__, __INLINE_49__, and __INLINE_50__ from the Makefile.
Do __INLINE_51__ and the two files-guard, __INLINE_53__. __INLINE_54__, __INLINE_55__, and __INLINE_56__ run **without tags**.

---

What the engine demanded, and how each part responded

The FTS5 capacity is not listed. In LadybugDB, the response is:
| INLINE 57 | No | A vector index per field with column weights applied during RRF merge |
| INLINE 58 | No | Pre-computed trigram tokenizer at write time, indexed with word tokenizers - better than MELHOR because partial overlap is scored instead of requiring containment |
| INLINE 59 | No | Same trigram bag: 11/11 vs. prefix index's 9/11 in truncated queries |
| INLINE 60 | No | Conjunctive form of `vec0` |
| INLINE 61 | No | Fallback, scoped to label |
| INLINE 62 | Vector Index Native | **Gain**: Maintained during insert and delete operations, so add ___INLINE_63__ and re-write the entire file that forced space leakage |

The inline fields are not listed. In LadybugDB, the response is:
| INLINE 57 | No | A vector index per field with column weights applied during RRF merge |
| INLINE 58 | No | Pre-computed trigram tokenizer at write time, indexed with word tokenizers - better than MELHOR because partial overlap is scored instead of requiring containment |
| INLINE 59 | No | Same trigram bag: 11/11 vs. prefix index's 9/11 in truncated queries |
| INLINE 60 | No | Conjunctive form of `vec0` |
| INLINE 61 | No | Fallback, scoped to label |
| INLINE 62 | Vector Index Native | **Gain**: Maintained during insert and delete operations, so add ___INLINE_63__ and re-write the entire file that forced space leakage |

Measured on this date and still in doubt in July: `UNWIND` with `FLOAT[768]` is accepted, so vectors will be included in the lot; the vector index is accepted in an empty table; and rows with `emb` NULL are ignored by the query.

The stemmer is explicitly fixed (`stemmer := 'porter'`) instead of left in the default state. The default
is porter today — sonded: `'none'` only the literal `schema`, while default, `'porter'` and
`'english'` reach `schemas` —, and ranking depends on it.

---

Three defects of fusion identified through measurement

The RRF dismisses the magnitude of the score and retains only the position, which breaks the reconstruction of
`bm25()` in three places. Nothing was predicted; all appeared as errors.

1. A tie became an advantage.
Two documents with identical scores were separated by the deterministic ordering, which is alphabetical, and this position entered into fusion. `schema` returned `SchemaValidator` about `validateSchema` due to the alphabetical face-up or face-down. Corrected with competition ranking — tied participants share the rank.
2. Summing fields gave a full vote to the weak signal. Rank 0 in index `path` (weight 1) is worth 1/(k+1) more for being weaker than the marriage, and k=60 exceeds ~6 positions in name index (weight 10). `config` returned `parseConfig` about `configLoader` because `parseConfig` lives in `config.go`. Corrected with a stronger signal + rest dampened by 0.2.
3. Exact name marriage was shifted. `config` returned `parseConfig` — which is house, docstring, and path — about the struct literally called `Config`. Corrected with an exact name boost.

In the wiki, the same family: The title's initial weight was initially set at 6.0, above the inline `body` field (1.0). Since the bag of words houses disjunctively, a single shared 3-gram puts the document in rank 0 of itself — and the query `credenciais`, which appears in the body of one document but not in the title at all, ranked another document first. The weights of the trigram in the wiki were pushed down to the field with the weakest weight (0.7 and 0.4), where the FTS5 design had them (a shift from 1.5 against 0.7 term passes).

---

Two flaws of the SWAP that SQLite concealed

The two only appeared in the e2e of the daemon, and neither is about search—both are about publishing to the database. While the index was an SQLite file outside of swap space, both were invisible: they only affected pages of the graph that no test checked at this resolution. With the index inside the store, there was a total and immediate failure in the search.

The inline 88 model left behind the sidecars of the engine.

He renamed the file and removed only `<path>.wal` alone. The other sidecars of liblbug —
`.shadow`, `.wal.checkpoint`, `.tmp`, and the two checkpoint locks — are named by
PATH, not by the identity of the file, so they survived the rename and remained alongside the
NEW file describing the previous incarnation. The next opening would retrieve from them,
above what had just been published.

---

Note: I have translated the code blocks as is, preserving their original format and content.

Measured: after the swap, `ladybugdb.shadow` with 1,1 MB and `ladybugdb.wal.checkpoint` with 44 KB next to a `ladybugdb` of 2.5 MB. Removing the sidecars, the same store becomes 516 KB.

The correction names suffixes instead of sweeping `<path>.*`, for the same reason that cleanup of a swap interruption had to learn: a glob also grabs copies of work and sidecars that follow.

2. The full rebuild was building the search in the file that the rename had just shut down

The code writes into a TEMPORARY database and renames it before publishing.
Inline 98 referred to this rebuild as "rebuilding" and only then constructed the search index through handle Inline 99 — which points to the renamed file. All lines and FTS indices went there, and they were discarded on the next open.

This translation maintains the technical details of database operations and indexing processes while translating the specific terms used in the original Portuguese text into idiomatic English.

The symptom was exactly what the test reported: `MATCH (n:SearchEntity) RETURN count(n)` returned three lines, `FileSourceAt` returned the text from the file, and every `QUERY_FTS_INDEX` returned zero.

Corrected with __INLINE_103__, which populates the search tables INSIDE the temporary database before renaming. The graph and index are now published by the same operation.

A silent bug that died in construction

INLINE_104: INLINE_105 inserted into INLINE_106 **without the column INLINE_107**. The rebuild filled in (line 347), the incremental did not. Every entity touched by an incremental lost its trigrams until the next full rebuild, and the recall abbreviation transition (INLINE_108 → INLINE_109) stopped reaching them, silent. Now the line is built somewhere else (INLINE_110), so it cannot be expressed.

---

## Testes

Green with `TestConsolidationQualityGate`, without a build tag.

Converted instead of erased, because the expectation survives the change in engine:

- **INLINE_114** (Ladybug × SQLite) **removed** — there is no second engine to compare against, and a differential test with one side removed compares implementation with toy.
  Your role is of `TestSearchIndexQualityFloor`, which asserts an absolute floor in the same corpus and the same probes. The corpus and `TestLadybugFTSFeatureParity` are left as they were.

- `TestExpansionFieldCeiling` — the assertion was "the two redactions tie at 9/9", a behavior of the FTS5 prefix index. Without it, the morphological redaction falls to 8/9, while only the exact token redaction reaches 9/9. Rewritten to assert the real condition, which is another reason not to build the expansion field.

- `TestFileSourceAtDoesNotMigrateTheIndex` → `TestFileSourceAtDoesNotMutateTheStore` more
  `TestFileSourceAtLeavesTheWriteHandleFree`. The danger has changed: it was a schema migration that toppled `file_fts`; now it's the writer's vacancy, which a read-write daemon would take from.

- Tests of `quoteToken`, `buildPhraseQuery`, `buildANDQuery`, `buildORQuery`,
  `buildPrefixQuery` removed with functions — there are no explicit phrase, boolean, or wildcard to build.

Quality measured: **14/16** on the lexical floor (compared to 13 in SQLite, which replaced it), and 11/11 at the decisive sonars of the hybrid.

---

⚠️ Measurement in the Real Corpus: The Drawing Does Not Scale in This Project

Measured against the production shard cache of project `<private-corpus>`, reconstructing to an outline path—production was not touched.

39,429 files, 2,501,342 entities — 12.5 times the 200K entities of July measurement scale.
This scale had never been measured.

```plaintext
| | new (Ladybug) | old (SQLite) |
|---|---|---|
| store | 5.7 GB | 5.5 GB (873 MB graph + 4.65 GB index) |
| full rebuild | 988 s (16.5 min) | — |
| **incremental from one file** | **1.178 s (19.6 min)** | ~330 ms (measured in 200k, in-place) |
| query | 487–778 ms | 50–146 ms (measured in 200k) |
| buffer pool required | **8 GiB** | — |
```

In the synthetic corpus, numbers are good and did not anticipate anything of this: 2000 files / 10,000 entities yield a full response in 2.30 seconds, an incremental response in 1.19 seconds, and a query response in just 69 milliseconds.

Three problems, in order of severity

The incremental cost is 20 minutes per file, and it's SLOWER than the full rebuild. It copies 5.7 GB, mutates, drops, recreates the 9 FTS indices, closes, and performs a swap. The DROP+CREATE is Corpus — the same work of a complete build — and the copy comes above. For a daemon that reindexes on every save, it's impractical.

**2. Do not build with the buffer pool that the project departs from.** `boundedDBBufferPool` stalls at
1 GiB per database (`dbBufferPoolCeil`); with it, the creation of `se_path` fails with
*"Buffer manager exception: Unable to allocate memory! The buffer pool is full"*. The 988 above are with `GRAPHIT_DB_BUFFER_MB=8192`.

This translation preserves the original structure and technical terms while translating the Portuguese text into idiomatic English.

3. Storage gain did not exist. 5.70 GB against 5.5 GB—tie. The savings that prompted the entire migration were absent from this corpus.

What does this mean?

The reenquadrage of copy+swap dissolved the RELATIVE argument of July — the 330 ms were baseline in-place, and in-place is no longer the production layout. It did not dissolve the ABSOLUTE: liblbug does not maintain index FTS during insert, so all writes are O(corpus), and 2.5 million entities take up dozens of minutes. This is the original block, measured by the scale that matters.

What is solid and not affected: the search is CORRECT and better (14/16 against 13/16, and queries about the 2.5M entities return results in 487-778 ms with new handle); the wiki migrated well because wikis are small and always reconstructed from scratch; SQLite exited binary; ICU exited.

The cheapest path before any major decision

Cutting FTS indices. Today is 9. The three expensive and low-value ones are `sf_source` (indexes the entire text of all files), `se_path` (weight 1) and `se_type` (weight 2). Cutting to 4-5 should significantly reduce build and disk usage, which is measurable in a single run. Continue
(Corpus): Reduce the size by 20 minutes, not delete them.

If not, the options are: to significantly increase the threshold of incremental (reindex only rarely and in batches, not by save); keep SQLite just for the AST index and leave the wiki on Ladybug; or wait upstream — `TestLadybugFTSPerRowInsertIsReliable` is exactly inverted to alert when liblbug starts maintaining the index during insert.

**Trap of the harness, registered because it cost time:** consult through the handle `lb`
after a `IncrementalRebuild` returns zero for everything. The swap renames above and the handle continues pointing to the detached inode — same family as the two swap defects above. A new handle on the same store responds normally.

---

## A ICU saiu junto

The question raised at the same session. The answer REVERSES the premise that it would be there due to SQLite: ICU is not of either.

- Version v0.17.0 of `liblbug.so` does not declare ICU in `NEEDED` (`libdl`, `libpthread`, `libssl`,
  `libcrypto`, `libm`, `libgcc_s`, `libc`, `ld-linux`), and `strings -a` does not find it there, so it is also named `dlopen`.
- The build documentation for LadybugDB does not list ICU on any platform.
- SQLite was already discarded as a consumer: it was compiled only with the tag `fts5`, never `sqlite_icu`. Removing it did not change anything in the calculation.
- Functional test, on Linux: remove all ICU files from an extracted runtime and run
  `ast query --hybrid`, which exercises LadybugDB, text index, vector index, and ONNX once. It worked.

Note: The inline references (`liblbug.so` to `ast query --hybrid`) are placeholders for actual code or documentation paths that should be replaced with the specific values in your translation context.

Removed from the THREE platforms: the `find` of `libicu*` in `build-linux` and `build-darwin` (along with the two `rm -f` that only existed to clean up the duplicates those globs produced), the flags `-licuuc -licuin -licudt` and includes from ICU in `build-windows` and `build-windows-native`, the three copies of `/mingw64/bin/libicu*.dll`, and an exclusion `! -name "*icu*"` in `find` that copies all DLLs from the sysroot of mingw.

**macOS and Windows were not verified** — this requires artifacts for each platform, and the Engineer decided to remove it even if something breaks. The asymmetry of risk is registered in the Makefile comment: on macOS, a dylib missing causes the process to abort at startup, so any failure there is total, not partial. `libicu-dev` / `icu4c` may still be necessary for COMPILING; this is another thing and it did not change.

Value: 37-73 MiB on Linux, varying with the number of ICU major versions used by the build machine, because the glob command grabbed all.
