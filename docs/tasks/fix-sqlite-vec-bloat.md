# Fix: sqlite-vec bloat no ladybugdb.search.sqlite

## Resultado

**20 GB → 760 MB** (redução de 96%)

## Problema

O arquivo `.graphit/ast/project/ladybugdb.search.sqlite` cresceu para **20GB** com apenas **2.3% de utilização real**.

### Diagnóstico

O `sqlite-vec` (`vec0`) aloca chunks de tamanho fixo de **1024 slots × 3072 bytes = 3MB por chunk**. Quando linhas são deletadas via `DELETE FROM entity_vec`, os slots são apenas marcados como inválidos no bitmap de `entity_vec_chunks.validity` — o espaço em disco **nunca é liberado**.

| Métrica | Valor |
|---|---|
| Slots alocados | 6,714,368 (6557 chunks × 1024) |
| Slots usados | 154,791 |
| Taxa de utilização | 2.3% |
| Espaço desperdiçado | ~18.76 GB |

### Causa secundária: DROP VIRTUAL TABLE falha silenciosamente

A primeira tentativa de fix usou `DROP VIRTUAL TABLE entity_vec` + `CREATE`. Mas o `sql.DB` do Go mantém um pool de conexões com prepared statements compilados que referenciam as tabelas — isso impede que o SQLite execute o DROP, que falha silenciosamente (erro ignorado com `_, _ = ...`). O `CREATE` subsequente falha com "table already exists".

## Solução Final Aplicada

### `internal/ast/fts_sqlite.go` — `RebuildFromCache()`

Substituído todo o mecanismo DROP/CREATE por **close + delete + reopen** do arquivo SQLite:

```go
_ = s.db.Close()
_ = os.Remove(s.path)
_ = os.Remove(s.path + "-wal")
_ = os.Remove(s.path + "-shm")

db, _ := sql.Open("sqlite3", s.path+"?_journal_mode=WAL&...")
migrateSearchSchema(db)  // recria tabelas vazias
s.db = db
// ... INSERT direto, sem DELETEs necessários
```

Essa abordagem é garantida de funcionar independente do sqlite-vec ou do pool de conexões.

### `cmd/graphit/commands/ast.go` — `ast embed` com `pending == 0`

Adicionado check de existência do SQLite no early-return:

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

- `internal/ast/fts_sqlite.go` — `RebuildFromCache()`: close+delete+reopen em vez de DROP/DELETE
- `cmd/graphit/commands/ast.go` — `newASTEmbedCmd()`: rebuild quando SQLite não existe e `pending == 0`
