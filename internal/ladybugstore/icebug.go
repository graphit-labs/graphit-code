package ladybugstore

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync/atomic"
	"unicode/utf8"

	"github.com/apache/arrow-go/v18/arrow"
	"github.com/apache/arrow-go/v18/arrow/array"
	"github.com/apache/arrow-go/v18/arrow/memory"
	"github.com/apache/arrow-go/v18/parquet"
	"github.com/apache/arrow-go/v18/parquet/compress"
	"github.com/apache/arrow-go/v18/parquet/pqarrow"
)

// Writing a store as icebug-disk, natively.
//
// THE CONSTRAINT: the format stores ONE CSR per relationship table, and a CSR is inherently
// one (source table -> target table) pair — indptr indexes the source table's dense ids and
// indices.target holds dense ids of the target table. Two consequences, both MEASURED, both
// silent:
//
//   - A relationship table declaring several FROM/TO pairs re-reads the same CSR once per
//     pair and reports N times the edges.
//   - The `[:A|B]` type-alternatives form applies the FIRST alternative's indptr to every
//     alternative, so a relationship type split across pair tables cannot be summed, and a
//     variable-length path across them has no correct form at all.
//
// THE SHAPE THAT FOLLOWS: every node lands in ONE table, `Entity`, carrying its label as a
// column. Then every relationship type has exactly one pair — Entity to Entity — so each type
// keeps its own single table and its own single CSR. `MATCH ()-[:CONTAINS]->()`, `type(r)` and
// `-[:CALLS*1..3]->` are native again, with no alternatives anywhere.
//
// Nothing is lost. Every node keeps every property (the table's columns are the union across
// labels, null where a label does not have one), every edge keeps its properties and its
// direction, self-loops included, and the label survives as data. What moves is the LABEL
// side: `MATCH (f:Function)` becomes a filter on `label`, which a reader rewrites — using only
// predicates measured to work on this storage. See docs/tasks/hub-em-s3-icebug-e-lancedb.md.
// maxRowGroupLength keeps a table in a single Parquet row group.
const maxRowGroupLength = 1 << 40

// parquetChunkRows is how many rows are held as Arrow at once while writing a table.
const parquetChunkRows = 64 << 10

const (
	// IcebugEntityTable is the single node table every label is folded into.
	IcebugEntityTable = "Entity"

	// IcebugLabelColumn carries the label a node had. This is what a reader filters on in
	// place of matching a node table by name.
	IcebugLabelColumn = "label"

	// IcebugReverseSuffix names the derived mirror of a relationship type.
	IcebugReverseSuffix = "_REVERSE"

	// IcebugIDColumn is the primary key: the dense CSR index itself.
	//
	// Using the dense id rather than synthesising a key from the original one removes a whole
	// class of collision — File.path and Directory.path can be the same string — and means the
	// CSR references the key directly instead of through a lookup.
	//
	// NOTE: not `_id`, which the engine rejects with "reserved property name".
	IcebugIDColumn = "entity_id"

	// icebugVersion is stamped into every Parquet file this package writes.
	icebugVersion = "v1"
)

// IcebugManifestFile is the record of a finished export.
const IcebugManifestFile = "icebug.json"

// IcebugOptions configures one export.
type IcebugOptions struct {
	// StorageURI is what each table declares as its `storage`. It is written verbatim into
	// schema.cypher, so it is the S3 prefix when publishing and a local directory when
	// testing.
	StorageURI string

	// DisableReverseEdges omits the separate <TYPE>_REVERSE tables. The zero value
	// exports them.
	//
	// SAFETY: it must never merge the mirror into the type's own table. The reference tool
	// does exactly that, and MEASURED, it destroys direction: a graph of 200.000 edges mounts
	// as 399.996, and `MATCH (a)-[:CALLS]->(b)` starts returning calls that do not exist. In a
	// code graph the direction of CALLS is the meaning.
	//
	DisableReverseEdges bool
}

func (o IcebugOptions) reverseEdgesEnabled() bool {
	return !o.DisableReverseEdges
}

