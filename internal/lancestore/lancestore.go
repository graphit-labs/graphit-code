// Package lancestore is the search layer: a LanceDB database, local or read on-the-fly from
// object storage.
//
// TWO MODES, AND THEY ARE NOT SYMMETRIC:
//
//   - LOCAL is where a project's own index lives, and it is the only place writes happen in
//     normal operation. It replaces the SQLite index entirely.
//   - REMOTE (`s3://…`) is a version PUBLISHED to the Hub. A publisher writes it once, by
//     extracting from the populated local index; every consumer only ever reads it, over the
//     network, without downloading a byte.
//
// So the write surface is aimed at local use and the query surface at both, and a caller that
// mixes them up gets an error rather than a surprise — see Store.Remote and ErrReadOnly.
//
// WHY THIS PACKAGE OWNS ITS OWN ARROW: lancedb-go is built against
// `github.com/apache/arrow/go/v17` while the rest of this project uses
// `github.com/apache/arrow-go/v18` (a different module path, so both coexist). Arrow values
// never cross this package's boundary — callers pass Go values and receive Go values — so the
// two versions never meet and nothing has to be converted.
package lancestore

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

// ErrNotBuilt is returned by every operation when the binary was compiled without the
// `lancedb` build tag.
//
// The native library is ~230 MiB and is built from source by `make fetch-lancedb`, so making
// it mandatory would break `go build ./...` for anyone who has not run that target. The tag is
// what keeps the tree buildable while the search port is in flight.
var ErrNotBuilt = errors.New("lancestore: built without the lancedb tag — run `make fetch-lancedb` and build with -tags lancedb")

// ErrReadOnly is returned when a write is attempted against a remote store.
//
// A published version is immutable from the consumer's side: it is produced once by extraction
// from a populated local index. Writing to it from a consumer would fork the artifact the
// registry names, which is the one thing this layout must not allow.
var ErrReadOnly = errors.New("lancestore: this store is a published version and is read-only")

// ErrNoSuchTable is returned when a table does not exist.
var ErrNoSuchTable = errors.New("lancestore: no such table")

// errRerankerChangedSet is returned when a Reranker gives back a different set than it received.
var errRerankerChangedSet = errors.New("lancestore: the reranker returned a different set of hits")

// FieldType is the column type of a schema field.
//
// This is a small closed set on purpose. It is what the search index needs — text to match,
// scalars to filter, a vector to compare — and keeping it closed means the Arrow mapping has
// no default branch that could silently store a value as something else.
type FieldType int

const (
	// FieldString is text. A field of this type can carry the inverted index.
	FieldString FieldType = iota
	// FieldInt64 is a whole number, filterable and orderable.
	FieldInt64
	// FieldFloat64 is a real number.
	FieldFloat64
	// FieldBool is a flag.
	FieldBool
	// FieldVector is a fixed-width embedding. Dim must be set.
	FieldVector
)

func (f FieldType) String() string {
	switch f {
	case FieldString:
		return "string"
	case FieldInt64:
		return "int64"
	case FieldFloat64:
		return "float64"
	case FieldBool:
		return "bool"
	case FieldVector:
		return "vector"
	default:
		return fmt.Sprintf("FieldType(%d)", int(f))
	}
}

// Field is one column.
type Field struct {
	Name string
	Type FieldType
	// Dim is the width of a FieldVector, ignored otherwise. A vector column is fixed-width in
	// Lance, so this is not optional for that type.
	Dim int
	// Nullable allows the column to hold nothing. A key column must not be nullable.
	Nullable bool
}

// Schema is a table's columns, in order.
type Schema struct {
	Fields []Field
}

// Field looks a column up by name.
func (s Schema) Field(name string) (Field, bool) {
	for _, f := range s.Fields {
		if f.Name == name {
			return f, true
		}
	}
	return Field{}, false
}

// Validate refuses a schema that cannot be stored, rather than letting the failure surface as
// an Arrow error three layers down.
func (s Schema) Validate() error {
	if len(s.Fields) == 0 {
		return errors.New("lancestore: schema has no fields")
	}
	seen := make(map[string]bool, len(s.Fields))
	for _, f := range s.Fields {
		if strings.TrimSpace(f.Name) == "" {
			return errors.New("lancestore: schema has a field with no name")
		}
		if seen[f.Name] {
			return fmt.Errorf("lancestore: schema repeats the field %q", f.Name)
		}
		seen[f.Name] = true
		if f.Type == FieldVector && f.Dim <= 0 {
			return fmt.Errorf("lancestore: vector field %q needs a positive Dim", f.Name)
		}
		if f.Type != FieldVector && f.Dim != 0 {
			return fmt.Errorf("lancestore: field %q is %s but sets Dim", f.Name, f.Type)
		}
	}
	return nil
}

