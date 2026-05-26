package ast

import (
	"os"
	"path/filepath"
	"strings"
)

func DetectProjectLang(projectPath string) string {
	counts := DetectProjectLangs(projectPath)
	best := ""
	bestCount := 0
	for lang, count := range counts {
		if count > bestCount {
			best = lang
			bestCount = count
		}
	}
	return best
}

func DetectProjectLangs(projectPath string) map[string]int {
	counts := make(map[string]int)

	ic := NewAstIgnoreChecker(projectPath)

	_ = filepath.Walk(projectPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}

		rel, relErr := filepath.Rel(projectPath, path)
		if relErr != nil {
			return nil
		}

		if info.IsDir() {
			if strings.HasPrefix(info.Name(), ".") {
				return filepath.SkipDir
			}
			if rel != "." && ic.IsIgnored(rel, true) {
				return filepath.SkipDir
			}
			return nil
		}

		if rel != "." && ic.IsIgnored(rel, false) {
			return nil
		}

		ext := strings.ToLower(filepath.Ext(path))
		lang := langForExtension(ext)
		if lang != "" {
			counts[lang]++
		}
		return nil
	})

	return counts
}

func langForExtension(ext string) string {
	return TreeSitterLangForExtension(ext)
}
