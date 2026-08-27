package memory

import (
	"os"
	"path/filepath"
	"strings"
)

const ImportantMemorySuffix = "_important_"

func IsImportantMemory(filename string) bool {
	base := filepath.Base(filename)
	return strings.HasSuffix(base, ImportantMemorySuffix+".md")
}

func ImportantFileName(id string) string {
	return id + ImportantMemorySuffix + ".md"
}

func NormalFileName(id string) string {
	return id + ".md"
}

type ImportantEntry struct {
	ID      string
	Title   string
	Content string
	Path    string
	created string
}

func ListImportantMemories(scope string) ([]ImportantEntry, error) {
	dir := RawDir(scope)
	return listImportantInDir(dir)
}

func listImportantInDir(dir string) ([]ImportantEntry, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var important []ImportantEntry
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		if !IsImportantMemory(name) {
			continue
		}

		id := strings.TrimSuffix(name, ImportantMemorySuffix+".md")

		absPath := filepath.Join(dir, name)
		data, err := os.ReadFile(absPath)
		if err != nil {
			continue
		}

		title, _ := parseMemoryMeta(absPath)
		content := extractBodyAfterFrontmatter(string(data))

		important = append(important, ImportantEntry{
			ID:      id,
			Title:   title,
			Content: strings.TrimSpace(content),
			Path:    absPath,
		})
	}
	return important, nil
}

func extractBodyAfterFrontmatter(content string) string {
	lines := strings.Split(content, "\n")
	inFrontmatter := false
	passedFrontmatter := false
	passedH1 := false
	var body []string

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

		if i == 0 && trimmed == "---" {
			inFrontmatter = true
			continue
		}
		if inFrontmatter {
			if trimmed == "---" {
				inFrontmatter = false
				passedFrontmatter = true
			}
			continue
		}

		if passedFrontmatter && !passedH1 {
			if trimmed == "" {
				continue
			}
			if strings.HasPrefix(trimmed, "# ") {
				passedH1 = true
				continue
			}
		}

		body = append(body, line)
	}

	return strings.TrimSpace(strings.Join(body, "\n"))
}
