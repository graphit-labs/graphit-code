//go:build lancedb

package lancestore

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"

	arrow "github.com/apache/arrow/go/v17/arrow"
	"github.com/apache/arrow/go/v17/arrow/array"
	"github.com/apache/arrow/go/v17/arrow/memory"
	"github.com/lancedb/lancedb-go/pkg/contracts"
	"github.com/lancedb/lancedb-go/pkg/lancedb"
)

// The build WITH the native library, which `make fetch-lancedb` produces.
//
// ARROW STOPS HERE. lancedb-go is built against github.com/apache/arrow/go/v17 while the rest
// of this project uses github.com/apache/arrow-go/v18. Both live in the binary because they are
// different module paths, and they never meet because no Arrow value leaves this file: rows
// arrive as Go values and results leave as Go values.

// Store is a LanceDB database, local or read on-the-fly from object storage.
type Store struct {
	conn   contracts.IConnection
	uri    string
	remote bool

	mu     sync.Mutex
	tables map[string]*Table
}

// Table is one table in a Store.
type Table struct {
	store  *Store
	tbl    contracts.ITable
	name   string
	schema Schema
}

// Available reports whether this build can open a store. Always true here.
func Available() bool { return true }

// Open connects to a store. Nothing is downloaded for a remote URI: the connection is opened
// against object storage and every query reads over the network.
func Open(ctx context.Context, cfg Config) (*Store, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}

	var opts *contracts.ConnectionOptions
	if so := cfg.storageOptions(); len(so) > 0 {
		opts = &contracts.ConnectionOptions{StorageOptions: so}
	}

	conn, err := lancedb.Connect(ctx, cfg.URI, opts)
	if err != nil {
		return nil, fmt.Errorf("lancestore: connecting to %s: %w", cfg.URI, err)
	}
	return &Store{conn: conn, uri: cfg.URI, remote: cfg.IsRemote(), tables: map[string]*Table{}}, nil
}

// URI is where this store lives.
func (s *Store) URI() string { return s.uri }

// Remote reports whether this store is a published version read over the network.
func (s *Store) Remote() bool { return s.remote }

// Close releases the connection and every open table.
func (s *Store) Close() error {
	s.mu.Lock()
	for _, t := range s.tables {
		if t.tbl != nil {
			_ = t.tbl.Close()
		}
	}
	s.tables = map[string]*Table{}
	s.mu.Unlock()

	if s.conn == nil {
		return nil
	}
	return s.conn.Close()
}

// TableNames lists the tables in the store.
func (s *Store) TableNames(ctx context.Context) ([]string, error) {
	names, err := s.conn.TableNames(ctx)
	if err != nil {
		return nil, fmt.Errorf("lancestore: listing tables in %s: %w", s.uri, err)
	}
	return names, nil
}

// DropTable removes a table. Refused on a remote store.
// Dropping a table that is not there is a NO-OP and not an error: the caller's intent is "this
// table must not exist", and a full rebuild against a fresh store legitimately starts from
// nothing. Returning an error here made the rebuild fail on its very first run with
// `Table 'files' was not found`, which reads like corruption rather than like an empty store.
func (s *Store) DropTable(ctx context.Context, name string) error {
	if s.remote {
		return ErrReadOnly
	}
	if err := s.conn.DropTable(ctx, name); err != nil && !isMissingTable(err) {
		return fmt.Errorf("lancestore: dropping %s: %w", name, err)
	}
	s.mu.Lock()
	delete(s.tables, name)
	s.mu.Unlock()
	return nil
}