// IcebugManifest records what an export produced, so a consumer can mount and verify it
// without trusting a directory listing.
type IcebugManifest struct {
	Version     string `json:"icebug_disk_version"`
	Storage     string `json:"storage"`
	Reverse     bool   `json:"add_reverse_edges"`
	NodeCount   int64  `json:"n_nodes"`
	EdgeCount   int64  `json:"n_edges"`
	EntityTable string `json:"entity_table"`
	LabelColumn string `json:"label_column"`
	IDColumn    string `json:"id_column"`

	// Labels is what the folded table holds, with the row count each label contributed. It is
	// how a reader answers "which labels exist" without scanning, and how a schema tool
	// reports the labels a remote context has.
	Labels []IcebugLabel `json:"labels"`

	// Columns are the folded table's columns, in the order the file was written.
	Columns []Field `json:"columns"`

	// Rels is one entry per relationship type.
	Rels []IcebugRelTable `json:"rels"`

	// LabelKeys names, per label, the column that WAS its primary key. A rebuild needs it to
	// restore the original schema; a reader needs it to know what identifies a node of that
	// label.
	LabelKeys map[string]string `json:"label_keys"`

	// RepairedStrings counts values that were not valid UTF-8 in the source and were repaired
	// so the Parquet STRING columns are readable. Non-zero means the source graph holds bytes
	// that were never text.
	RepairedStrings int64 `json:"repaired_strings"`

	Schema string `json:"schema_file"`
}

// IcebugLabel is one label folded into the entity table.
type IcebugLabel struct {
	Label string `json:"label"`
	Rows  int64  `json:"rows"`
}

// IcebugRelTable is one exported relationship type.
type IcebugRelTable struct {
	Table      string  `json:"table"`
	Type       string  `json:"type"`
	IndicesRel string  `json:"indices"`
	IndptrRel  string  `json:"indptr"`
	Rows       int64   `json:"rows"`
	Properties []Field `json:"properties"`

	// Reverse marks a derived mirror table. Its rows are not part of the graph's edge count,
	// and a query about direction must never read it as if it were the forward table.
	Reverse bool `json:"reverse,omitempty"`

	// Pairs records the (from, to) label combinations the type actually connects. The CSR does
	// not need them — every edge is Entity to Entity — but a rebuild does, to restore the
	// original FROM/TO declarations.
	Pairs []IcebugPair `json:"pairs"`
}

// IcebugPair is one (from, to) label combination a relationship type connects.
type IcebugPair struct {
	From string `json:"from"`
	To   string `json:"to"`
	Rows int64  `json:"rows"`
}

// utf8Repairs counts string values this process had to repair during an export.
//
// It is diagnostic only, and it is read into the manifest at the end of an export. A Parquet
// STRING column is UTF-8 by definition and the engine REJECTS a file whose string column is
// not — measured: "Invalid string encoding found in Parquet file: value \xD8\x06 is not valid
// UTF8". The source graph does hold such values (a mis-decoded byte in an indexed file), so
// they are repaired rather than dropped: the node and the edge survive, and the count says how
// many bytes were not text to begin with.
var utf8Repairs atomic.Int64

// sanitizeUTF8 makes a value safe for a Parquet STRING column, reporting whether it had to
// change anything.
func sanitizeUTF8(v string) string {
	if utf8.ValidString(v) {
		return v
	}
	utf8Repairs.Add(1)
	return strings.ToValidUTF8(v, "\uFFFD")
}

// Field is one column of a table, with the Cypher type the schema declares.
type Field struct {
	Name string `json:"name"`
	Type string `json:"type"`
}

// HasIcebug reports whether dir holds a finished icebug export.
func HasIcebug(dir string) bool {
	_, err := os.Stat(filepath.Join(dir, IcebugManifestFile))
	return err == nil
}

