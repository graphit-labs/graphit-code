package memory

import (
	"context"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// StoreTables is every table a memory scope owns. There is exactly one: a scope is a table,
// and its revision history lives in the same rows rather than a side table.
func StoreTables() []string { return []string{memoryTableName} }

// StorePolicy is what the inspection surface may see in a memory scope.
//
// NOTHING IS REDACTED, and that is a finding rather than an omission: the 21 columns were read
// back from a live table and none of them carries a credential, a fencing token or a lock. A
// memory record is content the user asked to keep, so the only cost worth managing here is
// size.
//
// body is heavy because a memory is prose and the whole point of a query is to avoid paying
// for prose while asking which records match. memory_source reads one when it is wanted.
// embedding needs no entry: lancequery keeps every vector column out of a projection by
// construction, whatever a module declares.
func StorePolicy() lancequery.Policy {
	return lancequery.Policy{
		Tables: StoreTables(),
		Heavy:  map[string][]string{memoryTableName: {"body"}},
	}
}

// DescribeStore reports the shape of this scope's memory table, read from the table itself.
//
// `only` is accepted although a memory scope holds exactly one table, so the signature matches
// the other three modules' DescribeStore. A caller that learned one should be able to use all
// four without checking which of them happens to take a narrowing argument.
func (m *MemoryService) DescribeStore(ctx context.Context, only []string) (lancequery.Schema, error) {
	tbl, err := m.openTable(ctx)
	if err != nil {
		return lancequery.Schema{}, err
	}
	defer func() { _ = tbl.Close() }()
	return lancequery.Describe(ctx, tbl.store, StorePolicy(), only)
}

// QueryStore runs one predicate query against this scope's memory table.
func (m *MemoryService) QueryStore(ctx context.Context, req lancequery.Request) (lancequery.Result, error) {
	if req.Table == "" {
		// One table means naming it is ceremony, so the caller may leave it out. Passing a
		// different name still fails in lancequery, which is where that message belongs.
		req.Table = memoryTableName
	}
	tbl, err := m.openTable(ctx)
	if err != nil {
		return lancequery.Result{}, err
	}
	defer func() { _ = tbl.Close() }()
	return lancequery.Run(ctx, tbl.store, StorePolicy(), req)
}