// CreateTable creates a table with the given schema. Refused on a remote store, EXCEPT that a
// publisher legitimately writes a remote version once — it does so with a store opened as
// local-write against the target URI, which is why this guard lives on the store and not on the
// URI scheme.
func (s *Store) CreateTable(ctx context.Context, name string, schema Schema) (*Table, error) {
	if s.remote {
		return nil, ErrReadOnly
	}
	if err := schema.Validate(); err != nil {
		return nil, err
	}
	arrowSchema, err := arrowSchemaOf(schema)
	if err != nil {
		return nil, err
	}
	isch, err := lancedb.NewSchema(arrowSchema)
	if err != nil {
		return nil, fmt.Errorf("lancestore: schema for %s: %w", name, err)
	}
	tbl, err := s.conn.CreateTable(ctx, name, isch)
	if err != nil {
		return nil, fmt.Errorf("lancestore: creating %s: %w", name, err)
	}
	return s.remember(name, tbl, schema), nil
}

// OpenTable opens an existing table.
//
// The schema is read back from the table rather than supplied, so a consumer of a published
// version does not have to know what the publisher wrote — which is what makes a remote context
// queryable without a local manifest.
func (s *Store) OpenTable(ctx context.Context, name string) (*Table, error) {
	s.mu.Lock()
	if t, ok := s.tables[name]; ok {
		s.mu.Unlock()
		return t, nil
	}
	s.mu.Unlock()

	tbl, err := s.conn.OpenTable(ctx, name)
	if err != nil {
		if isMissingTable(err) {
			return nil, fmt.Errorf("lancestore: %q in %s: %w", name, s.uri, ErrNoSuchTable)
		}
		return nil, fmt.Errorf("lancestore: opening %s: %w", name, err)
	}
	schema, err := readSchema(ctx, tbl)
	if err != nil {
		_ = tbl.Close()
		return nil, err
	}
	return s.remember(name, tbl, schema), nil
}

// EnsureTable opens the table, creating it with the given schema if it is not there yet.
func (s *Store) EnsureTable(ctx context.Context, name string, schema Schema) (*Table, error) {
	t, err := s.OpenTable(ctx, name)
	if err == nil {
		return t, nil
	}
	if !errors.Is(err, ErrNoSuchTable) {
		return nil, err
	}
	return s.CreateTable(ctx, name, schema)
}

func (s *Store) remember(name string, tbl contracts.ITable, schema Schema) *Table {
	t := &Table{store: s, tbl: tbl, name: name, schema: schema}
	s.mu.Lock()
	s.tables[name] = t
	s.mu.Unlock()
	return t
}

func isMissingTable(err error) bool {
	msg := strings.ToLower(err.Error())
	return strings.Contains(msg, "not found") || strings.Contains(msg, "does not exist")
}

// ---------- table ----------

// Name is the table's name.
func (t *Table) Name() string { return t.name }

// Schema is the table's columns.
func (t *Table) Schema() Schema { return t.schema }

// Close releases the table handle.
func (t *Table) Close() error {
	if t.tbl == nil {
		return nil
	}
	t.store.mu.Lock()
	delete(t.store.tables, t.name)
	t.store.mu.Unlock()
	return t.tbl.Close()
}

// Count is the number of rows.
func (t *Table) Count(ctx context.Context) (int64, error) {
	n, err := t.tbl.Count(ctx)
	if err != nil {
		return 0, fmt.Errorf("lancestore: counting %s: %w", t.name, err)
	}
	return n, nil
}

// Append adds rows. It does NOT replace anything — see Upsert for that.
func (t *Table) Append(ctx context.Context, rows []Row) error {
	if t.store.remote {
		return ErrReadOnly
	}
	if len(rows) == 0 {
		return nil
	}
	rec, err := recordOf(t.schema, rows)
	if err != nil {
		return err
	}
	defer rec.Release()

	if err := t.tbl.Add(ctx, rec, nil); err != nil {
		return fmt.Errorf("lancestore: appending %d rows to %s: %w", len(rows), t.name, err)
	}
	return nil
}

// DeleteWhere removes every row matching a SQL predicate.
func (t *Table) DeleteWhere(ctx context.Context, filter string) error {
	if t.store.remote {
		return ErrReadOnly
	}
	if strings.TrimSpace(filter) == "" {
		// Refused rather than treated as "everything": a caller that built an empty filter by
		// accident means to delete nothing, and emptying the index silently is unrecoverable.
		return errors.New("lancestore: DeleteWhere needs a filter; use DeleteWhere(\"true\") to mean everything")
	}
	if err := t.tbl.Delete(ctx, filter); err != nil {
		return fmt.Errorf("lancestore: deleting from %s where %s: %w", t.name, filter, err)
	}
	return nil
}