// ExportIcebug writes the store as an icebug-disk directory.
//
// Every Parquet file here is written by this package rather than by the engine's COPY TO.
// That is not a preference: MEASURED on a real corpus, a node table written by COPY TO is
// rejected by the engine's own icebug reader with "Invalid string encoding found in Parquet
// file … is not valid UTF8" once the table is large enough to change encoding. A two-row
// table survives it, which is why a small fixture cannot catch it.
func ExportIcebug(c Conn, outDir string, opts IcebugOptions) (*IcebugManifest, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("icebug: dir: %w", err)
	}

	utf8Repairs.Store(0)

	tables, err := listTables(c)
	if err != nil {
		return nil, err
	}

	labels := make([]string, 0, len(tables))
	relTypes := make([]string, 0, len(tables))
	for _, tb := range tables {
		if tb.kind == "NODE" {
			labels = append(labels, tb.name)
		} else {
			relTypes = append(relTypes, tb.name)
		}
	}
	// Sorted so an export is reproducible: a node's dense id depends on the order labels are
	// folded in, and a reproducible id is what makes two exports of one store comparable.
	sort.Strings(labels)
	sort.Strings(relTypes)

	folded, err := foldLabels(c, labels)
	if err != nil {
		return nil, err
	}
	if folded.count == 0 {
		return nil, fmt.Errorf("icebug: the store has no nodes")
	}

	man := &IcebugManifest{
		Version:     icebugVersion,
		Storage:     opts.StorageURI,
		Reverse:     opts.reverseEdgesEnabled(),
		EntityTable: IcebugEntityTable,
		LabelColumn: IcebugLabelColumn,
		IDColumn:    IcebugIDColumn,
		NodeCount:   int64(folded.count),
		Columns:     folded.columns,
		Labels:      folded.labels,
		LabelKeys:   folded.keys,
		Schema:      "schema.cypher",
	}

	if err := writeEntityTable(filepath.Join(outDir, "nodes_"+IcebugEntityTable+".parquet"), folded); err != nil {
		return nil, fmt.Errorf("icebug: write entity table: %w", err)
	}

	for _, relType := range relTypes {
		rels, err := exportIcebugRelType(c, relType, folded, outDir, opts)
		if err != nil {
			return nil, err
		}
		for _, rel := range rels {
			// A reverse table is derived, so it does not add to the graph's edge count.
			if !rel.Reverse {
				man.EdgeCount += rel.Rows
			}
			man.Rels = append(man.Rels, rel)
		}
	}

	man.RepairedStrings = utf8Repairs.Load()
	sortRelsLargestFirst(man.Rels)

	if err := writeIcebugSchema(outDir, man, opts); err != nil {
		return nil, err
	}
	raw, err := json.MarshalIndent(man, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("icebug: manifest: %w", err)
	}
	// Written LAST, so its presence means the export finished.
	if err := os.WriteFile(filepath.Join(outDir, IcebugManifestFile), raw, 0o644); err != nil {
		return nil, fmt.Errorf("icebug: write manifest: %w", err)
	}
	return man, nil
}

// foldedGraph is every label's nodes in one dense id space.
type foldedGraph struct {
	columns []Field
	labels  []IcebugLabel
	keys    map[string]string

	// rows holds one map per node, keyed by column name, in dense id order.
	rows []map[string]any

	// ids maps a node's internal identity to its dense id.
	ids   map[string]uint64
	count uint64
}

// foldedKey identifies a node by its label and its INTERNAL offset.
//
// NOT by its declared primary key. MEASURED on this project's own graph: `Comment` declares
// `uid` as its primary key and has 951 uid values appearing twice — the engine does not
// enforce the declaration. Keying on it would attach an edge to whichever twin the map
// happened to hold, silently. `offset(id(n))` is unique by construction (17408 of 17408
// distinct on that same table).
func foldedKey(label string, offset int64) string {
	return label + "\x00" + strconv.FormatInt(offset, 10)
}

// icebugOffsetAlias is the column the internal offset is projected as. It is never written to
// the artifact — the exported key is the dense id.
const icebugOffsetAlias = "__off"

