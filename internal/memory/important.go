package memory

import (
	"context"
	"path/filepath"
	"strings"
)

func MemoryFileName(id string) string {
	return id + ".md"
}

func MemoryIDFromFileName(name string) string {
	if name == "" {
		return ""
	}
	return strings.TrimSuffix(filepath.Base(name), ".md")
}

func IsImportantContent(content string) bool {
	return ParseMemoryFrontmatter(content).Important
}

type ImportantEntry struct {
	ID      string
	Title   string
	Content string
	Path    string
	created string
}

// ListImportantMemories is the scope's promoted memories, read from its store.
//
// `Path` is the path form a memory would have had — `<id>.md`. It is no longer a location, and it is
// kept because it is what identifies a memory to callers that display or link one; the store has no
// files to point at.
func ListImportantMemories(scope string) ([]ImportantEntry, error) {
	uri := TableURIForScope(scope)
	if uri == "" {
		return nil, nil
	}
	ctx := context.Background()
	tbl, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tbl.Close() }()

	records, err := tbl.Live(ctx)
	if err != nil {
		return nil, err
	}
	important := make([]ImportantEntry, 0)
	for _, rec := range records {
		if !rec.Important {
			continue
		}
		important = append(important, ImportantEntry{
			ID:      rec.ID,
			Title:   rec.Title,
			Content: strings.TrimSpace(rec.Body),
			Path:    MemoryFileName(rec.ID),
			created: rec.CreatedAt,
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