// DeleteByKey removes the rows whose key column matches any of the given values.
//
// The values are quoted and batched, because a predicate is a string and a caller's keys are
// arbitrary text — a path with an apostrophe in it would otherwise end the literal and change
// the predicate.
func (t *Table) DeleteByKey(ctx context.Context, keyColumn string, keys []string) error {
	if t.store.remote {
		return ErrReadOnly
	}
	if len(keys) == 0 {
		return nil
	}
	if _, ok := t.schema.Field(keyColumn); !ok && len(t.schema.Fields) > 0 {
		return fmt.Errorf("lancestore: %s has no column %q", t.name, keyColumn)
	}
	for _, batch := range batchStrings(keys, deleteBatch) {
		quoted := make([]string, 0, len(batch))
		for _, k := range batch {
			quoted = append(quoted, sqlQuote(k))
		}
		filter := fmt.Sprintf("%s IN (%s)", quoteIdent(keyColumn), strings.Join(quoted, ", "))
		if err := t.tbl.Delete(ctx, filter); err != nil {
			return fmt.Errorf("lancestore: deleting %d keys from %s: %w", len(batch), t.name, err)
		}
	}
	return nil
}

// Upsert replaces the rows sharing a key and appends the rest.
//
// It is delete-then-append rather than a merge, because that is what the storage offers, and
// the ORDER matters: deleting first means a re-indexed file cannot leave a stale copy of itself
// behind. The window between the two is why this is not safe to run concurrently against the
// same keys, which the incremental indexer guarantees by being single-writer.
func (t *Table) Upsert(ctx context.Context, keyColumn string, rows []Row) error {
	if t.store.remote {
		return ErrReadOnly
	}
	if len(rows) == 0 {
		return nil
	}
	keys := make([]string, 0, len(rows))
	for _, r := range rows {
		v, ok := r[keyColumn]
		if !ok {
			return fmt.Errorf("lancestore: upsert row has no %q", keyColumn)
		}
		s, ok := v.(string)
		if !ok {
			return fmt.Errorf("lancestore: upsert key %q is %T, want string", keyColumn, v)
		}
		keys = append(keys, s)
	}
	if err := t.DeleteByKey(ctx, keyColumn, keys); err != nil {
		return err
	}
	return t.Append(ctx, rows)
}

const deleteBatch = 256

// FoldNewRowsIntoIndexes folds rows written since the last index build INTO the existing
// indexes, without rebuilding them. It is the engine's own primitive (OptimizeIndex).
//
// IT IS A LATENCY MEASURE, NOT A CORRECTNESS ONE, and that distinction was measured rather than
// assumed. The obvious belief — that a row appended after the inverted index was built is
// invisible to full-text search until something reindexes — is FALSE here:
// TestFoldIsAboutLatencyNotVisibility appends a row carrying a term found nowhere else and finds
// it BEFORE any fold. The engine scans the unindexed fragments alongside the index.
//
// So skipping the fold does not lose rows; it makes queries progressively slower as unindexed
// fragments pile up, because the scanned part grows with every write. Calling it after a batch
// keeps reads on the indexed path.
//
// The SQLite index had no equivalent choice: it maintained its FTS tables with triggers and paid
// per row, on the write.
func (t *Table) FoldNewRowsIntoIndexes(ctx context.Context) error {
	if t.store.remote {
		return ErrReadOnly
	}
	if _, err := t.tbl.OptimizeWithAction(ctx, contracts.OptimizeAction{
		Kind: contracts.OptimizeIndex,
	}); err != nil {
		return fmt.Errorf("lancestore: folding new rows into %s's indexes: %w", t.name, err)
	}
	return nil
}

