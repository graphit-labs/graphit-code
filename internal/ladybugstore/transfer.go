package ladybugstore

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Moving a built store between machines: every table exported to Parquet, plus a manifest
// that also carries the CREATE statements that produced it.
//
// This lives here rather than in the package that first needed it because both stores in
// this project have the same shape of problem and neither may import the other. A Hub
// artifact — an AST context or a published documentation wiki — is built by a publisher and
// never changed by a consumer. Shipping the inputs and having every consumer re-derive the same
// result is work done N times for a value that was already computed once.
//
// Two things deliberately do NOT travel:
//
//   - Indexes. FTS and vector indexes are engine structures rather than data, so the
//     consumer builds them. That is also why Import does not build them: what a store needs
//     indexed is the owning package's knowledge, not this one's.
//   - The database file. Its on-disk layout belongs to the engine and carries a storage
//     format version that breaks across releases, with the newer binary upgrading in
//     PLACE — which would silently mutate a shared, version-scoped store that other
//     projects point at and older binaries can still open. Parquet is not an engine format,
//     so an artifact outlives the engine version that produced it.
const bundleVersion = 1

// BundleDir is where an artifact keeps its exported tables, relative to the artifact root.
const BundleDir = "graph"

const bundleFile = "tables.json"

// BundleTable is one exported file.
type BundleTable struct {
	Table string `json:"table"`
	Kind  string `json:"kind"` // "node" or "rel"
	// From and To name the node tables of this pair, empty for a node table. A
	// relationship table may declare several pairs, and COPY then requires the pair to be
	// named on the way back in — the engine rejects the ambiguous form rather than guessing.
	From string `json:"from,omitempty"`
	To   string `json:"to,omitempty"`
	File string `json:"file"`
	Rows int64  `json:"rows"`
}

// Bundle is the manifest of an exported store.
//
// DDL carries the exact CREATE statements of the store that produced it, in dependency
// order. That is what makes a bundle self-describing, and it is not optional: a schema can
// depend on what the producer saw — which labels a parse produced, which pairs a
// relationship joins — and a consumer replaying a bundle has no such input to derive it
// from.
type Bundle struct {
	Version int           `json:"v"`
	DDL     []string      `json:"ddl"`
	Tables  []BundleTable `json:"tables"`
}

// Conn is the subset of a store this file needs. It exists so the export can run against a
// handle the caller already holds — the engine allows one read-write handle per database,
// so opening a second one here would fail rather than wait.
type Conn interface {
	Exec(cypher string, params map[string]any) error
	Query(cypher string, params map[string]any) ([]map[string]any, error)
}

// HasBundle reports whether dir holds a finished export.
func HasBundle(dir string) bool {
	_, err := os.Stat(filepath.Join(dir, BundleDir, bundleFile))
	return err == nil
}

// ExportTables writes every non-empty table into outDir.
func ExportTables(c Conn, outDir string) (*Bundle, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("export: dir: %w", err)
	}
	tables, err := listTables(c)
	if err != nil {
		return nil, err
	}

	bundle := &Bundle{Version: bundleVersion}
	for _, pass := range []string{"NODE", "REL"} {
		for _, tb := range tables {
			if tb.kind != pass {
				continue
			}
			stmt, err := describeTable(c, tb)
			if err != nil {
				return nil, err
			}
			if stmt != "" {
				bundle.DDL = append(bundle.DDL, stmt)
			}
		}
	}

	for _, tb := range tables {
		var exported []BundleTable
		var expErr error
		if tb.kind == "NODE" {
			exported, expErr = exportNodeTable(c, tb.name, outDir)
		} else {
			exported, expErr = exportRelTable(c, tb.name, outDir)
		}
		if expErr != nil {
			return nil, expErr
		}
		bundle.Tables = append(bundle.Tables, exported...)
	}

	raw, err := json.MarshalIndent(bundle, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("export: manifest: %w", err)
	}
	if err := os.WriteFile(filepath.Join(outDir, bundleFile), raw, 0o644); err != nil {
		return nil, fmt.Errorf("export: write manifest: %w", err)
	}
	return bundle, nil
}

