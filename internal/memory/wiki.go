package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type WikiResult struct {
	ArticlesWritten int
}

func GenerateMemoryWiki(_ context.Context, rawDir, wikiDir string) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	entries, err := os.ReadDir(rawDir)
	if err != nil {
		if os.IsNotExist(err) {
			return &WikiResult{}, nil
		}
		return nil, fmt.Errorf("reading memory dir: %w", err)
	}

	var docs []memDoc
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()

		if name == "index.md" || name == "log.md" || strings.HasPrefix(name, "Memory_Wiki") {
			continue
		}
		absPath := filepath.Join(rawDir, name)

		important := IsImportantMemory(name)
		var id string
		if important {
			id = strings.TrimSuffix(name, ImportantMemorySuffix+".md")
		} else {
			id = strings.TrimSuffix(name, ".md")
		}

		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}

		title, createdAt := parseMemoryMeta(absPath)
		body := extractBodyAfterFrontmatter(string(data))
		memType := parseMemoryType(string(data))

		docs = append(docs, memDoc{
			id:        id,
			title:     title,
			createdAt: createdAt,
			important: important,
			body:      body,
			filename:  name,
			memType:   memType,
		})
	}

	sort.Slice(docs, func(i, j int) bool {
		if docs[i].important != docs[j].important {
			return docs[i].important
		}
		return docs[i].createdAt > docs[j].createdAt
	})

	result := &WikiResult{}
	usedSlugs := make(map[string]bool)

	for _, doc := range docs {
		slug := uniqueMemSlug(safeMemFilename(doc.title), usedSlugs)
		page := memoryEntityPage(doc.id, doc.title, doc.createdAt, doc.important, doc.body, doc.memType)
		path := filepath.Join(wikiDir, slug+".md")
		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	indexContent := memoryIndexPage(docs)
	if err := os.WriteFile(filepath.Join(wikiDir, "index.md"), []byte(indexContent), 0o644); err != nil {
		return result, err
	}

	appendMemLog(filepath.Join(wikiDir, "log.md"), len(docs), result.ArticlesWritten)

	return result, nil
}

func memoryEntityPage(id, title, createdAt string, important bool, body, memType string) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	b.WriteString("---\n")
	b.WriteString(fmt.Sprintf("title: %s\n", title))
	b.WriteString(fmt.Sprintf("id: %s\n", id))
	b.WriteString(fmt.Sprintf("updated: %s\n", now))
	if createdAt != "" {
		b.WriteString(fmt.Sprintf("created: %s\n", createdAt))
	}
	if memType != "" {
		b.WriteString(fmt.Sprintf("type: %s\n", memType))
	}
	tags := "memory"
	if important {
		tags += ", important"
	}
	if memType != "" {
		tags += ", " + memType
	}
	b.WriteString(fmt.Sprintf("tags: [%s]\n", tags))
	b.WriteString("---\n\n")
	b.WriteString(fmt.Sprintf("# %s\n\n", title))

	if memType != "" {
		typeEmojis := map[string]string{
			"convention": "🏗️", "correction": "🔧", "decision": "📐",
			"tension": "⚡", "fact": "📋", "skill": "🛠️",
		}
		emoji := typeEmojis[memType]
		if emoji == "" {
			emoji = "📄"
		}
		b.WriteString(fmt.Sprintf("> %s **Type:** %s\n\n", emoji, memType))
	}

	if important {
		b.WriteString("> ⭐ **Important memory** — surfaced in IDE rules.\n\n")
	}

	if createdAt != "" {
		if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
			age := time.Since(t)
			if age > 30*24*time.Hour {
				days := int(age.Hours() / 24)
				b.WriteString(fmt.Sprintf("> ⚠️ **Stale memory** — created %d days ago. Consider refreshing with `memory update %s`.\n\n", days, id))
			}
		}
	}

	if body != "" {
		b.WriteString(body + "\n\n")
	}
	b.WriteString("---\n*Navigate: [[index]] · [[log]]*\n")
	return b.String()
}