// Compact merges the small fragments a burst of writes leaves behind, and reclaims the bytes of
// deleted rows. Separate from FoldNewRowsIntoIndexes because it is about layout rather than
// searchability: skipping it makes reads slower, skipping the other makes them WRONG.
func (t *Table) Compact(ctx context.Context) error {
	if t.store.remote {
		return ErrReadOnly
	}
	if _, err := t.tbl.OptimizeWithAction(ctx, contracts.OptimizeAction{
		Kind: contracts.OptimizeCompact,
	}); err != nil {
		return fmt.Errorf("lancestore: compacting %s: %w", t.name, err)
	}
	return nil
}

// EnsureIndexes builds the given indexes, skipping any that already exist.
//
// A full-text query needs IndexInvertedText on its column and returns NOTHING without one, so
// this is not an optimisation step that can be deferred.
func (t *Table) EnsureIndexes(ctx context.Context, indexes ...Index) error {
	if t.store.remote {
		return ErrReadOnly
	}
	existing := map[string]bool{}
	if infos, err := t.tbl.GetAllIndexes(ctx); err == nil {
		for _, i := range infos {
			existing[strings.ToLower(i.Name)] = true
		}
	}

	for _, idx := range indexes {
		if f, ok := t.schema.Field(idx.Column); !ok {
			return fmt.Errorf("lancestore: %s has no column %q to index", t.name, idx.Column)
		} else if idx.Kind == IndexInvertedText && f.Type != FieldString {
			return fmt.Errorf("lancestore: %s.%s is %s — an inverted index needs a string column",
				t.name, idx.Column, f.Type)
		}

		name := indexName(idx)
		if existing[strings.ToLower(name)] {
			continue
		}
		kind, err := vendorIndexKind(idx.Kind)
		if err != nil {
			return err
		}
		params := vendorIndexParams(idx)
		if err := t.tbl.CreateIndexWithParams(ctx, []string{idx.Column}, kind, params,
			&contracts.CreateIndexOptions{Name: name}); err != nil {
			return fmt.Errorf("lancestore: building %s on %s.%s: %w", idx.Kind, t.name, idx.Column, err)
		}
	}
	return nil
}

func indexName(idx Index) string {
	return fmt.Sprintf("%s_%s_idx", idx.Column, strings.ReplaceAll(idx.Kind.String(), "-", "_"))
}

// vendorIndexParams renders the tokenizer configuration for the engine. Only the fields a
// caller set are passed, so an unset option keeps the engine's own default rather than this
// package's idea of one.
func vendorIndexParams(idx Index) contracts.IndexParams {
	p := contracts.IndexParams{}
	if idx.Kind != IndexInvertedText {
		return p
	}
	o := idx.Text
	p.FtsLanguage = o.Language
	p.FtsStem = o.Stem
	p.FtsRemoveStopWords = o.RemoveStopWords
	p.FtsLowerCase = o.LowerCase
	p.FtsASCIIFolding = o.ASCIIFolding
	p.FtsWithPosition = o.WithPosition
	p.FtsBaseTokenizer = o.BaseTokenizer
	p.FtsNgramMinLength = o.NgramMin
	p.FtsNgramMaxLength = o.NgramMax
	p.FtsNgramPrefixOnly = o.NgramPrefixOnly
	p.FtsMaxTokenLength = o.MaxTokenLength
	return p
}

func vendorIndexKind(k IndexKind) (contracts.IndexType, error) {
	switch k {
	case IndexInvertedText:
		return contracts.IndexTypeFts, nil
	case IndexVectorIVFPQ:
		return contracts.IndexTypeIvfPq, nil
	case IndexVectorHNSW:
		return contracts.IndexTypeHnswSq, nil
	case IndexScalarBTree:
		return contracts.IndexTypeBTree, nil
	case IndexScalarBitmap:
		return contracts.IndexTypeBitmap, nil
	default:
		return 0, fmt.Errorf("lancestore: unknown index kind %d", int(k))
	}
}