// Row is one record, keyed by column name.
//
// Values are Go natives: string, int64, float64, bool, []float32. A missing key stores null,
// which is refused for a non-nullable column.
type Row map[string]any

// IndexKind is the kind of index to build on a column.
type IndexKind int

const (
	// IndexInvertedText is the full-text (BM25) index. Only a string column can carry it, and
	// a full-text query needs it: without one the query matches nothing.
	IndexInvertedText IndexKind = iota
	// IndexVectorIVFPQ is the compressed IVF index, for large collections.
	IndexVectorIVFPQ
	// IndexVectorHNSW is the graph index, for lower-latency recall.
	IndexVectorHNSW
	// IndexScalarBTree is an ordered index for range filters.
	IndexScalarBTree
	// IndexScalarBitmap suits a low-cardinality column, such as an entity type.
	IndexScalarBitmap
)

func (k IndexKind) String() string {
	switch k {
	case IndexInvertedText:
		return "inverted-text"
	case IndexVectorIVFPQ:
		return "vector-ivf-pq"
	case IndexVectorHNSW:
		return "vector-hnsw"
	case IndexScalarBTree:
		return "scalar-btree"
	case IndexScalarBitmap:
		return "scalar-bitmap"
	default:
		return fmt.Sprintf("IndexKind(%d)", int(k))
	}
}

// Index is one index to build.
type Index struct {
	Column string
	Kind   IndexKind
	// Text configures the engine's own tokenizer, and is only read for IndexInvertedText.
	//
	// PREFERRING THIS OVER DOING THE SAME WORK IN GO IS THE RULE, not a preference: the engine
	// owns tokenising, stemming, folding and n-gram generation, and anything this package
	// pre-computes instead is a second implementation to keep in step with it.
	Text TextIndexOptions
}

// TextIndexOptions is the engine's inverted-index tuning.
//
// Every field maps to a Lance FTS parameter. A zero value leaves the engine's default, so a
// caller configures only what it has measured a reason to change.
type TextIndexOptions struct {
	// Language for stemming and stop words, e.g. "English". Empty leaves the default.
	Language string
	// Stem enables the language's stemmer, so "configuration" reaches "configures".
	Stem *bool
	// RemoveStopWords drops the language's stop words from the index.
	RemoveStopWords *bool
	// LowerCase folds case. On by default in the engine.
	LowerCase *bool
	// ASCIIFolding maps accented characters to ASCII, which matters for identifiers in
	// languages that use them.
	ASCIIFolding *bool
	// WithPosition stores term positions, which phrase queries need and which the engine's
	// documentation warns significantly increases index size.
	WithPosition *bool
	// BaseTokenizer selects the tokenizer: "simple" (words) or "ngram" (substrings). An "ngram"
	// tokenizer is what gives substring and truncation matching without a prefix query, which
	// this engine has no syntax for.
	BaseTokenizer string
	// NgramMin and NgramMax bound the generated substrings, read only for the ngram tokenizer.
	NgramMin *uint32
	NgramMax *uint32
	// NgramPrefixOnly restricts the ngram tokenizer to prefixes of each token.
	NgramPrefixOnly *bool
	// MaxTokenLength drops tokens longer than this.
	MaxTokenLength *uint32
}

// Query is one search, in any of the three modes.
//
// WHICH MODE IT IS is decided by what is set, not by a flag:
//
//	Text alone            -> full text, BM25 over the inverted index
//	Vector alone          -> nearest neighbour
//	Text AND Vector       -> HYBRID, and the fusion is the ENGINE'S
//
// The hybrid case is the reason this package exists rather than a thin wrapper: LanceDB runs
// both passes and fuses them with its own reranker, so nothing here re-implements ranking.
type Query struct {
	// Text is the full-text query. Requires an IndexInvertedText on TextColumn.
	Text string
	// TextColumn pins which column the text query runs against. Empty lets the engine use the
	// single text-indexed column, which is an error if there is more than one.
	TextColumn string

	// Vector is the embedding to compare against.
	Vector []float32
	// VectorColumn pins the vector column.
	VectorColumn string

	// Filter is a SQL predicate applied to the candidate set, e.g. `is_dependency = false`.
	Filter string

	// Columns projects the read down to the ones named. Empty reads every column, which on
	// a table carrying an embedding and a full text body means the whole table: a lookup
	// that only needs keys must say so or it pays for the payload it is not going to read.
	Columns []string

	// Limit caps the rows returned. Zero means DefaultLimit.
	Limit int

	// Offset skips rows before the limit is applied, so a lookup larger than one page can
	// be walked. Only meaningful on a "filter" query, where the order is the table's.
	Offset int

	// RRFK is the reciprocal-rank-fusion constant for a hybrid query. Zero means the engine's
	// default of 60. It has no effect on a single-mode query.
	RRFK float32

	// Rerank turns the cross-encoder stage on. The zero value leaves it OFF, which is the
	// default and the shipped behaviour — see rerank.go for why.
	Rerank RerankConfig
}

