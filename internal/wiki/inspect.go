package wiki

import (
	"context"
	"errors"

	"github.com/graphit-labs/graphit-code/internal/lancequery"
)

// StoreTables is every table a wiki index owns: the pages, the links between them, the sync
// history and the index's own metadata.
func StoreTables() []string {
	return []string{lanceChunksTable, lanceXRefsTable, lanceSyncLogTable, lanceMetaTable}
}

// StorePolicy is what the inspection surface may see in a wiki index.
//
// Nothing is redacted. The 28 columns of `chunks` were read back from a live index and none of
// them holds a credential: a wiki page is published documentation, and the index adds
// clustering, staleness and revision bookkeeping around it. If a column ever does carry a
// secret, this is the list that has to grow.
//
// The heavy set is large here because a wiki is prose by definition. body is the page,
// search_terms concatenates several fields for BM25, and summary is a paragraph — returning
// all three by default would make "which pages are stale" cost as much as reading the wiki.
// wiki_source is the route for a page's text.
func StorePolicy() lancequery.Policy {
	return lancequery.Policy{
		Tables: StoreTables(),
		Heavy: map[string][]string{
			lanceChunksTable:  {"body", "summary", "search_terms"},
			lanceSyncLogTable: {"added_json", "updated_json", "deleted_json", "details_json"},
		},
	}
}

// DescribeStore reports the shape of this index's tables, read from the tables themselves.
//
// It deliberately does not call ensureTables: that path CREATES what is missing, which is the
// right thing before a sync and the wrong thing for a read. lancequery opens what it needs and
// says so plainly when a table is absent — which is itself the honest answer for an index that
// was never built.
func (w *WikiDB) DescribeStore(ctx context.Context, only []string) (lancequery.Schema, error) {
	if w == nil || w.store == nil {
		return lancequery.Schema{}, errNoWikiStore
	}
	return lancequery.Describe(ctx, w.store, StorePolicy(), only)
}

// QueryStore runs one predicate query against one table of this index.
func (w *WikiDB) QueryStore(ctx context.Context, req lancequery.Request) (lancequery.Result, error) {
	if w == nil || w.store == nil {
		return lancequery.Result{}, errNoWikiStore
	}
	return lancequery.Run(ctx, w.store, StorePolicy(), req)
}

var errNoWikiStore = errors.New("wiki: no index is open")
