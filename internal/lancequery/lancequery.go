// Package lancequery is the read-only inspection layer over a LanceDB store: what columns a
// table has, and which rows match a predicate.
//
// WHY IT EXISTS: an agent that wanted the status of five known task IDs had no way to ask for
// it. Search ranks by relevance and get returns one whole record, so the only route left was
// fetching everything and grepping the output. This package is the missing third question —
// a structured one about a known set, projecting only the columns the answer needs.
//
// WHAT IT IS NOT, and none of these are oversights:
//
//   - NOT SQL. There is no SELECT, JOIN, GROUP BY or aggregate. Request.Filter is a predicate
//     evaluated by the engine, and that is the whole of the query language.
//   - NOT ORDERED. lancedb-go's QueryConfig has no ORDER BY, so a filter query comes back in
//     storage order and carries no score. Sorting the fetched page in Go would look like
//     ordering while paging through a different candidate set each time, so this package does
//     not offer it. When order matters, filter on a key range instead.
//   - NOT A WRITER. Nothing here appends, upserts, deletes, merges or creates.
//   - NOT A PAGINATOR. Limit and Offset are passed to the engine verbatim and the rows come
//     back untrimmed. Cursors, page ceilings and next_cursor belong to internal/pagination,
//     which every other paginated surface in this project already uses; a second scheme here
//     would be one more thing for a caller to get wrong.
//
// It also opens nothing. A caller hands it an already-open store, because resolving a URI,
// an imported context and its credentials is the owning module's job — and keeping that out
// means this package touches only lancestore's public API, so it needs no build tag of its
// own. A binary built without the `lancedb` tag fails earlier, at lancestore.Open.
//
// # Three tiers of column
//
// A module declares its own Policy, and columns fall into three tiers:
//
//   - REDACTED never leaves the store. It is refused in the projection AND in the filter:
//     allowing `claim_token = 'x'` as a predicate would turn the query into an oracle that
//     confirms a guess one call at a time, so closing only the projection closes nothing.
//   - VECTOR is never returned as numbers. A 768-wide embedding on a 66k-row table is the
//     opposite of what this package is for. It is left out of the default projection, and
//     naming it explicitly yields a marker describing the column rather than its values.
//   - HEAVY is a column a module knows is large — a source body, a full document. Out of the
//     default projection for cost, returned in full when asked for by name.
//
// Everything else is projected by default.
package lancequery

import (
	"context"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	page "github.com/graphit-labs/graphit-code/internal/pagination"
)

const (
	// DefaultLimit is the row count when a request sets none. It matches
	// pagination.DefaultPageSize, because a caller who asks for nothing should get the same
	// page everywhere in this project.
	DefaultLimit = page.DefaultPageSize
	// everyRow is the predicate used when a request supplies none. lancestore.Query.Validate
	// refuses a query with no Text, Vector or Filter, so "the whole table, paginated" needs a
	// tautology to be expressible at all.
	everyRow = "true"
)

// THERE IS DELIBERATELY NO CEILING HERE, and the reason is easy to get wrong.
//
// Capping Limit at page.MaxPageSize would clip the one row that makes paging work:
// page.Window.FetchLimit is PageSize+1, so a full page asks this layer for 101 rows and a
// ceiling of 100 would silently drop the look-ahead, making the last page claim it is the last
// when it is not. The ceiling belongs where the page is defined — page.Open already
// refuses a page_size above MaxPageSize — and this layer stays a faithful pass-through to the
// engine. A caller reaching past a tool surface is asking for a raw read and gets one.

// Policy is what a module declares about its own store: which tables are visible, and how each
// column is treated. A zero Policy exposes every table and treats every column as ordinary.
type Policy struct {
	// Tables limits what is visible. Empty means every table in the store.
	Tables []string
	// Redacted maps a table to columns that never leave it, in either direction.
	Redacted map[string][]string
	// Heavy maps a table to columns kept out of the default projection for size. Unlike a
	// redacted column, a heavy one is returned in full when the caller names it.
	Heavy map[string][]string
}

func (p Policy) allows(table string) bool {
	if len(p.Tables) == 0 {
		return true
	}
	for _, t := range p.Tables {
		if t == table {
			return true
		}
	}
	return false
}

