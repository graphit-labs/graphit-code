package ast

import (
	"bufio"
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

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

// MountIcebugGraph prepares the mount cache for a published graph by staging its DDL.
//
// NOTHING OF THE GRAPH IS READ HERE. The statements name an `s3://` location and the engine
// resolves it lazily, on the first traversal — so this is fast and stays fast as the graph grows,
// which is the entire point. The catalog itself is built IN-MEMORY, per connection, from the
// staged schema.cypher; no ladybugdb file ever exists.
//
// schemaCypher is the published `schema.cypher`, verbatim. It is not rewritten: the publisher
// already wrote the consumer's location into it, because the publisher is the only party that
// knows where it put the objects.
func MountIcebugGraph(ctx context.Context, storeDir, schemaCypher string, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)
	stmts := splitCypherStatements(schemaCypher)
	if len(stmts) == 0 {
		return fmt.Errorf("mounting the graph: the published schema has no statements")
	}

	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		return fmt.Errorf("mounting the graph: %w", err)
	}
	if err := os.WriteFile(filepath.Join(storeDir, IcebugSchemaFile), []byte(schemaCypher), 0o644); err != nil {
		return fmt.Errorf("mounting the graph: staging schema: %w", err)
	}

	log.Info("graph mount staged for in-memory build",
		"statements", len(stmts), "bytes_transferred", 0)
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