// ImportTables creates the bundle's schema and loads every file.
//
// It does NOT build indexes: which of a store's tables need a full-text or vector index is
// the owning package's knowledge. Callers build them after, over rows already present,
// which is also the cheaper order.
func ImportTables(c Conn, inDir string) (*Bundle, error) {
	raw, err := os.ReadFile(filepath.Join(inDir, bundleFile))
	if err != nil {
		return nil, fmt.Errorf("import: read manifest: %w", err)
	}
	var bundle Bundle
	if err := json.Unmarshal(raw, &bundle); err != nil {
		return nil, fmt.Errorf("import: parse manifest: %w", err)
	}
	if bundle.Version != bundleVersion {
		return nil, fmt.Errorf("import: bundle version %d, this build writes %d",
			bundle.Version, bundleVersion)
	}

	for _, stmt := range bundle.DDL {
		if err := c.Exec(stmt, nil); err != nil && !IsAlreadyExists(err) {
			return nil, fmt.Errorf("import: schema: %w", err)
		}
	}

	for _, pass := range []string{"node", "rel"} {
		for _, t := range bundle.Tables {
			if t.Kind != pass {
				continue
			}
			q := fmt.Sprintf("COPY %s FROM '%s'",
				QuoteIdent(t.Table), EscapeLiteral(filepath.Join(inDir, t.File)))
			if t.Kind == "rel" {
				// Named unconditionally, not only for multi-pair tables: the engine
				// accepts the qualifier on a single-pair table too, and a schema that
				// grows a second pair upstream must not silently change what an existing
				// bundle means.
				q += fmt.Sprintf(" (from='%s', to='%s')", t.From, t.To)
			}
			if err := c.Exec(q, nil); err != nil {
				return nil, fmt.Errorf("import: copy %s: %w", t.Table, err)
			}
		}
	}
	return &bundle, nil
}

type tableRef struct {
	name string
	kind string
}

func listTables(c Conn) ([]tableRef, error) {
	rows, err := c.Query("CALL show_tables() RETURN name, type", nil)
	if err != nil {
		return nil, fmt.Errorf("export: list tables: %w", err)
	}
	out := make([]tableRef, 0, len(rows))
	for _, rec := range rows {
		name := Str(rec["name"])
		kind := strings.ToUpper(Str(rec["type"]))
		if name == "" || (kind != "NODE" && kind != "REL") {
			continue
		}
		out = append(out, tableRef{name: name, kind: kind})
	}
	return out, nil
}

func describeTable(c Conn, tb tableRef) (string, error) {
	rows, err := c.Query(fmt.Sprintf("CALL table_info('%s') RETURN *", tb.name), nil)
	if err != nil {
		return "", fmt.Errorf("export: table_info %s: %w", tb.name, err)
	}
	var cols []string
	var pk string
	for _, rec := range rows {
		name := Str(rec["name"])
		typ := Str(rec["type"])
		if name == "" || typ == "" {
			continue
		}
		cols = append(cols, fmt.Sprintf("%s %s", QuoteIdent(name), typ))
		if b, ok := rec["primary key"].(bool); ok && b {
			pk = name
		}
	}

	if tb.kind == "NODE" {
		if pk == "" {
			return "", fmt.Errorf("export: node table %s has no primary key", tb.name)
		}
		return fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS %s(%s, PRIMARY KEY (%s))",
			QuoteIdent(tb.name), strings.Join(cols, ", "), QuoteIdent(pk)), nil
	}

	pairs, err := connectionsOf(c, tb.name)
	if err != nil {
		return "", err
	}
	if len(pairs) == 0 {
		return "", nil
	}
	parts := make([]string, 0, len(pairs)+len(cols))
	for _, p := range pairs {
		parts = append(parts, fmt.Sprintf("FROM %s TO %s", QuoteIdent(p.src), QuoteIdent(p.dst)))
	}
	parts = append(parts, cols...)
	return fmt.Sprintf("CREATE REL TABLE IF NOT EXISTS %s(%s)",
		QuoteIdent(tb.name), strings.Join(parts, ", ")), nil
}