// Search runs a full-text, semantic or hybrid query.
//
// THE HYBRID CASE IS THE POINT: when both Text and Vector are set, the engine runs the dense
// and the BM25 pass and fuses them with its own reciprocal-rank-fusion reranker. No ranking is
// computed here — that was the whole reason for choosing this engine.
func (t *Table) Search(ctx context.Context, q Query) ([]Hit, error) {
	if err := q.Validate(); err != nil {
		return nil, err
	}
	mode := q.Mode()
	limit := q.limit()

	// With reranking on, the FIRST stage widens: a cross-encoder can only reorder what retrieval
	// returned, so recall has to be retrieval's problem rather than the reranker's.
	fetch := q.Rerank.candidates(limit)

	cfg := contracts.QueryConfig{Limit: &fetch}
	if q.Filter != "" {
		cfg.Where = q.Filter
	}

	switch mode {
	case "fts":
		cfg.FTSSearch = &contracts.FTSSearch{Column: q.TextColumn, Query: q.Text}
	case "semantic":
		cfg.VectorSearch = &contracts.VectorSearch{
			Column: q.VectorColumn, Vector: q.Vector, K: fetch,
		}
	case "hybrid":
		// One query carrying both channels. FullTextQuery inside VectorSearch is what turns it
		// hybrid; the reranker fuses the two rankings.
		cfg.VectorSearch = &contracts.VectorSearch{
			Column: q.VectorColumn, Vector: q.Vector, K: fetch,
			FullTextQuery:  q.Text,
			FullTextColumn: q.TextColumn,
		}
		cfg.Reranker = &contracts.RerankerConfig{Kind: contracts.RerankerRRF, RRFK: q.RRFK}
	case "filter":
		// Nothing to configure: cfg already carries Where and Limit, and with no ranking channel
		// the engine returns the matching rows unranked.
	}

	rows, err := t.tbl.Select(ctx, cfg)
	if err != nil {
		return nil, fmt.Errorf("lancestore: %s search on %s: %w", mode, t.name, err)
	}

	// _score AND _relevance_score ARE TWO DIFFERENT COLUMNS, and a hybrid row carries BOTH.
	//
	// They used to share one `case` arm, both assigning to h.Score, inside this `for k, v := range
	// r` — so which one survived was decided by Go's map iteration order, which is randomised per
	// map. MEASURED before the fix, twenty identical queries against one unchanged index: every
	// row returned exactly two distinct scores, e.g. RowGroups at 0.015625 three times and 1.0
	// seventeen times. Same index, same query, same row.
	//
	// The consequence was not a wobbly number but a wrong ORDER, because the caller sorts by this
	// field: the entity a query named by name dropped from rank one to rank four. It read as a
	// relevance problem and was a map-iteration problem.
	//
	// So they are kept apart. _score is the channel's own value — BM25 on a text query — and
	// RelevanceScore is what the engine's reranker produced when it fused two channels. Score
	// exposes the one that ranks THIS query: fused when there was a fusion, the raw channel score
	// otherwise.
	hits := make([]Hit, 0, len(rows))
	for _, r := range rows {
		h := Hit{Row: Row{}, Mode: mode}
		var raw, fused float64
		var haveFused bool
		for k, v := range r {
			switch k {
			case "_score":
				raw = toFloat(v)
			case "_relevance_score":
				fused, haveFused = toFloat(v), true
			case "_distance":
				h.Distance = toFloat(v)
			default:
				h.Row[k] = t.normalizeRead(k, v)
			}
		}
		h.RawScore = raw
		h.RelevanceScore = fused
		if haveFused {
			h.Score = fused
		} else {
			h.Score = raw
		}
		hits = append(hits, h)
	}

	// The second stage. Off unless the caller asked for it; a failure here degrades to the
	// engine's own order rather than losing the answer.
	ranked, rerankErr := q.Rerank.apply(ctx, q.Text, hits, limit)
	if rerankErr != nil {
		// Degraded, not failed: the engine's own order is a good answer, and losing every result
		// because a second-stage model could not load would be worse than losing the reordering.
		return ranked, fmt.Errorf("lancestore: %s search on %s succeeded but reranking did not: %w",
			mode, t.name, rerankErr)
	}
	return ranked, nil
}

