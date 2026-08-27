package ast

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// A Hub AST artifact is icebug, and it is MOUNTED rather than loaded.
//
// This replaces the Parquet path entirely. That path exported the graph table by table and the
// consumer loaded the rows into a database of its own — which meant the bytes travelled, the
// indexes were rebuilt per consumer, and every project pinned to a version kept its own copy of
// the same immutable graph.
//
// Icebug inverts it. The publisher writes one CSR per relationship table plus a folded node table,
// and a `schema.cypher` whose every `CREATE ... WITH (storage = '…', format = 'icebug-disk')`
// names the location. Installing runs that DDL against an empty local database: the CATALOG is
// local, the DATA stays where it was published, and a traversal reads the objects it needs.
//
// WHAT COMES DOWN ON INSTALL is the schema and the manifest — two files, a few kilobytes. That is
// metadata, not graph: without the DDL there is nothing to point at the objects, and it is the
// same information the DDL would have to be reconstructed from anyway.
//
// THE KNOWN GAPS are recorded with measurements in
// docs/tasks/hub-em-s3-icebug-e-lancedb.md:
//
//   - THE NATIVE MULTI-HOP PLAN over a mounted graph enumerates the node table before recursive
//     expansion and times out on the real corpus. LadybugBackend.Query recognizes a deliberately
//     narrow, bounded reachability subset and executes it as selective one-hop CSR frontiers;
//     unsupported recursive Cypher still reaches the upstream planner and may retain that cost.
//   - A RELATIONSHIP TABLE HOLDS ONE CSR, so it can declare exactly one FROM/TO pair. This graph
//     has ~97 distinct pairs, which is why every label is folded into one `Entity` table and the
//     label becomes a column. `MATCH (f:Function)` therefore has to be written
//     `MATCH (e:Entity {label: 'Function'})` against a mounted context.
//   - EVERY PARQUET HAS EXACTLY ONE ROW GROUP. Multiple groups make the current Icebug reader
//     return silently wrong bound-endpoint results at scale. Row-group pruning is therefore not an
//     available optimization; traversal performance comes from the direct and reverse CSR tables.
//
// The trade was made deliberately: an installed context that answers selective bounded traversal
// over objects nobody downloaded is worth more than one that answers everything after copying a
// gigabyte, and the alternative on offer was neither.

// IcebugBundleDir is where an artifact keeps its mounted graph, relative to its root.
const IcebugBundleDir = "graph.icebug"

// IcebugSchemaFile is the DDL the consumer runs. Written by the exporter, read by the installer.
const IcebugSchemaFile = "schema.cypher"

// SearchBundleDir is where an artifact keeps its search index, beside the graph.
//
// The directory is copied, not converted: it is already the format the engine queries, and it
// carries its own inverted and vector indexes — so a consumer neither rebuilds nor downloads it.
const SearchBundleDir = LanceIndexDirName

// HasIcebugBundle reports whether dir holds a finished icebug export.
func HasIcebugBundle(dir string) bool {
	return ladybug.HasIcebug(filepath.Join(dir, IcebugBundleDir))
}

// IcebugBundlePath is the mounted-graph directory inside an artifact root.
func IcebugBundlePath(root string) string { return filepath.Join(root, IcebugBundleDir) }

// ExportGraphToIcebug writes the graph at dbPath as an icebug directory under outDir, and copies
// the search index beside it.
//
// storageURI is what every table will declare as its `storage`, so it must be the location the
// CONSUMER will read from — the artifact's own prefix on object storage — not the directory being
// written now. Getting that wrong produces an artifact that mounts against the publisher's local
// disk and fails everywhere else, which is why it is a required argument rather than derived here.
func ExportGraphToIcebug(dbPath, outDir, searchDir, storageURI string, reverseEdges bool, logger *slog.Logger) (*ladybug.CanonicalManifest, error) {
	log := slogutil.Resolve(logger)
	if strings.TrimSpace(storageURI) == "" {
		return nil, fmt.Errorf("icebug export: no storage URI — the artifact would mount against " +
			"the publisher's disk")
	}

	be := NewLadybugDBReadOnly(LadybugConfig{DBPath: dbPath})
	if err := be.connect(); err != nil {
		return nil, fmt.Errorf("icebug export: open store: %w", err)
	}
	defer func() { _ = be.Close() }()

	start := time.Now()
	man, err := ladybug.ExportIcebugCanonical(backendConn{be}, outDir, ladybug.IcebugOptions{
		StorageURI:          storageURI,
		DisableReverseEdges: !reverseEdges,
	})
	if err != nil {
		return nil, fmt.Errorf("icebug export: %w", err)
	}

	searchBytes := int64(0)
	if searchDir != "" {
		n, cErr := copyLanceIndex(LanceIndexPath(dbPath), searchDir)
		switch {
		case cErr != nil:
			log.Warn("icebug export: the search index could not be copied; the artifact will be "+
				"traversable but neither searchable nor readable", "error", cErr)
		case n == 0:
			log.Warn("icebug export: no search index beside the graph; the artifact will be " +
				"traversable but neither searchable nor readable")
		default:
			searchBytes = n
		}
	}

	log.Info("icebug export complete",
		"nodes", len(man.NodeTables), "edges", man.EdgeCount, "rel_tables", len(man.RelGroups),
		"search_bytes", searchBytes, "storage", storageURI,
		"duration_s", time.Since(start).Seconds())
	return man, nil
}