// foldLabels reads every label's nodes into one table.
//
// The column set is the UNION across labels: a property a label does not have is null for its
// nodes, which is what makes folding lossless. A name declared with two different types in two
// labels is refused rather than coerced — silently widening a column is how a value changes
// meaning between publish and read.
func foldLabels(c Conn, labels []string) (*foldedGraph, error) {
	g := &foldedGraph{
		keys: map[string]string{},
		ids:  map[string]uint64{},
	}
	byName := map[string]Field{}
	var order []string

	addColumn := func(f Field) error {
		if prev, seen := byName[f.Name]; seen {
			if prev.Type != f.Type {
				return fmt.Errorf("icebug: column %q is %s in one label and %s in another — folding would change its type",
					f.Name, prev.Type, f.Type)
			}
			return nil
		}
		byName[f.Name] = f
		order = append(order, f.Name)
		return nil
	}

	if err := addColumn(Field{Name: IcebugIDColumn, Type: "INT64"}); err != nil {
		return nil, err
	}
	if err := addColumn(Field{Name: IcebugLabelColumn, Type: "STRING"}); err != nil {
		return nil, err
	}

	type labelPlan struct {
		label string
		pk    string
		cols  []Field
	}
	plans := make([]labelPlan, 0, len(labels))
	for _, label := range labels {
		cols, pk, err := icebugColumns(c, label)
		if err != nil {
			return nil, err
		}
		if pk == "" {
			return nil, fmt.Errorf("icebug: node table %s has no primary key", label)
		}
		for _, f := range cols {
			if f.Name == IcebugIDColumn || f.Name == IcebugLabelColumn {
				return nil, fmt.Errorf("icebug: label %s declares a column named %q, which the folded table reserves",
					label, f.Name)
			}
			if err := addColumn(f); err != nil {
				return nil, err
			}
		}
		plans = append(plans, labelPlan{label: label, pk: pk, cols: cols})
	}

	for _, name := range order {
		g.columns = append(g.columns, byName[name])
	}

	for _, p := range plans {
		proj := make([]string, 0, len(p.cols)+1)
		proj = append(proj, fmt.Sprintf("offset(id(n)) AS %s", QuoteIdent(icebugOffsetAlias)))
		for _, f := range p.cols {
			proj = append(proj, fmt.Sprintf("n.%s AS %s", QuoteIdent(f.Name), QuoteIdent(f.Name)))
		}
		// Ordered by the internal offset, which is deterministic even when the declared
		// primary key repeats — ordering by a non-unique key leaves twins in an arbitrary
		// order and makes the export irreproducible.
		rows, err := c.Query(fmt.Sprintf("MATCH (n:%s) RETURN %s ORDER BY %s",
			QuoteIdent(p.label), strings.Join(proj, ", "), QuoteIdent(icebugOffsetAlias)), nil)
		if err != nil {
			return nil, fmt.Errorf("icebug: read nodes of %s: %w", p.label, err)
		}
		if len(rows) == 0 {
			continue
		}

		g.keys[p.label] = p.pk
		for _, r := range rows {
			id := g.count
			row := make(map[string]any, len(p.cols)+2)
			for k, v := range r {
				if k == icebugOffsetAlias {
					continue
				}
				row[k] = v
			}
			row[IcebugIDColumn] = int64(id)
			row[IcebugLabelColumn] = p.label

			key := foldedKey(p.label, Int64(r[icebugOffsetAlias]))
			if _, clash := g.ids[key]; clash {
				return nil, fmt.Errorf("icebug: %s returned internal offset %d twice — the store is being written while it is read",
					p.label, Int64(r[icebugOffsetAlias]))
			}
			g.ids[key] = id
			g.rows = append(g.rows, row)
			g.count++
		}
		g.labels = append(g.labels, IcebugLabel{Label: p.label, Rows: int64(len(rows))})
	}
	return g, nil
}

// writeEntityTable writes the folded node table, in dense id order.
func writeEntityTable(dest string, g *foldedGraph) error {
	fields := make([]arrow.Field, 0, len(g.columns))
	for _, f := range g.columns {
		fields = append(fields, arrow.Field{
			Name:     f.Name,
			Type:     arrowTypeForCypher(f.Type),
			Nullable: true,
		})
	}
	schema := arrow.NewSchema(fields, icebugMetadata())

	return writeParquet(dest, schema, len(g.rows), func(b *array.RecordBuilder, from, to int) {
		for ci, col := range g.columns {
			builder := b.Field(ci)
			for i := from; i < to; i++ {
				appendArrowValue(builder, g.rows[i][col.Name])
			}
		}
	})
}