type relPair struct{ src, dst, srcKey, dstKey string }

func connectionsOf(c Conn, table string) ([]relPair, error) {
	rows, err := c.Query(fmt.Sprintf("CALL show_connection('%s') RETURN *", table), nil)
	if err != nil {
		return nil, fmt.Errorf("export: connections of %s: %w", table, err)
	}
	var out []relPair
	for _, rec := range rows {
		p := relPair{
			src:    Str(rec["source table name"]),
			dst:    Str(rec["destination table name"]),
			srcKey: Str(rec["source table primary key"]),
			dstKey: Str(rec["destination table primary key"]),
		}
		if p.src == "" || p.dst == "" {
			continue
		}
		out = append(out, p)
	}
	return out, nil
}

func exportNodeTable(c Conn, table, outDir string) ([]BundleTable, error) {
	n, err := countRows(c, fmt.Sprintf("MATCH (n:%s) RETURN count(n) AS c", QuoteIdent(table)))
	if err != nil {
		return nil, fmt.Errorf("export: count %s: %w", table, err)
	}
	if n == 0 {
		return nil, nil
	}
	file := fmt.Sprintf("node-%s.parquet", table)
	// n.* expands in the order the table declares its columns, which is the order COPY
	// FROM expects — mapping is by POSITION, not by name. Listing columns by hand would
	// make that a convention both sides must keep; letting the engine expand it makes it a
	// contract the engine keeps.
	q := fmt.Sprintf("COPY (MATCH (n:%s) RETURN n.*) TO '%s'",
		QuoteIdent(table), EscapeLiteral(filepath.Join(outDir, file)))
	if err := c.Exec(q, nil); err != nil {
		return nil, fmt.Errorf("export: %s: %w", table, err)
	}
	return []BundleTable{{Table: table, Kind: "node", File: file, Rows: n}}, nil
}

func exportRelTable(c Conn, table, outDir string) ([]BundleTable, error) {
	pairs, err := connectionsOf(c, table)
	if err != nil {
		return nil, err
	}
	var out []BundleTable
	for _, p := range pairs {
		if p.srcKey == "" || p.dstKey == "" {
			continue
		}
		pattern := fmt.Sprintf("MATCH (a:%s)-[r:%s]->(b:%s)",
			QuoteIdent(p.src), QuoteIdent(table), QuoteIdent(p.dst))
		n, err := countRows(c, pattern+" RETURN count(r) AS c")
		if err != nil {
			return nil, fmt.Errorf("export: count %s: %w", table, err)
		}
		if n == 0 {
			continue
		}
		file := fmt.Sprintf("rel-%s-%s-%s.parquet", table, p.src, p.dst)
		q := fmt.Sprintf("COPY (%s RETURN a.%s, b.%s, r.*) TO '%s'",
			pattern, QuoteIdent(p.srcKey), QuoteIdent(p.dstKey),
			EscapeLiteral(filepath.Join(outDir, file)))
		if err := c.Exec(q, nil); err != nil {
			return nil, fmt.Errorf("export: %s (%s->%s): %w", table, p.src, p.dst, err)
		}
		out = append(out, BundleTable{
			Table: table, Kind: "rel", From: p.src, To: p.dst, File: file, Rows: n,
		})
	}
	return out, nil
}

func countRows(c Conn, query string) (int64, error) {
	rows, err := c.Query(query, nil)
	if err != nil {
		return 0, err
	}
	if len(rows) == 0 {
		return 0, nil
	}
	return Int64(rows[0]["c"]), nil
}

// QuoteIdent backticks a label so a name that collides with a keyword still binds.
func QuoteIdent(name string) string {
	return "`" + strings.ReplaceAll(name, "`", "") + "`"
}

// EscapeLiteral makes a filesystem path safe inside a single-quoted Cypher literal. The
// paths here are built by this package, but a store directory is user-supplied and a quote
// in it would otherwise end the literal early.
func EscapeLiteral(p string) string {
	return strings.ReplaceAll(strings.ReplaceAll(p, `\`, `\\`), `'`, `\'`)
}