// MountIcebugGraph creates the local catalog for a published graph by running its DDL.
//
// NOTHING OF THE GRAPH IS READ HERE. The statements name an `s3://` location and the engine
// resolves it lazily, on the first traversal — so this is fast and stays fast as the graph grows,
// which is the entire point.
//
// schemaCypher is the published `schema.cypher`, verbatim. It is not rewritten: the publisher
// already wrote the consumer's location into it, because the publisher is the only party that
// knows where it put the objects.
func MountIcebugGraph(ctx context.Context, dbPath, schemaCypher string, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)
	stmts := splitCypherStatements(schemaCypher)
	if len(stmts) == 0 {
		return fmt.Errorf("mounting the graph: the published schema has no statements")
	}

	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
		return fmt.Errorf("mounting the graph: %w", err)
	}

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := db.connect(); err != nil {
		return fmt.Errorf("mounting the graph: open local catalog: %w", err)
	}
	defer func() { _ = db.Close() }()

	conn := backendConn{db}
	start := time.Now()
	for i, stmt := range stmts {
		if err := conn.Exec(stmt, nil); err != nil {
			// The statement is named in the error because a DDL failure here is almost always one
			// table's column type, and without it the message says only that mounting failed.
			return fmt.Errorf("mounting the graph: statement %d of %d failed: %w\n  %s",
				i+1, len(stmts), err, firstDDLLine(stmt))
		}
	}

	log.Info("graph mounted from object storage",
		"statements", len(stmts), "bytes_transferred", 0,
		"duration_ms", time.Since(start).Seconds()*1000)
	return nil
}

// splitCypherStatements splits a DDL file on semicolons at end of line.
//
// Deliberately simple, and safe for exactly this input: the file is machine-written by
// writeIcebugSchema, one statement per line, and the only string literals in it are the storage
// URI and column names — neither of which can contain a newline. A general Cypher splitter would
// be more code defending against inputs this function never sees.
func splitCypherStatements(src string) []string {
	var out []string
	sc := bufio.NewScanner(strings.NewReader(src))
	sc.Buffer(make([]byte, 0, 64*1024), 8*1024*1024)
	var cur strings.Builder
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "//") {
			continue
		}
		cur.WriteString(line)
		if strings.HasSuffix(line, ";") {
			stmt := strings.TrimSuffix(cur.String(), ";")
			if s := strings.TrimSpace(stmt); s != "" {
				out = append(out, s)
			}
			cur.Reset()
			continue
		}
		cur.WriteByte(' ')
	}
	if s := strings.TrimSpace(cur.String()); s != "" {
		out = append(out, s)
	}
	return out
}

func firstDDLLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		s = s[:i]
	}
	if len(s) > 160 {
		return s[:160] + "…"
	}
	return s
}

// ---------- the pieces the transfer needs ----------

// backendConn adapts a LadybugBackend to the transfer package's Conn.
//
// It exists so the export can run on a handle the caller already holds: the engine allows
// exactly one read-write handle per database, so opening a second store on the same path
// would fail rather than wait.
type backendConn struct{ be *LadybugBackend }

func (c backendConn) Exec(cypher string, params map[string]any) error {
	if len(params) == 0 {
		return c.be.execQuery(cypher)
	}
	_, err := c.be.Query(context.Background(), cypher, params)
	return err
}

func (c backendConn) Query(cypher string, params map[string]any) ([]map[string]any, error) {
	res, err := c.be.Query(context.Background(), cypher, params)
	if err != nil {
		return nil, err
	}
	out := make([]map[string]any, 0, len(res.Records))
	for _, rec := range res.Records {
		out = append(out, map[string]any(rec))
	}
	return out, nil
}

// copyLanceIndex copies a Lance directory into the artifact, returning the bytes written.
//
// A plain recursive copy is correct here in a way it would not be for a live database file: a
// Lance directory is immutable data files plus a manifest, so a copy taken while nothing is
// writing is a valid dataset, and the publisher holds the store closed for exactly that reason.
func copyLanceIndex(srcDir, dstDir string) (int64, error) {
	info, err := os.Stat(srcDir)
	if err != nil || !info.IsDir() {
		return 0, nil
	}
	var written int64
	err = filepath.Walk(srcDir, func(path string, fi os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(srcDir, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dstDir, rel)
		if fi.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		if !fi.Mode().IsRegular() {
			return nil
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		in, err := os.Open(path)
		if err != nil {
			return err
		}
		defer func() { _ = in.Close() }()
		out, err := os.Create(target)
		if err != nil {
			return err
		}
		defer func() { _ = out.Close() }()
		n, err := io.Copy(out, in)
		written += n
		return err
	})
	return written, err
}
