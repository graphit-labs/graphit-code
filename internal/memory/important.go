package memory

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
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

func RenderImportantBlock(scope string) string {
	entries, err := ListImportantMemories(scope)
	if err != nil || len(entries) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("## 📌 Key Project Memories\n\n")
	b.WriteString("> These are the most critical project decisions and conventions.\n")
	b.WriteString("> They are automatically maintained. Do not edit this section.\n\n")

	for _, e := range entries {
		_, _ = fmt.Fprintf(&b, "### %s\n", e.Title)
		_, _ = fmt.Fprintf(&b, "*ID: `%s`*\n\n", e.ID)
		if e.Content != "" {
			b.WriteString(e.Content + "\n")
		}
		b.WriteString("\n")
	}

	return b.String()
}

func ListRecentMemories(scope string, limit int) ([]ImportantEntry, error) {
	dir := RawDir(scope)
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var all []ImportantEntry
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()

		if IsImportantMemory(name) {
			continue
		}

		id := strings.TrimSuffix(name, ".md")
		absPath := filepath.Join(dir, name)
		data, err := os.ReadFile(absPath)
		if err != nil {
			continue
		}

		title, createdAt := parseMemoryMeta(absPath)
		content := extractBodyAfterFrontmatter(string(data))

		all = append(all, ImportantEntry{
			ID:      id,
			Title:   title,
			Content: strings.TrimSpace(content),
			Path:    absPath,
			created: createdAt,
		})
	}

	sort.Slice(all, func(i, j int) bool {
		return all[i].created > all[j].created
	})

	if len(all) > limit {
		all = all[:limit]
	}
	return all, nil
}

func RenderRecentBlock(scope string, limit int) string {
	entries, err := ListRecentMemories(scope, limit)
	if err != nil || len(entries) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("## 🕐 Recent Memories\n\n")
	b.WriteString("> Latest agent-learned facts. Check these for immediate context.\n\n")

	for _, e := range entries {
		summary := firstLineFromContent(e.Content)
		if summary != "" {
			_, _ = fmt.Fprintf(&b, "- **%s** — %s *(ID: `%s`)*\n", e.Title, summary, e.ID)
		} else {
			_, _ = fmt.Fprintf(&b, "- **%s** *(ID: `%s`)*\n", e.Title, e.ID)
		}
	}
	b.WriteString("\n")

	return b.String()
}

func firstLineFromContent(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed != "" && !strings.HasPrefix(trimmed, "#") {
			if len(trimmed) > 100 {
				return trimmed[:100] + "…"
			}
			return trimmed
		}
	}
	return ""
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
