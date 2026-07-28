package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type GraphWriter struct {
	db          GraphDB
	rootPath    string
	cluster     string
	indexSource bool
}

func NewGraphWriter(db GraphDB, rootPath string, indexSource bool) *GraphWriter {
	abs, _ := filepath.Abs(rootPath)
	return &GraphWriter{db: db, rootPath: abs, indexSource: indexSource}
}

func (w *GraphWriter) rel(path string) string {
	abs, err := filepath.Abs(path)
	if err != nil {
		return path
	}
	rel, err := filepath.Rel(w.rootPath, abs)
	if err != nil {
		return abs
	}

	if strings.HasPrefix(rel, "..") {
		return ""
	}
	return rel
}

// DeleteRepository removes every node the graph holds for the files under
// repoPath, so a forced reindex rebuilds from a clean slate. It used to be a stub
// returning nil, which is why entities of deleted or renamed files survived every
// reindex and only a full reset could clear them.
//
// Scoping is by File.path, stored relative to the indexed root: a repoPath equal
// to that root covers the whole graph, a subdirectory covers its own prefix, and
// a path outside the root owns nothing here and deletes nothing.
func (w *GraphWriter) DeleteRepository(ctx context.Context, repoPath string) error {
	prefix := w.rel(repoPath)
	if prefix == "" {
		return nil
	}

	labels := ActiveNodeLabels(ctx, w.db)
	active := make(map[string]bool, len(labels))
	for _, l := range labels {
		active[l] = true
	}

	files, err := w.pathsUnder(ctx, LabelFile, prefix)
	if err != nil {
		return err
	}

	pathScoped := make(map[string]bool, len(labels))
	if len(files) > 0 {
		params := map[string]any{"paths": files}

		for _, label := range labels {
			if label == LabelFile || label == LabelDirectory || !w.labelHasPath(ctx, label) {
				continue
			}
			pathScoped[label] = true
			q := fmt.Sprintf("MATCH (n:`%s`) WHERE n.path IN $paths DETACH DELETE n", label)
			if _, err := w.db.Execute(ctx, q, params); err != nil {
				return fmt.Errorf("delete %s nodes: %w", label, err)
			}
		}

		if _, err := w.db.Execute(ctx,
			`UNWIND $paths AS p MATCH (f:File {path: p}) DETACH DELETE f`, params); err != nil {
			return fmt.Errorf("delete file nodes: %w", err)
		}
	}

	// Directories are scaffolding rather than file content, so they never appear
	// in the file list — but a tree that is gone should not leave its folders
	// standing either.
	if active[LabelDirectory] {
		dirs, err := w.pathsUnder(ctx, LabelDirectory, prefix)
		if err != nil {
			return err
		}
		if len(dirs) > 0 {
			if _, err := w.db.Execute(ctx,
				`UNWIND $paths AS p MATCH (d:Directory {path: p}) DETACH DELETE d`,
				map[string]any{"paths": dirs}); err != nil {
				return fmt.Errorf("delete directory nodes: %w", err)
			}
		}
	}

	// Parameter and Field get a minimal schema with no path column when the
	// grammar does not declare them as labels, so they cannot be matched by file.
	// They are only ever reachable from their owner, which the deletes above have
	// just removed — so whatever is left unowned belonged to a file that is gone.
	// Scoping still holds for a partial delete: a parameter whose owner survives
	// keeps its incoming edge.
	for _, label := range labels {
		if pathScoped[label] || label == LabelFile || label == LabelDirectory {
			continue
		}
		q := fmt.Sprintf(
			"MATCH (n:`%s`) OPTIONAL MATCH ()-[r]->(n) WITH n, count(r) AS owners WHERE owners = 0 DELETE n", label)
		if _, err := w.db.Execute(ctx, q, nil); err != nil {
			return fmt.Errorf("delete orphaned %s nodes: %w", label, err)
		}
	}

	return nil
}