func (p Policy) redactedIn(table string) map[string]bool { return set(p.Redacted[table]) }

func (p Policy) heavyIn(table string) map[string]bool { return set(p.Heavy[table]) }

func set(names []string) map[string]bool {
	out := make(map[string]bool, len(names))
	for _, n := range names {
		out[n] = true
	}
	return out
}

// Column is one column as the caller should understand it, including how this layer treats it.
type Column struct {
	Name     string `json:"name"`
	Type     string `json:"type"`
	Nullable bool   `json:"nullable,omitempty"`
	// Dim is the width of a vector column and zero for every other type.
	Dim int `json:"dim,omitempty"`
	// Redacted marks a column that no query will return or accept in a predicate.
	Redacted bool `json:"redacted,omitempty"`
	// Heavy marks a column left out of the default projection; name it to get it.
	Heavy bool `json:"heavy,omitempty"`
}

// TableInfo is one table's shape and size.
type TableInfo struct {
	Name    string   `json:"name"`
	Rows    int64    `json:"rows"`
	Columns []Column `json:"columns"`
}

// Schema is what Describe answers.
type Schema struct {
	Store  string      `json:"store"`
	Remote bool        `json:"remote,omitempty"`
	Tables []TableInfo `json:"tables"`
}

// Request is one structured question about one table.
type Request struct {
	// Table is required; there is no cross-table query.
	Table string
	// Filter is a Lance SQL predicate such as `status = 'completed'` or
	// `id IN ('tsk-a','tsk-b')`. Empty matches every row.
	Filter string
	// Columns is the projection. Empty means every column that is neither vector, heavy nor
	// redacted.
	Columns []string
	// Text adds a BM25 term, which requires an inverted index on TextColumn.
	Text       string
	TextColumn string
	// Limit is the row count asked of the engine, verbatim. Zero means DefaultLimit. It is
	// not clamped: see the note above the constants.
	Limit int
	// Offset skips rows. Meaningful only against the table's storage order, which is not
	// guaranteed stable across compaction.
	Offset int
}

// Result is one page of rows.
type Result struct {
	Table string `json:"table"`
	// Mode is what the engine actually ran: "filter" for a predicate alone, "fts" with a text
	// term.
	Mode string `json:"mode"`
	// Columns is the projection actually applied, in the table's own column order.
	Columns []string `json:"columns"`
	// Rows is exactly what the engine returned for the requested Limit and Offset. Nothing is
	// trimmed here; a caller that paginates hands these to pagination.FinishFetched, which
	// drops the look-ahead row and mints the cursor.
	Rows []map[string]any `json:"rows"`
}

// Describe reports the shape of the tables a policy exposes. Passing table names in `only`
// restricts it to those; nil describes every visible table.
func Describe(ctx context.Context, store *lancestore.Store, p Policy, only []string) (Schema, error) {
	if store == nil {
		return Schema{}, fmt.Errorf("lancequery: no store")
	}
	present, err := visibleTables(ctx, store, p)
	if err != nil {
		return Schema{}, err
	}
	wanted := present
	if len(only) > 0 {
		wanted = nil
		for _, name := range only {
			if !contains(present, name) {
				return Schema{}, unknownTable(name, present)
			}
			wanted = append(wanted, name)
		}
	}

	out := Schema{Store: store.URI(), Remote: store.Remote()}
	for _, name := range wanted {
		tbl, err := store.OpenTable(ctx, name)
		if err != nil {
			return Schema{}, fmt.Errorf("lancequery: describing %s: %w", name, err)
		}
		rows, err := tbl.Count(ctx)
		if err != nil {
			return Schema{}, fmt.Errorf("lancequery: counting %s: %w", name, err)
		}
		out.Tables = append(out.Tables, TableInfo{
			Name:    name,
			Rows:    rows,
			Columns: describeColumns(tbl.Schema(), p.redactedIn(name), p.heavyIn(name)),
		})
	}
	return out, nil
}

