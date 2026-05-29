package ast

import (
	"log/slog"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func PreScanForImports(files []string, logger *slog.Logger) map[string][]string {
	log := slogutil.Resolve(logger)
	importsMap := make(map[string][]string)

	byExt := make(map[string][]string)
	for _, f := range files {
		ext := strings.ToLower(filepath.Ext(f))
		byExt[ext] = append(byExt[ext], f)
	}

	tsParser := &TreeSitterParser{}
	for ext, paths := range byExt {
		if !HasTreeSitterForExtension(ext) {
			continue
		}

		langMap := genericPreScan(tsParser, paths)
		mergeImportsMap(importsMap, langMap)
		if len(langMap) > 0 {
			log.Debug("pre-scan extension", "ext", ext, "definitions", len(langMap), "files", len(paths))
		}
	}

	log.Debug("pre-scan complete", "total_definitions", len(importsMap))
	return importsMap
}

func genericPreScan(parser LanguageParser, files []string) map[string][]string {
	result := make(map[string][]string)
	for _, path := range files {
		pf, err := parser.Parse(path, false, ParseOptions{})
		if err != nil {
			continue
		}
		for _, e := range pf.AllEntities() {
			if e.Name != "" {
				result[e.Name] = appendUniqStr(result[e.Name], path)
			}
		}
	}
	return result
}

func mergeImportsMap(dst, src map[string][]string) {
	for name, paths := range src {
		for _, p := range paths {
			dst[name] = appendUniqStr(dst[name], p)
		}
	}
}

func appendUniqStr(sl []string, v string) []string {
	for _, s := range sl {
		if s == v {
			return sl
		}
	}
	return append(sl, v)
}
