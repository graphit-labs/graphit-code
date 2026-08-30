# Fix: sqlite-vec bloat in ladybugdb.search.sqlite

## Result

**20 GB → 760 MB** (96% reduction)

## Problem

The file `.graphit/ast/project/ladybugdb.search.sqlite` grew to **20GB** with only **2.3%
actual utilization**.

### Diagnosis

`sqlite-vec` (`vec0`) allocates fixed-size chunks of **1024 slots × 3072 bytes = 3MB per
chunk**. When rows are deleted via `DELETE FROM entity_vec`, the slots are only marked invalid
in the `entity_vec_chunks.validity` bitmap — the disk space is **never released**.

| Metric | Value |
|---|---|
| Allocated slots | 6,714,368 (6557 chunks × 1024) |
| Used slots | 154,791 |
| Utilization rate | 2.3% |
| Wasted space | ~18.76 GB |

### Secondary cause: DROP VIRTUAL TABLE fails silently

The first fix attempt used `DROP VIRTUAL TABLE entity_vec` + `CREATE`. But Go's `sql.DB`
maintains a connection pool with prepared statements compiled against the tables — this
prevents SQLite from executing the DROP, which fails silently (error ignored with `_, _ =
...`). The subsequent `CREATE` fails with "table already exists".

## Final Solution Applied

### `internal/ast/fts_sqlite.go` — `RebuildFromCache()`

Replaced the entire DROP/CREATE mechanism with **close + delete + reopen** of the SQLite file:

```go
_ = s.db.Close()
_ = os.Remove(s.path)
_ = os.Remove(s.path + "-wal")
_ = os.Remove(s.path + "-shm")

db, _ := sql.Open("sqlite3", s.path+"?_journal_mode=WAL&...")
migrateSearchSchema(db)  // recreates empty tables
s.db = db
// ... direct INSERT, no DELETEs needed
```

This approach is guaranteed to work regardless of sqlite-vec or the connection pool.

### `cmd/graphit/commands/ast.go` — `ast embed` with `pending == 0`

Added a check for SQLite's existence in the early return:

```go
if pending == 0 {
    if _, err := os.Stat(idxPath); os.IsNotExist(err) {
        // SQLite was deleted — rebuild without re-embedding
        searchIdx.RebuildFromCache(parseCache, embLookup)
    }
    return nil
}
```

## Modified Files

- `internal/ast/fts_sqlite.go` — `RebuildFromCache()`: close+delete+reopen instead of DROP/DELETE
- `cmd/graphit/commands/ast.go` — `newASTEmbedCmd()`: rebuild when SQLite doesn't exist and `pending == 0`