// CompactionResult is what the engine reports it actually did, which is the only honest answer
// to "did compaction help". Counting data files cannot tell you: compaction writes the merged
// fragment and leaves the ones it replaced on disk until the versions referencing them are
// pruned, so the file count goes UP before it goes down.
type CompactionResult struct {
	FragmentsRemoved int64
	FragmentsAdded   int64
	FilesRemoved     int64
	FilesAdded       int64
}

// PruneResult is what pruning reclaimed.
type PruneResult struct {
	BytesRemoved int64
	OldVersions  int64
}

// Version is one entry in a table's history.
//
// Lance is MVCC: every write produces a new immutable version and the previous one stays until
// it is pruned. That makes the history a RECOVERY MECHANISM and not only a curiosity — a
// destructive write is undone by restoring the version before it, which is what lets a store
// with no second copy of its data still be recoverable.
type Version struct {
	// Version is the dataset version number, which is what Checkout and Restore take.
	Version uint64
	// Timestamp is when the version was created.
	Timestamp time.Time
	// Metadata is whatever the writer attached to the commit, if anything.
	Metadata map[string]string
}

// ErrNoTimeTravel is returned when the backend's table does not implement the optional
// version-history capability.
//
// It is a NAMED error rather than a panic or a generic failure because the capability is
// genuinely optional in the binding: `contracts.ITableTimeTravel` is layered beside `ITable`
// and reached by type assertion, so a backend can legitimately lack it. A caller that depends
// on rollback has to be able to detect that and say so, rather than discovering it as a nil
// dereference.
var ErrNoTimeTravel = errors.New("lancestore: this table does not support version history")

// ErrCommitConflict is returned when a write lost a commit race and the retries ran out.
//
// Concurrent writers to the same table are expected on a shared remote store: Lance commits by
// writing a manifest, so two writers that commit at the same moment produce one winner and one
// conflict. The retry loop absorbs the ordinary case; this error is what a caller sees when the
// contention outlasted it.
var ErrCommitConflict = errors.New("lancestore: the write lost its commit race and the retries ran out")

// DefaultLimit is used when a Query sets no Limit.
const DefaultLimit = 20

// Mode reports which of the three searches this query describes.
func (q Query) Mode() string {
	switch {
	case q.Text != "" && len(q.Vector) > 0:
		return "hybrid"
	case len(q.Vector) > 0:
		return "semantic"
	case q.Text != "":
		return "fts"
	case q.Filter != "":
		// A filter with no ranking channel is a LOOKUP, not a search: read the rows matching a
		// predicate, in storage order. It is how a caller fetches a known row by key, and how a
		// test asserts what was actually written rather than what a constructor intended.
		return "filter"
	default:
		return "none"
	}
}

// Validate refuses a query that cannot run.
func (q Query) Validate() error {
	if q.Mode() == "none" {
		return errors.New("lancestore: a query needs Text, Vector or Filter")
	}
	if len(q.Vector) > 0 && q.VectorColumn == "" {
		return errors.New("lancestore: a vector query needs VectorColumn")
	}
	if q.Limit < 0 {
		return fmt.Errorf("lancestore: negative limit %d", q.Limit)
	}
	return nil
}

func (q Query) limit() int {
	if q.Limit <= 0 {
		return DefaultLimit
	}
	return q.Limit
}

// Hit is one result row, with the search's own scoring attached.
type Hit struct {
	// Row is every column the table returned, as Go values.
	Row Row
	// Score is the engine's relevance for this row: BM25 for a text search, and the fused
	// score for a hybrid one. Higher is better.
	//
	// It is the one to rank by. RawScore and RelevanceScore below are the two columns it is
	// chosen from, kept separately because they are not the same number and a hybrid row has
	// both — collapsing them is what made a hybrid ranking depend on map iteration order.
	Score float64

	// RawScore is the channel's own value, from the engine's _score column: BM25 on a text
	// query. On a hybrid query it is the text channel's score BEFORE fusion, so it is not
	// comparable with another row's fused score.
	RawScore float64

	// RelevanceScore is what the engine's reranker produced, from its _relevance_score column.
	// Present only when a fusion ran, which is the hybrid case; zero otherwise.
	RelevanceScore float64
	// Distance is the vector distance, present for a semantic or hybrid search. Lower is
	// closer. It is NOT comparable with Score.
	Distance float64
	// Mode is the search that produced this hit, so a caller can tell a fused row from a
	// single-channel one without tracking it themselves.
	Mode string
}

// String returns the row's most identifying column, for logs.
func (h Hit) String() string {
	for _, k := range []string{"name", "title", "path", "id"} {
		if v, ok := h.Row[k]; ok {
			return fmt.Sprintf("%v", v)
		}
	}
	return fmt.Sprintf("%v", h.Row)
}