func describeColumns(schema lancestore.Schema, redacted, heavy map[string]bool) []Column {
	cols := make([]Column, 0, len(schema.Fields))
	for _, f := range schema.Fields {
		cols = append(cols, Column{
			Name:     f.Name,
			Type:     f.Type.String(),
			Nullable: f.Nullable,
			Dim:      f.Dim,
			Redacted: redacted[f.Name],
			Heavy:    heavy[f.Name],
		})
	}
	return cols
}

// Run executes one structured query. It never writes and never returns a redacted column.
func Run(ctx context.Context, store *lancestore.Store, p Policy, req Request) (Result, error) {
	if store == nil {
		return Result{}, fmt.Errorf("lancequery: no store")
	}
	present, err := visibleTables(ctx, store, p)
	if err != nil {
		return Result{}, err
	}
	name := strings.TrimSpace(req.Table)
	if name == "" {
		return Result{}, fmt.Errorf("lancequery: a query needs a table; available: %s", strings.Join(present, ", "))
	}
	if !contains(present, name) {
		return Result{}, unknownTable(name, present)
	}

	tbl, err := store.OpenTable(ctx, name)
	if err != nil {
		return Result{}, fmt.Errorf("lancequery: opening %s: %w", name, err)
	}
	schema := tbl.Schema()
	redacted := p.redactedIn(name)
	heavy := p.heavyIn(name)

	// The filter is checked before anything else runs. A redacted column in a predicate is an
	// oracle, not a projection leak, so refusing it in the projection alone would not close it.
	if col := mentionsRedacted(req.Filter, redacted); col != "" {
		return Result{}, fmt.Errorf(
			"lancequery: %q is redacted in %s and cannot appear in a filter", col, name)
	}
	if col := mentionsRedacted(req.TextColumn, redacted); col != "" {
		return Result{}, fmt.Errorf(
			"lancequery: %q is redacted in %s and cannot be searched", col, name)
	}

	projected, fetch, err := resolveProjection(schema, req.Columns, redacted, heavy, name)
	if err != nil {
		return Result{}, err
	}

	limit := req.Limit
	if limit <= 0 {
		limit = DefaultLimit
	}

	filter := strings.TrimSpace(req.Filter)
	if filter == "" {
		filter = everyRow
	}

	hits, err := tbl.Search(ctx, lancestore.Query{
		Filter:     filter,
		Columns:    fetch,
		Text:       strings.TrimSpace(req.Text),
		TextColumn: req.TextColumn,
		Limit:      limit,
		Offset:     req.Offset,
	})
	if err != nil {
		return Result{}, err
	}

	res := Result{Table: name, Columns: projected}
	markers := vectorMarkers(schema, projected, fetch)
	res.Rows = make([]map[string]any, 0, len(hits))
	for _, h := range hits {
		res.Mode = h.Mode
		row := make(map[string]any, len(projected))
		for k, v := range h.Row {
			row[k] = v
		}
		for col, marker := range markers {
			row[col] = marker
		}
		res.Rows = append(res.Rows, row)
	}
	if res.Mode == "" {
		res.Mode = lancestore.Query{Filter: filter, Text: strings.TrimSpace(req.Text)}.Mode()
	}
	return res, nil
}

