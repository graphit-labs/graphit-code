# Fix: sqlite-vec bloat no ladybugdb.search.sqlite

## Resultado

"20 GB → 760 MB" (a reduction of 96%)

## Problema

The file `.graphit/ast/project/ladybugdb.search.sqlite` grew to **20GB** with only **2.3% of actual usage**.

Diagnosis

The `sqlite-vec` (`vec0`) allocates fixed-size chunks of **1024 slots × 3072 bytes = 3MB per chunk**. When lines are deleted via `DELETE FROM entity_vec`, the slots are only marked as invalid in the `entity_vec_chunks.validity` bitmap — disk space is never released.

Slots allocated: 6,714,368 (6557 chunks × 1024)
Slots used: 154,791
Utilization rate: 2.3%
Space wasted: ~18.76 GB

Secondary Cause: Virtual Table DROP fails silently

The first attempt to fix used `DROP VIRTUAL TABLE entity_vec` + `CREATE`. However, the `sql.DB` in Go maintains a pool of prepared statements compiled against tables — this prevents SQLite from executing the DROP, which silently fails (error ignored with `_, _ = ...`). The `CREATE` subsequent attempt fails with "table already exists".

Final Solution Applied

### `internal/ast/fts_sqlite.go` — `RebuildFromCache()`

Replaced the entire mechanism of `DROP/CREATE` with **close + delete + reopen** for the SQLite file:

```go
_ = s.db.Close()
_ = os.Remove(s.path)
_ = os.Remove(s.path + "-wal")
_ = os.Remove(s.path + "-shm")

db, _ := sql.Open("sqlite3", s.path+"?_journal_mode=WAL&...")
migrateSearchSchema(db)  // recria tabelas vazias
s.db = db
... INSERT directly, without necessary DELETES
```

This approach is guaranteed to work independently of sqlite-vec or connection pools.

--- INLINE 12 --- INLINE 13 with INLINE 14

Added a check for the existence of SQLite in an early return:

```go
if pending == 0 {
    if _, err := os.Stat(idxPath); os.IsNotExist(err) {
        // SQLite foi deletado — rebuildar sem re-embeddar
        searchIdx.RebuildFromCache(parseCache, embLookup)
    }
    return nil
}
```

## Arquivos modificados

- `internal/ast/fts_sqlite.go` - `RebuildFromCache()`: close, delete, and reopen instead of DROP/DELETE
- `cmd/graphit/commands/ast.go` - `newASTEmbedCmd()`: rebuild when SQLite does not exist and `pending == 0`
