package wiki

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
)

// A published documentation wiki travels as its index, copied rather than converted.
//
// It is the same case as a Hub AST context, and for the same reason — which is not size but
// mutability. A knowledge artifact is written by ONE project, pinned by its version, and never
// compiled by a consumer, so having every consumer re-derive the same frozen result from shards
// repeats work for a value already computed.
//
// A MEMORY wiki is deliberately not eligible. It is read-and-write and multi-writer: a consumer
// adds to it and pushes back, so it must carry its source, and an index built by someone else
// would be something to overwrite rather than something to extend. The distinction is mutability,
// not file format.
//
// WHAT CHANGED WITH THE ENGINE. This used to export table by table into Parquet, and the consumer
// loaded the rows and then built the inverted and vector indexes itself — because indexes are
// engine structure and did not travel. A Lance directory carries its own, so nothing is rebuilt on
// install and nothing is converted on publish: the directory IS the queryable artifact, which is
// also what lets it be read straight off S3 without being downloaded at all.

// BundleDir is where an artifact keeps its index, relative to its root.
const BundleDir = WikiIndexDirName

// HasBundle reports whether dir holds a finished export.
func HasBundle(dir string) bool {
	info, err := os.Stat(BundlePath(dir))
	return err == nil && info.IsDir()
}

// BundlePath is the index directory inside an artifact root.
func BundlePath(root string) string { return filepath.Join(root, BundleDir) }

// ExportToParquet copies the index of the wiki at wikiDir into outDir.
//
// The name is kept because callers and artifact layouts use it, and because what it produces is
// still a directory of Parquet-family files — Lance's data files are Parquet-encoded. What is no
// longer true is that anything is transformed on the way out.
func ExportToParquet(ctx context.Context, wikiDir, outDir string) (int64, error) {
	src := WikiIndexPath(wikiDir)
	info, err := os.Stat(src)
	if err != nil || !info.IsDir() {
		return 0, fmt.Errorf("wiki export: no index at %s", src)
	}
	// Opened and closed first, so a corrupt index fails here rather than on the consumer's first
	// query. Nothing is written; this is a liveness check on what is about to be published.
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return 0, fmt.Errorf("wiki export: the index does not open: %w", err)
	}
	if !db.HasContent(ctx) {
		_ = db.Close()
		return 0, fmt.Errorf("wiki export: the index at %s is empty", src)
	}
	_ = db.Close()

	n, err := copyDirTree(src, outDir)
	if err != nil {
		return 0, fmt.Errorf("wiki export: %w", err)
	}
	return n, nil
}

// ImportFromParquet places a published index into the wiki at wikiDir.
//
// It reports how many chunks landed, for the same reason BuildDBFromCache does — zero is a real
// answer rather than an error, and a caller that read it as success would leave a healthy, empty
// index behind.
func ImportFromParquet(ctx context.Context, wikiDir, inDir string) (int, error) {
	info, err := os.Stat(inDir)
	if err != nil || !info.IsDir() {
		return 0, fmt.Errorf("wiki import: no index at %s", inDir)
	}
	if _, err := copyDirTree(inDir, WikiIndexPath(wikiDir)); err != nil {
		return 0, fmt.Errorf("wiki import: %w", err)
	}

	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return 0, fmt.Errorf("wiki import: the copied index does not open: %w", err)
	}
	defer func() { _ = db.Close() }()

	chunks, _, _, _, err := db.Stats(ctx)
	if err != nil {
		return 0, fmt.Errorf("wiki import: count: %w", err)
	}
	return chunks, nil
}

// copyDirTree copies a directory recursively, returning the bytes written.
//
// Correct for a Lance directory in a way it would not be for a live database file: the data files
// are immutable and the manifest names which of them a version consists of, so a copy taken while
// nothing is writing is a valid dataset. Both call sites hold the store closed for that reason.
func copyDirTree(srcDir, dstDir string) (int64, error) {
	var written int64
	err := filepath.Walk(srcDir, func(path string, fi os.FileInfo, err error) error {
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