// resolveProjection returns what the caller will see and what the engine is asked to read.
//
// The two differ for a vector column: it can be named in the projection, but it is never read,
// because transferring 768 floats per row to then discard them would pay the exact cost this
// package exists to avoid. Its place in the answer is filled by a marker instead.
func resolveProjection(
	schema lancestore.Schema, requested []string, redacted, heavy map[string]bool, table string,
) (projected, fetch []string, err error) {
	if len(requested) == 0 {
		for _, f := range schema.Fields {
			if redacted[f.Name] || heavy[f.Name] || f.Type == lancestore.FieldVector {
				continue
			}
			projected = append(projected, f.Name)
			fetch = append(fetch, f.Name)
		}
		if len(projected) == 0 {
			return nil, nil, fmt.Errorf(
				"lancequery: %s has no column that a default projection may return; name one explicitly", table)
		}
		return projected, fetch, nil
	}

	seen := make(map[string]bool, len(requested))
	// Ordering follows the table's own column order rather than the request's, so two callers
	// asking for the same set get the same answer shape.
	wanted := make(map[string]bool, len(requested))
	for _, raw := range requested {
		col := strings.TrimSpace(raw)
		if col == "" {
			continue
		}
		field, ok := schema.Field(col)
		if !ok {
			return nil, nil, fmt.Errorf("lancequery: %s has no column %q; available: %s",
				table, col, strings.Join(columnNames(schema), ", "))
		}
		if redacted[col] {
			return nil, nil, fmt.Errorf("lancequery: %q is redacted in %s and is never returned", col, table)
		}
		_ = field
		if seen[col] {
			continue
		}
		seen[col] = true
		wanted[col] = true
	}
	if len(wanted) == 0 {
		return nil, nil, fmt.Errorf("lancequery: the projection named no usable column")
	}
	for _, f := range schema.Fields {
		if !wanted[f.Name] {
			continue
		}
		projected = append(projected, f.Name)
		if f.Type != lancestore.FieldVector {
			fetch = append(fetch, f.Name)
		}
	}
	if len(fetch) == 0 {
		// Every requested column was a vector. The engine still needs something to read, and
		// an empty projection means "every column" to it, which is the opposite of the intent.
		return nil, nil, fmt.Errorf(
			"lancequery: a projection of vector columns alone returns no values; add a scalar column")
	}
	return projected, fetch, nil
}

// vectorMarkers describes each projected-but-unread vector column. The text says plainly that
// values are withheld, so a reader cannot mistake the marker for an empty embedding.
func vectorMarkers(schema lancestore.Schema, projected, fetch []string) map[string]string {
	read := set(fetch)
	out := map[string]string{}
	for _, name := range projected {
		if read[name] {
			continue
		}
		if f, ok := schema.Field(name); ok && f.Type == lancestore.FieldVector {
			out[name] = fmt.Sprintf("<vector dim=%d; values not returned>", f.Dim)
		}
	}
	return out
}

func visibleTables(ctx context.Context, store *lancestore.Store, p Policy) ([]string, error) {
	names, err := store.TableNames(ctx)
	if err != nil {
		return nil, fmt.Errorf("lancequery: listing tables: %w", err)
	}
	out := make([]string, 0, len(names))
	for _, n := range names {
		if p.allows(n) {
			out = append(out, n)
		}
	}
	sort.Strings(out)
	return out, nil
}

func unknownTable(name string, present []string) error {
	if len(present) == 0 {
		return fmt.Errorf("lancequery: no table %q; this store exposes none", name)
	}
	return fmt.Errorf("lancequery: no table %q; available: %s", name, strings.Join(present, ", "))
}

func columnNames(schema lancestore.Schema) []string {
	out := make([]string, 0, len(schema.Fields))
	for _, f := range schema.Fields {
		out = append(out, f.Name)
	}
	return out
}

func contains(haystack []string, needle string) bool {
	for _, h := range haystack {
		if h == needle {
			return true
		}
	}
	return false
}

var (
	wordMu    sync.Mutex
	wordCache = map[string]*regexp.Regexp{}
)

// mentionsRedacted reports the first redacted column named in a predicate, or "".
//
// The match is on whole words, case-insensitively: `claim_token = 'x'` is caught while a
// distinct column named `claim_token_hash` is not, because Go's \b treats `_` as a word
// character. The known imprecision runs the safe way — a string literal that happens to spell
// the column name, as in `title = 'claim_token'`, is refused even though it leaks nothing.
// Over-refusal here costs a caller one clearer rephrasing; under-refusal costs a token.
func mentionsRedacted(expr string, redacted map[string]bool) string {
	if strings.TrimSpace(expr) == "" || len(redacted) == 0 {
		return ""
	}
	names := make([]string, 0, len(redacted))
	for name := range redacted {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		if wordPattern(name).MatchString(expr) {
			return name
		}
	}
	return ""
}

func wordPattern(name string) *regexp.Regexp {
	wordMu.Lock()
	defer wordMu.Unlock()
	if re, ok := wordCache[name]; ok {
		return re
	}
	re := regexp.MustCompile(`(?i)\b` + regexp.QuoteMeta(name) + `\b`)
	wordCache[name] = re
	return re
}
