package ast

import (
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func exportGraphToIcebug(storeDir, outDir, searchDir, storageURI string, logger *slog.Logger) (*ladybug.CanonicalManifest, error) {
	log := slogutil.Resolve(logger)
	if strings.TrimSpace(storageURI) == "" {
		return nil, fmt.Errorf("icebug export: no storage URI — the artifact would mount against " +
			"the publisher's disk")
	}

	man, err := exportDirectFromRebuildIndexFromStore(storeDir, outDir, storageURI)
	if err != nil {
		return nil, fmt.Errorf("icebug export: %w", err)
	}

	searchBytes := int64(0)
	if searchDir != "" {
		n, cErr := copyLanceIndex(LanceIndexPath(storeDir), searchDir)
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
		"search_bytes", searchBytes, "storage", storageURI)
	return man, nil
}

func exportDirectFromRebuildIndexFromStore(storeDir, outDir, storageURI string) (*ladybug.CanonicalManifest, error) {
	src := filepath.Join(storeDir, IcebugBundleDir)
	rawMan, err := os.ReadFile(filepath.Join(storeDir, ladybug.IcebugManifestFile))
	if err != nil {
		return nil, fmt.Errorf("icebug export: read manifest: %w", err)
	}
	man, err := parseCanonicalManifest(rawMan)
	if err != nil {
		return nil, err
	}
	if err := copyDirContents(src, outDir); err != nil {
		return nil, fmt.Errorf("icebug export: copy bundle: %w", err)
	}
	if err := rewriteSchemaStorageURI(filepath.Join(outDir, IcebugSchemaFile), storageURI); err != nil {
		return nil, err
	}
	man.Storage = storageURI
	return man, nil
}

func parseCanonicalManifest(raw []byte) (*ladybug.CanonicalManifest, error) {
	var man ladybug.CanonicalManifest
	if err := json.Unmarshal(raw, &man); err != nil {
		return nil, fmt.Errorf("icebug: parse manifest: %w", err)
	}
	if man.Format != "icebug-canonical" || !man.Finished {
		return nil, fmt.Errorf("icebug: unsupported manifest format %q (only icebug-canonical v2+ supported)", man.Format)
	}
	return &man, nil
}

func copyDirContents(src, dst string) error {
	if err := os.MkdirAll(dst, 0o755); err != nil {
		return err
	}
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		if err := copyIcebugFile(filepath.Join(src, e.Name()), filepath.Join(dst, e.Name())); err != nil {
			return err
		}
	}
	return nil
}

func rewriteSchemaStorageURI(schemaPath, newURI string) error {
	raw, err := os.ReadFile(schemaPath)
	if err != nil {
		return err
	}
	s := string(raw)
	prefix := "storage = '"
	var b strings.Builder
	rest := s
	for {
		idx := strings.Index(rest, prefix)
		if idx < 0 {
			b.WriteString(rest)
			break
		}
		b.WriteString(rest[:idx+len(prefix)])
		rest = rest[idx+len(prefix):]
		end := strings.IndexByte(rest, '\'')
		if end < 0 {
			b.WriteString(rest)
			break
		}
		b.WriteString(newURI)
		rest = rest[end:]
	}
	return os.WriteFile(schemaPath, []byte(b.String()), 0o644)
}

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