// pathsUnder lists the paths of label's nodes that live under prefix, which is
// the repository path already made relative to the indexed root — "." for the
// root itself.
func (w *GraphWriter) pathsUnder(ctx context.Context, label, prefix string) ([]string, error) {
	res, err := w.db.Query(ctx, fmt.Sprintf("MATCH (n:`%s`) RETURN n.path AS path", label), nil)
	if err != nil {
		return nil, fmt.Errorf("list indexed %s nodes: %w", label, err)
	}

	var paths []string
	for _, rec := range res.Records {
		p, ok := rec["path"].(string)
		if !ok || p == "" {
			continue
		}
		if prefix == "." || p == prefix || strings.HasPrefix(p, prefix+string(filepath.Separator)) {
			paths = append(paths, p)
		}
	}
	return paths, nil
}

// labelHasPath reports whether a node table carries the owning file's path.
func (w *GraphWriter) labelHasPath(ctx context.Context, label string) bool {
	res, err := w.db.Query(ctx, fmt.Sprintf("CALL table_info('%s') RETURN *", label), nil)
	if err != nil {
		return false
	}
	for _, rec := range res.Records {
		if name, ok := rec["name"].(string); ok && name == "path" {
			return true
		}
	}
	return false
}

func collectFiles(rootPath string) ([]string, error) {
	var files []string

	ic := NewAstIgnoreChecker(rootPath)

	err := filepath.Walk(rootPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}

		rel, relErr := filepath.Rel(rootPath, path)
		if relErr != nil {
			return nil
		}

		if rel != "." {
			if info.IsDir() {

				if strings.HasPrefix(info.Name(), ".") {
					return filepath.SkipDir
				}
				if ic.IsIgnored(rel, true) && !ic.ShouldDescend(rel) {
					return filepath.SkipDir
				}
				return nil
			}

			if ic.IsIgnored(rel, false) {
				return nil
			}
		}

		if info.IsDir() {
			return nil
		}

		ext := strings.ToLower(filepath.Ext(path))
		if HasParserForExtensionIn(rootPath, ext) {
			files = append(files, path)
		}
		return nil
	})
	return files, err
}

func contextTypeToLabel(contextType string) string {
	if contextType == "" {
		return ""
	}
	knownLabels := map[string]string{

		"package":   "Package",
		"procedure": "Procedure",
		"function":  "Function",
		"table":     "Table",
		"view":      "View",
		"type":      "Type",
		"sequence":  "Sequence",
		"synonym":   "Synonym",
		"index":     "Index",
		"class":     "Class",
		"struct":    "Struct",
		"interface": "Interface",
		"namespace": "Namespace",
		"enum":      "Enum",
		"trigger":   "Trigger",
	}
	if label, ok := knownLabels[strings.ToLower(contextType)]; ok {
		return label
	}

	if len(contextType) > 0 {
		return strings.ToUpper(contextType[:1]) + contextType[1:]
	}
	return contextType
}

func entityUID(relFilePath, name, ctxName string) string {
	if ctxName != "" {
		return relFilePath + "::" + ctxName + "." + name
	}
	return relFilePath + "::" + name
}

func canonicalModuleName(importPath string, importingFileRelPath string) string {

	name := strings.Trim(importPath, "'\"")

	if strings.HasPrefix(name, "./") || strings.HasPrefix(name, "../") {
		dir := filepath.Dir(importingFileRelPath)
		resolved := filepath.Join(dir, name)

		resolved = filepath.Clean(resolved)

		resolved = filepath.ToSlash(resolved)

		if !strings.HasPrefix(resolved, "..") {
			name = resolved
		}
	}

	for _, ext := range []string{".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".mts"} {
		if strings.HasSuffix(name, ext) {
			name = strings.TrimSuffix(name, ext)
			break
		}
	}

	name = filepath.ToSlash(name)

	return name
}
