package wiki

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
)

// StagePublishedIndex validates a local wiki and copies its Lance index into stagingRoot.
func StagePublishedIndex(ctx context.Context, wikiDir, stagingRoot string) (int64, error) {
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

	n, err := copyDirTree(src, WikiIndexPath(stagingRoot))
	if err != nil {
		return 0, fmt.Errorf("wiki export: %w", err)
	}
	return n, nil
}

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
