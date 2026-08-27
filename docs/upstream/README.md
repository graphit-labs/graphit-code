# Pending Upstream Reports

Five external-dependency defects found while working on the AST indexer.
**None has been filed** — opening an issue on a third-party repository is an external action and requires an
Engineer decision.

Each file here is the report body, ready to paste, in English (the upstream project's audience).

| file | project | severity | effect on Graphit |
|---|---|---|---|
| `antlr4-go-stdout.md` | antlr/antlr4 (Go runtime) v4.13.1 | high | corrupted the MCP stdout protocol |
| `grammars-v4-plsql-sll-blowup.md` | antlr/grammars-v4 | high | OOM on a 1.7 KB file |
| `liblbug-fts-insert.md` | LadybugDB/liblbug 0.18.2 | high | incremental cost O(corpus) instead of O(1) |
| `liblbug-close-and-unwind.md` | LadybugDB/liblbug 0.18.2 | medium | `Close()` up to 5s; crash on `UNWIND ... CREATE` |
| `liblbug-string-corruption.md` | LadybugDB/liblbug 0.18.2 | high | **silent data loss** |

Four of the five have a minimal repro. The fifth, `liblbug-string-corruption.md`, **has none and
probably should not be filed as-is** — the field-scale probe (35,358 lines,
866 MB, same byte composition, same index build) reproduces nothing. With volume
eliminated alongside data shape, concurrency, and the cgo pointer, suspicion now falls on
the path strings travel **before** the database: parser, shard cache, JSON round-trip, multiple goroutines. No probe covers that; all of them hand the database a string assembled
right there. The file is kept for the value of the eliminations it records.

The most valuable to us is `liblbug-fts-insert.md`: until it is fixed, every
incremental search-index update recreates seven FTS indexes over the entire corpus.
`TestLadybugFTSPerRowInsertIsReliable` is intentionally inverted — it passes while the bug
exists and fails once fixed, signaling that the mitigation can be removed.