// ---------- arrow, which does not leave this file ----------

// normalizeRead converts a value the engine returned into the type the caller wrote.
//
// A VECTOR DOES NOT COME BACK AS IT WENT IN. It is written as []float32 and read as
// []interface{} holding float64, because that is what the Arrow-to-Go bridge produces for a
// fixed-size list. Every caller doing `v.([]float32)` therefore gets a failed assertion and a nil
// vector — silently, since a type assertion with the two-value form does not error.
//
// MEASURED: the wiki's StoredEmbeddings returned an empty list while EmbeddingStats correctly
// counted the same rows as embedded, because one used the filter and the other the assertion.
// Converting here means the round trip is symmetric at the one place that knows the schema.
func (t *Table) normalizeRead(column string, v any) any {
	f, ok := t.schema.Field(column)
	if !ok || f.Type != FieldVector {
		return v
	}
	switch raw := v.(type) {
	case []float32:
		return raw
	case []any:
		out := make([]float32, 0, len(raw))
		for _, e := range raw {
			out = append(out, float32(toFloat(e)))
		}
		return out
	case []float64:
		out := make([]float32, 0, len(raw))
		for _, e := range raw {
			out = append(out, float32(e))
		}
		return out
	}
	return v
}

func arrowSchemaOf(s Schema) (*arrow.Schema, error) {
	fields := make([]arrow.Field, 0, len(s.Fields))
	for _, f := range s.Fields {
		dt, err := arrowTypeOf(f)
		if err != nil {
			return nil, err
		}
		fields = append(fields, arrow.Field{Name: f.Name, Type: dt, Nullable: f.Nullable})
	}
	return arrow.NewSchema(fields, nil), nil
}

func arrowTypeOf(f Field) (arrow.DataType, error) {
	switch f.Type {
	case FieldString:
		return arrow.BinaryTypes.String, nil
	case FieldInt64:
		return arrow.PrimitiveTypes.Int64, nil
	case FieldFloat64:
		return arrow.PrimitiveTypes.Float64, nil
	case FieldBool:
		return arrow.FixedWidthTypes.Boolean, nil
	case FieldVector:
		return arrow.FixedSizeListOf(int32(f.Dim), arrow.PrimitiveTypes.Float32), nil
	default:
		return nil, fmt.Errorf("lancestore: field %q has unknown type %d", f.Name, int(f.Type))
	}
}

// readSchema recovers the column set from an existing table, so a consumer needs no manifest.
func readSchema(ctx context.Context, tbl contracts.ITable) (Schema, error) {
	as, err := tbl.Schema(ctx)
	if err != nil {
		return Schema{}, fmt.Errorf("lancestore: reading the schema of %s: %w", tbl.Name(), err)
	}
	out := Schema{}
	for _, f := range as.Fields() {
		ft, dim := fieldTypeOf(f.Type)
		out.Fields = append(out.Fields, Field{Name: f.Name, Type: ft, Dim: dim, Nullable: f.Nullable})
	}
	return out, nil
}

func fieldTypeOf(dt arrow.DataType) (FieldType, int) {
	switch t := dt.(type) {
	case *arrow.FixedSizeListType:
		return FieldVector, int(t.Len())
	}
	switch dt.ID() {
	case arrow.INT64:
		return FieldInt64, 0
	case arrow.FLOAT64:
		return FieldFloat64, 0
	case arrow.BOOL:
		return FieldBool, 0
	default:
		// Anything else is read as text, which is what the query surface treats it as.
		return FieldString, 0
	}
}

