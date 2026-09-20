package ast

import (
	"context"
	"errors"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// StoreTables is the full-text side of an AST store: the indexed entities and the files they
// came from. The Cypher graph is a different store entirely — graph.icebug beside search.lance
// — and is not reachable from here.
func StoreTables() []string { return []string{lanceEntitiesTable, lanceFilesTable} }

// StorePolicy is what the FTS inspection surface may see.
//
// NEITHER `entities.body` NOR `files.source` IS WHAT ITS NAME SUGGESTS, and that is why both
// are redacted rather than merely held back for size.
//
// They are BM25 documents this package synthesises, not stored code. entityBody concatenates
// the entity's name, its split form, lowercase variants, its label and generated n-grams;
// fileSearchDocument appends path n-grams to the file text behind a `\x00` separator. Handing
// either to a caller returns something that reads as corrupted source — a run of two-letter
// fragments and a NUL byte — and invites them to treat it as the real thing.
//
// Marking them heavy would have been the wrong tier: heavy means "large but true", and a
// caller who names the column gets the value. These are not true. Redacted refuses them in both
// the projection and the filter, which is right for the projection and is an accepted loss in
// the filter: a substring search over code belongs to ast_search in fts mode, which ranks
// against these very columns using the tokeniser they were built for. Reading real source is
// ast_source.
//
// Nothing else is redacted. An index of a repository's own code holds no credential the
// repository does not already hold in plain sight.
//
// `docstring` deliberately stays in the default projection. It is the one column that explains
// what an entity is without being the entity, which is exactly what a listing wants.
func StorePolicy() lancequery.Policy {
	return lancequery.Policy{
		Tables: StoreTables(),
		Redacted: map[string][]string{
			lanceEntitiesTable: {lanceBodyColumn},
			lanceFilesTable:    {lanceFileTextColumn},
		},
	}
}

var errNoSearchIndex = errors.New("ast: no search index is open")

// DescribeStore reports the shape of the FTS tables, read from the tables themselves.
func (s *SearchIndex) DescribeStore(ctx context.Context, only []string) (lancequery.Schema, error) {
	if s == nil || s.store == nil {
		return lancequery.Schema{}, errNoSearchIndex
	}
	return lancequery.Describe(ctx, s.store, StorePolicy(), only)
}

// QueryStore runs one predicate query against one FTS table. It answers the questions hybrid
// search cannot: every entity in a path, the split between project code and dependencies, how
// many entities of a kind exist.
func (s *SearchIndex) QueryStore(ctx context.Context, req lancequery.Request) (lancequery.Result, error) {
	if s == nil || s.store == nil {
		return lancequery.Result{}, errNoSearchIndex
	}
	return lancequery.Run(ctx, s.store, StorePolicy(), req)
}
