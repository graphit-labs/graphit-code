package ast

import (
	"context"
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

func (w *GraphWriter) DeleteRepository(ctx context.Context, repoPath string) error {

	return nil
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

				if strings.HasPrefix(info.Name(), ".") || ic.IsIgnored(rel, true) {
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
		if HasTreeSitterForExtension(ext) {
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