// recordOf builds one Arrow record from Go values.
//
// A missing key is null, which is refused for a non-nullable column HERE rather than letting
// the builder produce a record the engine rejects with a message that names no column.
func recordOf(s Schema, rows []Row) (arrow.Record, error) {
	as, err := arrowSchemaOf(s)
	if err != nil {
		return nil, err
	}
	b := array.NewRecordBuilder(memory.DefaultAllocator, as)
	defer b.Release()

	for ci, f := range s.Fields {
		fb := b.Field(ci)
		for ri, row := range rows {
			v, present := row[f.Name]
			if !present || v == nil {
				if !f.Nullable {
					return nil, fmt.Errorf("lancestore: row %d has no %q and the column is not nullable", ri, f.Name)
				}
				fb.AppendNull()
				continue
			}
			if err := appendValue(fb, f, v); err != nil {
				return nil, fmt.Errorf("lancestore: row %d, column %q: %w", ri, f.Name, err)
			}
		}
	}
	return b.NewRecord(), nil
}

func appendValue(fb array.Builder, f Field, v any) error {
	switch f.Type {
	case FieldString:
		s, ok := v.(string)
		if !ok {
			return fmt.Errorf("want string, got %T", v)
		}
		fb.(*array.StringBuilder).Append(s)
	case FieldInt64:
		n, err := toInt64(v)
		if err != nil {
			return err
		}
		fb.(*array.Int64Builder).Append(n)
	case FieldFloat64:
		fb.(*array.Float64Builder).Append(toFloat(v))
	case FieldBool:
		x, ok := v.(bool)
		if !ok {
			return fmt.Errorf("want bool, got %T", v)
		}
		fb.(*array.BooleanBuilder).Append(x)
	case FieldVector:
		vec, ok := v.([]float32)
		if !ok {
			return fmt.Errorf("want []float32, got %T", v)
		}
		if len(vec) != f.Dim {
			return fmt.Errorf("vector has %d values, column is %d wide", len(vec), f.Dim)
		}
		lb := fb.(*array.FixedSizeListBuilder)
		lb.Append(true)
		lb.ValueBuilder().(*array.Float32Builder).AppendValues(vec, nil)
	default:
		return fmt.Errorf("unknown field type %d", int(f.Type))
	}
	return nil
}

func toInt64(v any) (int64, error) {
	switch n := v.(type) {
	case int64:
		return n, nil
	case int:
		return int64(n), nil
	case int32:
		return int64(n), nil
	case float64:
		return int64(n), nil
	default:
		return 0, fmt.Errorf("want int64, got %T", v)
	}
}

func toFloat(v any) float64 {
	switch n := v.(type) {
	case float64:
		return n
	case float32:
		return float64(n)
	case int64:
		return float64(n)
	case int:
		return float64(n)
	default:
		return 0
	}
}

// ---------- SQL literal safety ----------

// sqlQuote renders a value as a single-quoted SQL literal, doubling embedded quotes.
//
// SAFETY: keys here are file paths and entity ids, which contain apostrophes often enough that
// this is not theoretical — a path like `it's/a.go` would otherwise close the literal early and
// turn the rest of the key into syntax.
func sqlQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", "''") + "'"
}

// quoteIdent renders a column name safely, with BACKTICKS.
//
// SAFETY, and this one is a silent data corruption if got wrong: the filter dialect treats a
// DOUBLE-QUOTED name as a string LITERAL, not as an identifier. MEASURED — `"uid" IN ('u2')`
// deletes nothing and returns NO ERROR, because the predicate it really evaluates is
// `'uid' IN ('u2')`, which is false for every row. Since Upsert is delete-then-append, a delete
// that silently matches nothing leaves the old row in place and appends the new one, so the
// index quietly accumulates duplicates. Backticks and bare names both work; backticks are used
// so a column whose name collides with a keyword still parses.
func quoteIdent(s string) string {
	return "`" + strings.ReplaceAll(s, "`", "``") + "`"
}

func batchStrings(in []string, size int) [][]string {
	if size <= 0 {
		size = len(in)
	}
	var out [][]string
	for start := 0; start < len(in); start += size {
		end := start + size
		if end > len(in) {
			end = len(in)
		}
		out = append(out, in[start:end])
	}
	return out
}