// exportIcebugRelType writes one relationship type's CSR over the folded id space.
//
// Every pair the type connects contributes to the SAME CSR, because both endpoints are now
// the entity table. That is the whole point: one type, one table, one CSR, no alternatives.
func exportIcebugRelType(c Conn, relType string, g *foldedGraph, outDir string,
	opts IcebugOptions) ([]IcebugRelTable, error) {

	pairs, err := connectionsOf(c, relType)
	if err != nil {
		return nil, err
	}
	props, _, err := icebugColumns(c, relType)
	if err != nil {
		return nil, err
	}

	var edges []csrEdge
	propValues := make([][]any, len(props))
	var pairInfo []IcebugPair
	var reversePairInfo []IcebugPair

	for _, p := range pairs {
		pattern := fmt.Sprintf("MATCH (a:%s)-[r:%s]->(b:%s)",
			QuoteIdent(p.src), QuoteIdent(relType), QuoteIdent(p.dst))
		// Endpoints are resolved by internal offset, matching how the dense mapping was keyed.
		proj := []string{
			"offset(id(a)) AS __src",
			"offset(id(b)) AS __dst",
		}
		for _, f := range props {
			proj = append(proj, fmt.Sprintf("r.%s AS %s", QuoteIdent(f.Name), QuoteIdent(f.Name)))
		}
		rows, err := c.Query(pattern+" RETURN "+strings.Join(proj, ", "), nil)
		if err != nil {
			return nil, fmt.Errorf("icebug: read %s %s->%s: %w", relType, p.src, p.dst, err)
		}
		if len(rows) == 0 {
			continue
		}

		var kept, mirrored int64
		for _, r := range rows {
			srcID, ok := g.ids[foldedKey(p.src, Int64(r["__src"]))]
			if !ok {
				continue
			}
			dstID, ok := g.ids[foldedKey(p.dst, Int64(r["__dst"]))]
			if !ok {
				continue
			}
			edges = append(edges, csrEdge{source: srcID, target: dstID})
			if srcID != dstID {
				mirrored++
			}
			for i, f := range props {
				propValues[i] = append(propValues[i], r[f.Name])
			}
			kept++
		}
		if kept > 0 {
			pairInfo = append(pairInfo, IcebugPair{From: p.src, To: p.dst, Rows: kept})
		}
		if mirrored > 0 {
			reversePairInfo = append(reversePairInfo, IcebugPair{
				From: p.dst, To: p.src, Rows: mirrored,
			})
		}
	}

	if len(edges) == 0 {
		return nil, nil
	}
	// Sorting is REQUIRED here: edges of one type arrive pair by pair, so they are only
	// ordered within a pair. A CSR needs them ordered across the whole type, by
	// (csr_source, csr_target).
	sortEdges(&edges, propValues)

	indicesFile := "indices_" + relType + ".parquet"
	indptrFile := "indptr_" + relType + ".parquet"

	propFields := make([]arrow.Field, 0, len(props))
	for _, f := range props {
		propFields = append(propFields, arrow.Field{
			Name: f.Name, Type: arrowTypeForCypher(f.Type), Nullable: true,
		})
	}
	if err := writeIndices(filepath.Join(outDir, indicesFile), edges, propFields, propValues); err != nil {
		return nil, fmt.Errorf("icebug: indices %s: %w", relType, err)
	}
	if err := writeIndptr(filepath.Join(outDir, indptrFile), edges, g.count); err != nil {
		return nil, fmt.Errorf("icebug: indptr %s: %w", relType, err)
	}

	out := []IcebugRelTable{{
		Table: relType, Type: relType,
		IndicesRel: indicesFile, IndptrRel: indptrFile,
		Rows: int64(len(edges)), Properties: props, Pairs: pairInfo,
	}}

	if opts.reverseEdgesEnabled() {
		rev, propsRev := reverseEdges(edges, propValues)
		sortEdges(&rev, propsRev)

		revTable := relType + IcebugReverseSuffix
		revIndices := "indices_" + revTable + ".parquet"
		revIndptr := "indptr_" + revTable + ".parquet"
		if err := writeIndices(filepath.Join(outDir, revIndices), rev, propFields, propsRev); err != nil {
			return nil, fmt.Errorf("icebug: indices %s: %w", revTable, err)
		}
		if err := writeIndptr(filepath.Join(outDir, revIndptr), rev, g.count); err != nil {
			return nil, fmt.Errorf("icebug: indptr %s: %w", revTable, err)
		}
		out = append(out, IcebugRelTable{
			Table: revTable, Type: relType, Reverse: true,
			IndicesRel: revIndices, IndptrRel: revIndptr,
			Rows: int64(len(rev)), Properties: props, Pairs: reversePairInfo,
		})
	}
	return out, nil
}

// sortEdges orders edges by (source, target), carrying their property values along.
func sortEdges(edges *[]csrEdge, propValues [][]any) {
	e := *edges
	idx := make([]int, len(e))
	for i := range idx {
		idx[i] = i
	}
	sort.SliceStable(idx, func(a, b int) bool {
		if e[idx[a]].source != e[idx[b]].source {
			return e[idx[a]].source < e[idx[b]].source
		}
		return e[idx[a]].target < e[idx[b]].target
	})

	sorted := make([]csrEdge, len(e))
	for i, from := range idx {
		sorted[i] = e[from]
	}
	*edges = sorted

	for c := range propValues {
		col := propValues[c]
		if len(col) != len(idx) {
			continue
		}
		reordered := make([]any, len(col))
		for i, from := range idx {
			reordered[i] = col[from]
		}
		propValues[c] = reordered
	}
}

