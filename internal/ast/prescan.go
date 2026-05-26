package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func PreScanForImports(files []string) map[string][]string {
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
			fmt.Fprintf(os.Stderr, "[PRE-SCAN] %s: %d definitions from %d files\n", ext, len(langMap), len(paths))
		}
	}

	fmt.Fprintf(os.Stderr, "[PRE-SCAN] Total: %d cross-file definitions\n", len(importsMap))
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