func memoryIndexPage(docs []memDoc) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")

	b.WriteString("---\n")
	b.WriteString("title: Memory Wiki\n")
	b.WriteString(fmt.Sprintf("updated: %s\n", now))
	b.WriteString("tags: [memory, index]\n")
	b.WriteString("---\n\n")
	b.WriteString("# Memory Wiki\n\n")
	b.WriteString(fmt.Sprintf("> %s memory wiki. **Start here.** Scan the catalog below, then follow [[wikilinks]] to drill into specific pages.\n", brand.DisplayName))
	b.WriteString(fmt.Sprintf("> Check [[log]] for the timeline of updates. Last updated: %s\n\n", now))

	importantCount := 0
	for _, d := range docs {
		if d.important {
			importantCount++
		}
	}
	b.WriteString(fmt.Sprintf("**%d memories · %d important**\n\n", len(docs), importantCount))
	b.WriteString("---\n\n")

	if importantCount > 0 {
		b.WriteString("## ⭐ Important Memories\n\n")
		b.WriteString("> Critical decisions, conventions, and constraints.\n\n")
		for _, d := range docs {
			if !d.important {
				continue
			}
			link := fmt.Sprintf("[[%s]]", safeMemFilename(d.title))
			summary := firstLine(d.body)
			if summary != "" {
				b.WriteString(fmt.Sprintf("- %s — %s\n", link, summary))
			} else {
				b.WriteString(fmt.Sprintf("- %s\n", link))
			}
		}
		b.WriteString("\n")
	}

	typeOrder := []struct {
		key   string
		emoji string
		label string
	}{
		{"convention", "🏗️", "Conventions"},
		{"correction", "🔧", "Corrections"},
		{"decision", "📐", "Decisions"},
		{"tension", "⚡", "Tensions"},
		{"skill", "🛠️", "Skills"},
		{"fact", "📋", "Facts"},
	}

	for _, tp := range typeOrder {
		var typed []memDoc
		for _, d := range docs {
			if d.memType == tp.key {
				typed = append(typed, d)
			}
		}
		if len(typed) == 0 {
			continue
		}
		b.WriteString(fmt.Sprintf("## %s %s\n\n", tp.emoji, tp.label))
		for _, d := range typed {
			link := fmt.Sprintf("[[%s]]", safeMemFilename(d.title))
			prefix := ""
			if d.important {
				prefix = "⭐ "
			}
			summary := firstLine(d.body)
			if summary != "" {
				b.WriteString(fmt.Sprintf("- %s%s — %s\n", prefix, link, summary))
			} else {
				b.WriteString(fmt.Sprintf("- %s%s\n", prefix, link))
			}
		}
		b.WriteString("\n")
	}

	var untyped []memDoc
	for _, d := range docs {
		if d.memType == "" {
			untyped = append(untyped, d)
		}
	}
	if len(untyped) > 0 {
		b.WriteString("## 📝 Other Memories\n\n")
		for _, d := range untyped {
			link := fmt.Sprintf("[[%s]]", safeMemFilename(d.title))
			prefix := ""
			if d.important {
				prefix = "⭐ "
			}
			summary := firstLine(d.body)
			if summary != "" {
				b.WriteString(fmt.Sprintf("- %s%s — %s\n", prefix, link, summary))
			} else {
				b.WriteString(fmt.Sprintf("- %s%s\n", prefix, link))
			}
		}
		b.WriteString("\n")
	}

	b.WriteString("## How to Use This Wiki\n\n")
	b.WriteString("1. **Scan by Type** — find conventions, corrections, decisions, etc.\n")
	b.WriteString("2. **Check Important** — ⭐ items are critical project decisions surfaced in IDE rules.\n")
	b.WriteString("3. **Read the Log** — [[log]] shows the timeline of wiki updates.\n\n")
	b.WriteString("---\n")
	b.WriteString(fmt.Sprintf("*Generated by %s · %s · [[log]]*\n", brand.DisplayName, now))
	return b.String()
}

func appendMemLog(logPath string, totalMemories, articlesWritten int) {
	now := time.Now().UTC().Format("2006-01-02")
	entry := fmt.Sprintf("## [%s] index | Wiki regenerated\n\n"+
		"- Memories: %d\n- Articles written: %d\n\n",
		now, totalMemories, articlesWritten)

	existing, _ := os.ReadFile(logPath)
	var content string
	if len(existing) == 0 {
		content = "---\ntitle: Memory Wiki Log\ntags: [memory, log]\n---\n\n# Memory Wiki Log\n\n" +
			"> Append-only chronological record. Parse with: `grep '^## \\[' log.md | tail -5`\n\n"
	} else {
		content = string(existing)
	}

	parts := strings.SplitN(content, "\n---\n", 2)
	if len(parts) == 2 {
		content = parts[0] + "\n---\n\n" + entry + parts[1]
	} else {
		content += "\n" + entry
	}
	if err := os.WriteFile(logPath, []byte(content), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] wiki log: write %s failed: %v\n", logPath, err)
	}
}

func safeMemFilename(name string) string {
	r := strings.NewReplacer("/", "-", " ", "_", ":", "-", "\\", "-", "?", "", "*", "")
	return r.Replace(name)
}

func uniqueMemSlug(base string, used map[string]bool) string {
	slug := base
	n := 2
	for used[slug] {
		slug = fmt.Sprintf("%s_%d", base, n)
		n++
	}
	used[slug] = true
	return slug
}

func firstLine(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed != "" && !strings.HasPrefix(trimmed, "#") {
			if len(trimmed) > 120 {
				return trimmed[:120] + "…"
			}
			return trimmed
		}
	}
	return ""
}

type memDoc struct {
	id        string
	title     string
	createdAt string
	important bool
	body      string
	filename  string
	memType   string
}

var reMemoryType = regexp.MustCompile(`(?m)^type:\s*(.+)$`)

func parseMemoryType(content string) string {
	if m := reMemoryType.FindStringSubmatch(content); m != nil {
		return strings.TrimSpace(m[1])
	}
	return ""
}