// icebugColumns reads a table's columns and primary key from the catalog, mapping engine
// types to the Cypher types schema.cypher declares.
func icebugColumns(c Conn, table string) ([]Field, string, error) {
	rows, err := c.Query(fmt.Sprintf("CALL table_info('%s') RETURN *", table), nil)
	if err != nil {
		return nil, "", fmt.Errorf("icebug: table_info %s: %w", table, err)
	}
	var out []Field
	var pk string
	for _, rec := range rows {
		name := Str(rec["name"])
		typ := Str(rec["type"])
		if name == "" || typ == "" {
			continue
		}
		out = append(out, Field{Name: name, Type: cypherType(typ)})
		if b, ok := rec["primary key"].(bool); ok && b {
			pk = name
		}
	}
	return out, pk, nil
}

// cypherType maps an engine type onto the Cypher type a schema declares. A parameterised
// type keeps only its base, and anything unrecognised becomes STRING, as the format requires.
func cypherType(engineType string) string {
	base := strings.ToUpper(strings.TrimSpace(engineType))
	if i := strings.Index(base, "("); i > 0 {
		base = base[:i]
	}
	switch base {
	case "INT64", "INT32", "INT16", "INT8", "INT128",
		"UINT64", "UINT32", "UINT16", "UINT8",
		"DOUBLE", "FLOAT", "BOOL", "STRING", "DATE", "TIMESTAMP", "TIME", "BLOB",
		"SERIAL", "UUID", "INTERVAL":
		return base
	case "BIGINT":
		return "INT64"
	case "INTEGER", "INT":
		return "INT32"
	case "SMALLINT":
		return "INT16"
	case "TINYINT":
		return "INT8"
	case "HUGEINT":
		return "INT128"
	case "REAL":
		return "FLOAT"
	case "BOOLEAN":
		return "BOOL"
	case "VARCHAR", "TEXT", "CHAR":
		return "STRING"
	default:
		return "STRING"
	}
}

// arrowTypeForCypher is the Arrow type a column is written as.
//
// Anything this does not recognise is written as a string, matching cypherType's own fallback
// so the declared type and the stored type never disagree.
func arrowTypeForCypher(cypher string) arrow.DataType {
	switch cypher {
	case "INT64", "SERIAL":
		return arrow.PrimitiveTypes.Int64
	case "INT32":
		return arrow.PrimitiveTypes.Int32
	case "INT16":
		return arrow.PrimitiveTypes.Int16
	case "INT8":
		return arrow.PrimitiveTypes.Int8
	case "UINT64":
		return arrow.PrimitiveTypes.Uint64
	case "UINT32":
		return arrow.PrimitiveTypes.Uint32
	case "DOUBLE":
		return arrow.PrimitiveTypes.Float64
	case "FLOAT":
		return arrow.PrimitiveTypes.Float32
	case "BOOL":
		return arrow.FixedWidthTypes.Boolean
	case "BLOB":
		return arrow.BinaryTypes.Binary
	default:
		return arrow.BinaryTypes.String
	}
}

// writeIcebugSchema emits the DDL that mounts the export.
//
// One node table and one relationship table per type, each with exactly ONE FROM/TO pair —
// which is the invariant the format requires and the reason for folding the labels.
//
// THE ORDER OF THE RELATIONSHIP TABLES IS LOAD-BEARING. See sortRelsLargestFirst.
func writeIcebugSchema(outDir string, man *IcebugManifest, opts IcebugOptions) error {
	var b strings.Builder
	storage := EscapeLiteral(opts.StorageURI)

	cols := make([]string, 0, len(man.Columns))
	for _, f := range man.Columns {
		cols = append(cols, fmt.Sprintf("%s %s", QuoteIdent(f.Name), f.Type))
	}
	fmt.Fprintf(&b, "CREATE NODE TABLE %s(%s, PRIMARY KEY(%s)) WITH (storage = '%s', format = 'icebug-disk');\n",
		QuoteIdent(IcebugEntityTable), strings.Join(cols, ", "), QuoteIdent(IcebugIDColumn), storage)

	for _, r := range man.Rels {
		parts := []string{fmt.Sprintf("FROM %s TO %s",
			QuoteIdent(IcebugEntityTable), QuoteIdent(IcebugEntityTable))}
		for _, f := range r.Properties {
			parts = append(parts, fmt.Sprintf("%s %s", QuoteIdent(f.Name), f.Type))
		}
		fmt.Fprintf(&b, "CREATE REL TABLE %s(%s) WITH (storage = '%s', format = 'icebug-disk');\n",
			QuoteIdent(r.Table), strings.Join(parts, ", "), storage)
	}

	return os.WriteFile(filepath.Join(outDir, "schema.cypher"), []byte(b.String()), 0o644)
}

// sortRelsLargestFirst orders the relationship tables by descending row count, which is the
// order they are CREATEd in and therefore the order of their table ids.
//
// THIS IS A CORRECTNESS FIX, NOT A TIDY-UP, and it is the one thing standing between this export
// and a silently wrong `[:A|B|…]`.
//
// MEASURED, on our export AND on the reference tool's own output: the engine bounds EVERY
// alternative of a type-alternatives pattern by the row count of the FIRST alternative — the one
// with the lowest table id, which is creation order. So a later alternative holding more edges
// than the first is silently truncated to the first's count:
//
//	first 54.823 then 92.396 -> 109.646   (= 2x54.823, the second one truncated)
//	first 92.396 then 54.823 -> 147.219   (correct: nothing to truncate)
//
// Query order does not matter; only creation order does. And the bound is the FIRST alternative's
// count, not the minimum over them — verified with three tables. So ordering the tables by
// descending row count makes the lowest-id member of ANY subset also the largest member of that
// subset, no alternative is ever truncated, and the form is correct for every combination.
//
// This is why the defect looked like it belonged to a single pair of tables: with the tables in
// alphabetical order, only the 9 pairs whose alphabetically-first table was also the smaller one
// were wrong, and the other 19 came out right because there was nothing to truncate. The
// reference tool was cleared of it for the same reason — the one comparison ever run against it
// happened to declare the bigger table first.
func sortRelsLargestFirst(rels []IcebugRelTable) {
	sort.SliceStable(rels, func(i, j int) bool {
		if rels[i].Rows != rels[j].Rows {
			return rels[i].Rows > rels[j].Rows
		}
		// A tie broken by name keeps an export reproducible.
		return rels[i].Table < rels[j].Table
	})
}

type csrEdge struct {
	source uint64
	target uint64
}

// reverseEdges is the mirror of every non-self-loop edge, carrying the same properties.
//
// A self-loop is left out: its mirror is itself, so including it would make the reverse table
// report an edge the forward table already has.
func reverseEdges(edges []csrEdge, propValues [][]any) ([]csrEdge, [][]any) {
	outEdges := make([]csrEdge, 0, len(edges))
	outProps := make([][]any, len(propValues))
	for c := range outProps {
		outProps[c] = make([]any, 0, len(edges))
	}

	for i, e := range edges {
		if e.source == e.target {
			continue
		}
		outEdges = append(outEdges, csrEdge{source: e.target, target: e.source})
		for c := range propValues {
			outProps[c] = append(outProps[c], propValues[c][i])
		}
	}
	return outEdges, outProps
}

// writeIndices writes the target column plus every edge property, in CSR row order.
func writeIndices(dest string, edges []csrEdge, propFields []arrow.Field, propValues [][]any) error {
	fields := make([]arrow.Field, 0, len(propFields)+1)
	// Nullable to match what the reference tool writes: its columns are `optional`, ours were
	// `required`, and the reader is the only consumer that gets to have an opinion.
	fields = append(fields, arrow.Field{Name: "target", Type: arrow.PrimitiveTypes.Uint64, Nullable: true})
	fields = append(fields, propFields...)
	schema := arrow.NewSchema(fields, icebugMetadata())

	return writeParquet(dest, schema, len(edges), func(b *array.RecordBuilder, from, to int) {
		tb := b.Field(0).(*array.Uint64Builder)
		for i := from; i < to; i++ {
			tb.Append(edges[i].target)
		}
		for c, vals := range propValues {
			col := b.Field(c + 1)
			for i := from; i < to; i++ {
				appendArrowValue(col, vals[i])
			}
		}
	})
}

// writeIndptr writes the N+1 offsets, including a source node with no outgoing edge.
func writeIndptr(dest string, edges []csrEdge, nodeCount uint64) error {
	ptr := make([]uint64, nodeCount+1)
	for _, e := range edges {
		if e.source < nodeCount {
			ptr[e.source+1]++
		}
	}
	for i := uint64(1); i <= nodeCount; i++ {
		ptr[i] += ptr[i-1]
	}

	schema := arrow.NewSchema(
		[]arrow.Field{{Name: "ptr", Type: arrow.PrimitiveTypes.Uint64, Nullable: true}}, icebugMetadata())
	return writeParquet(dest, schema, len(ptr), func(b *array.RecordBuilder, from, to int) {
		pb := b.Field(0).(*array.Uint64Builder)
		for i := from; i < to; i++ {
			pb.Append(ptr[i])
		}
	})
}

func icebugMetadata() *arrow.Metadata {
	md := arrow.NewMetadata([]string{"icebug_disk_version"}, []string{icebugVersion})
	return &md
}

// writeParquet writes the whole table as ONE row group.
//
// SAFETY: the row group count is load-bearing, and `pqarrow` makes it easy to get wrong —
// every call to FileWriter.Write starts a NEW row group, so batching the rows produced 49 row
// groups where the reference tool produces 1, and `parquet.WithMaxRowGroupLength` does not
// merge them. MEASURED: a multi-row-group file mounts, counts correctly through an anonymous
// pattern, and then fails to resolve a node when the pattern binds one — while the reference
// tool's single-row-group file answers both. WriteBuffered is the one form that batches
// without that cost: it appends into the CURRENT row group while the running total stays
// under MaxRowGroupLength, so the table is fed in chunks and still lands in one group.
//
// Dictionary encoding is off. Tested BOTH ways on the real corpus: it makes no difference to
// correctness, so this is not a safety constraint. What did produce "Invalid string encoding
// … is not valid UTF8" was the ENGINE's own COPY TO output, not ours, and the real fix for that
// was sanitizeUTF8. Left off because plain encoding is the simpler thing to reason about when
// comparing our files against the reference tool's.
func writeParquet(dest string, schema *arrow.Schema, rows int, fill func(*array.RecordBuilder, int, int)) error {
	f, err := os.Create(dest)
	if err != nil {
		return err
	}
	defer f.Close()

	props := parquet.NewWriterProperties(
		parquet.WithCompression(compress.Codecs.Zstd),
		parquet.WithDictionaryDefault(false),
		parquet.WithMaxRowGroupLength(maxRowGroupLength),
	)
	w, err := pqarrow.NewFileWriter(schema, f, props, pqarrow.DefaultWriterProps())
	if err != nil {
		return err
	}

	builder := array.NewRecordBuilder(memory.DefaultAllocator, schema)
	defer builder.Release()

	writeChunk := func(from, to int) error {
		fill(builder, from, to)
		rec := builder.NewRecordBatch()
		defer rec.Release()
		return w.WriteBuffered(rec)
	}
	for from := 0; from < rows; from += parquetChunkRows {
		to := from + parquetChunkRows
		if to > rows {
			to = rows
		}
		if err := writeChunk(from, to); err != nil {
			_ = w.Close()
			return err
		}
	}
	if rows == 0 {
		if err := writeChunk(0, 0); err != nil {
			_ = w.Close()
			return err
		}
	}
	return w.Close()
}

func appendArrowValue(b array.Builder, v any) {
	if v == nil {
		b.AppendNull()
		return
	}
	switch bb := b.(type) {
	case *array.StringBuilder:
		bb.Append(sanitizeUTF8(Str(v)))
	case *array.LargeStringBuilder:
		bb.Append(sanitizeUTF8(Str(v)))
	case *array.Int64Builder:
		bb.Append(Int64(v))
	case *array.Int32Builder:
		bb.Append(int32(Int64(v)))
	case *array.Int16Builder:
		bb.Append(int16(Int64(v)))
	case *array.Int8Builder:
		bb.Append(int8(Int64(v)))
	case *array.Uint64Builder:
		bb.Append(uint64(Int64(v)))
	case *array.Uint32Builder:
		bb.Append(uint32(Int64(v)))
	case *array.Float64Builder:
		if n, ok := v.(float64); ok {
			bb.Append(n)
		} else {
			bb.Append(float64(Int64(v)))
		}
	case *array.Float32Builder:
		if n, ok := v.(float64); ok {
			bb.Append(float32(n))
		} else {
			bb.Append(float32(Int64(v)))
		}
	case *array.BooleanBuilder:
		if x, ok := v.(bool); ok {
			bb.Append(x)
		} else {
			bb.AppendNull()
		}
	case *array.BinaryBuilder:
		if x, ok := v.([]byte); ok {
			bb.Append(x)
		} else {
			bb.Append([]byte(Str(v)))
		}
	default:
		// A builder this does not know would otherwise drop the value silently.
		b.AppendNull()
	}
}
