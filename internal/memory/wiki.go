package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type WikiResult struct {
	ArticlesWritten int
}

func GenerateMemoryWiki(_ context.Context, rawDir, wikiDir string, logger ...*slog.Logger) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	// Load process cache for incremental builds.
	processCache, _ := wiki.NewWikiProcessCache(wikiDir)
	validPaths := make(map[string]bool)

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

		contentHash := wiki.ContentHash(data)
		validPaths[name] = true

		// Always parse metadata (cheap).
		title, createdAt := parseMemoryMeta(absPath)

		// Try cache first — skip re-processing unchanged files.
		if processCache != nil && !processCache.HasChanged(name, contentHash) {
			if cached := processCache.Get(name, contentHash); len(cached) > 0 {
				cc := cached[0]
				docs = append(docs, memDoc{
					id:          id,
					title:       cc.Title,
					createdAt:   createdAt,
					important:   important,
					body:        cc.Body,
					filename:    name,
					memType:     cc.DocType,
					contentHash: cc.ContentHash,
				})
				continue
			}
		}

		// Cache miss — process from source.
		body := extractBodyAfterFrontmatter(string(data))
		memType := parseMemoryType(string(data))

		doc := memDoc{
			id:          id,
			title:       title,
			createdAt:   createdAt,
			important:   important,
			body:        body,
			filename:    name,
			memType:     memType,
			contentHash: contentHash,
		}
		docs = append(docs, doc)

		// Store in cache.
		if processCache != nil {
			processCache.Store(name, contentHash, []wiki.CachedChunk{{
				Title:       title,
				Body:        body,
				DocType:     memType,
				ContentHash: contentHash,
			}})
		}
	}

	// Prune deleted files from cache.
	if processCache != nil {
		processCache.Prune(validPaths)
	}

	sort.Slice(docs, func(i, j int) bool {
		if docs[i].important != docs[j].important {
			return docs[i].important
		}
		return docs[i].createdAt > docs[j].createdAt
	})

	result := &WikiResult{}
	usedSlugs := make(map[string]bool)

	// FAST PATH: use wiki.FastPathCheck — checks processCache (O(1) per entry,
	// no disk I/O) and a single ReadDir to detect deletions.
	fastSlugs := make(map[string]bool, len(docs))
	fastEntries := make([]wiki.DocHashEntry, 0, len(docs))
	for _, doc := range docs {
		slug := wiki.UniqueSlug(wiki.SafeSlug(doc.title), fastSlugs)
		fastEntries = append(fastEntries, wiki.DocHashEntry{
			CacheKey:    doc.filename,
			ContentHash: doc.contentHash,
			Slug:        slug,
		})
	}
	if wiki.FastPathCheck(wikiDir, fastEntries, processCache) {
		_ = processCache.Save()
		return result, nil
	}

	for _, doc := range docs {
		slug := wiki.UniqueSlug(wiki.SafeSlug(doc.title), usedSlugs)
		path := filepath.Join(wikiDir, slug+".md")
		// Only write if content hash changed — avoids I/O when wiki is up to date.
		// We read the existing file's content_hash from frontmatter.
		if doc.contentHash != "" {
			if existingHash := wiki.ReadFrontmatterField(path, "content_hash"); existingHash == doc.contentHash {
				continue
			}
		}
		page := memoryEntityPageWithHash(doc.id, doc.title, doc.createdAt, doc.important, doc.body, doc.memType, doc.contentHash)
		if err := os.WriteFile(path, []byte(page), 0o644); err != nil {
			continue
		}
		result.ArticlesWritten++
	}

	// Remove stale wiki pages that no longer correspond to any memory in the raw dir.
	keepFiles := map[string]bool{}
	for slug := range usedSlugs {
		keepFiles[slug+".md"] = true
	}
	if existing, readErr := os.ReadDir(wikiDir); readErr == nil {
		for _, e := range existing {
			if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
				continue
			}
			if !keepFiles[e.Name()] {
				_ = os.Remove(filepath.Join(wikiDir, e.Name()))
			}
		}
	}

	// Build WikiDB for FTS5 search
	wikiChunks := make([]wiki.WikiChunk, 0, len(docs))
	for _, doc := range docs {
		slug := wiki.SafeSlug(doc.title)
		now := time.Now().UTC().Format("2006-01-02")
		wc := len(strings.Fields(doc.body))

		wikiChunks = append(wikiChunks, wiki.WikiChunk{
			Slug:        slug,
			Title:       doc.title,
			Body:        doc.body,
			Summary:     wiki.ExtractSummary(doc.body),
			DocType:     doc.memType,
			Source:      doc.filename,
			WordCount:   wc,
			Updated:     now,
			Important:   doc.important,
			ContentHash: doc.contentHash,
		})
	}

	// Pass a logEntry only when articles were actually written so that
	// pipeline.RebuildDB can use CheckAllHashesMatch to skip the expensive
	// SQLite rebuild when the DB is already in sync.
	var logEntry *wiki.SyncLogEntry
	if result.ArticlesWritten > 0 {
		logEntry = &wiki.SyncLogEntry{
			Timestamp:       time.Now().UTC().Format(time.RFC3339),
			TotalDocs:       len(docs),
			ArticlesWritten: result.ArticlesWritten,
		}
	}

	_ = wiki.RebuildDB(wikiDir, wikiChunks, nil, logEntry, processCache)

	return result, nil
}

func memoryEntityPageWithHash(id, title, createdAt string, important bool, body, memType, contentHash string) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "title: %s\n", title)
	_, _ = fmt.Fprintf(&b, "id: %s\n", id)
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	if contentHash != "" {
		_, _ = fmt.Fprintf(&b, "content_hash: %s\n", contentHash)
	}
	if createdAt != "" {
		_, _ = fmt.Fprintf(&b, "created: %s\n", createdAt)
	}
	if memType != "" {
		_, _ = fmt.Fprintf(&b, "type: %s\n", memType)
	}
	tags := "memory"
	if important {
		tags += ", important"
	}
	if memType != "" {
		tags += ", " + memType
	}
	_, _ = fmt.Fprintf(&b, "tags: [%s]\n", tags)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", title)

	if memType != "" {
		typeEmojis := map[string]string{
			"convention": "🏗️", "correction": "🔧", "decision": "📐",
			"tension": "⚡", "fact": "📋", "skill": "🛠️",
		}
		emoji := typeEmojis[memType]
		if emoji == "" {
			emoji = "📄"
		}
		_, _ = fmt.Fprintf(&b, "> %s **Type:** %s\n\n", emoji, memType)
	}

	if important {
		b.WriteString("> ⭐ **Important memory** — surfaced in IDE rules.\n\n")
	}

	if createdAt != "" {
		if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
			age := time.Since(t)
			if age > 30*24*time.Hour {
				days := int(age.Hours() / 24)
				_, _ = fmt.Fprintf(&b, "> ⚠️ **Stale memory** — created %d days ago. Consider refreshing with `memory update %s`.\n\n", days, id)
			}
		}
	}

	if body != "" {
		b.WriteString(body + "\n\n")
	}
	b.WriteString("---\n")
	return b.String()
}

func memoryIndexPage(docs []memDoc) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")

	b.WriteString("---\n")
	b.WriteString("title: Memory Wiki\n")
	_, _ = fmt.Fprintf(&b, "updated: %s\n", now)
	b.WriteString("tags: [memory, index]\n")
	b.WriteString("---\n\n")
	b.WriteString("# Memory Wiki\n\n")
	_, _ = fmt.Fprintf(&b, "> %s memory wiki. **Start here.** Scan the catalog below, then follow [[wikilinks]] to drill into specific pages.\n", brand.DisplayName)
	_, _ = fmt.Fprintf(&b, "> Check [[log]] for the timeline of updates. Last updated: %s\n\n", now)

	importantCount := 0
	for _, d := range docs {
		if d.important {
			importantCount++
		}
	}
	_, _ = fmt.Fprintf(&b, "**%d memories · %d important**\n\n", len(docs), importantCount)
	b.WriteString("---\n\n")

	if importantCount > 0 {
		b.WriteString("## ⭐ Important Memories\n\n")
		b.WriteString("> Critical decisions, conventions, and constraints.\n\n")
		for _, d := range docs {
			if !d.important {
				continue
			}
			link := fmt.Sprintf("[[%s]]", wiki.SafeSlug(d.title))
			summary := firstLine(d.body)
			if summary != "" {
				_, _ = fmt.Fprintf(&b, "- %s — %s\n", link, summary)
			} else {
				_, _ = fmt.Fprintf(&b, "- %s\n", link)
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
		_, _ = fmt.Fprintf(&b, "## %s %s\n\n", tp.emoji, tp.label)
		for _, d := range typed {
			link := fmt.Sprintf("[[%s]]", wiki.SafeSlug(d.title))
			prefix := ""
			if d.important {
				prefix = "⭐ "
			}
			summary := firstLine(d.body)
			if summary != "" {
				_, _ = fmt.Fprintf(&b, "- %s%s — %s\n", prefix, link, summary)
			} else {
				_, _ = fmt.Fprintf(&b, "- %s%s\n", prefix, link)
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
			link := fmt.Sprintf("[[%s]]", wiki.SafeSlug(d.title))
			prefix := ""
			if d.important {
				prefix = "⭐ "
			}
			summary := firstLine(d.body)
			if summary != "" {
				_, _ = fmt.Fprintf(&b, "- %s%s — %s\n", prefix, link, summary)
			} else {
				_, _ = fmt.Fprintf(&b, "- %s%s\n", prefix, link)
			}
		}
		b.WriteString("\n")
	}

	b.WriteString("## How to Use This Wiki\n\n")
	b.WriteString("1. **Scan by Type** — find conventions, corrections, decisions, etc.\n")
	b.WriteString("2. **Check Important** — ⭐ items are critical project decisions surfaced in IDE rules.\n")
	b.WriteString("3. **Read the Log** — [[log]] shows the timeline of wiki updates.\n\n")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "*Generated by %s · %s · [[log]]*\n", brand.DisplayName, now)
	return b.String()
}

func appendMemLog(logPath string, totalMemories, articlesWritten int, logger *slog.Logger) {
	log := slogutil.Resolve(logger)
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
		log.Warn("wiki log: write failed", "path", logPath, "error", err)
	}
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
	id          string
	title       string
	createdAt   string
	important   bool
	body        string
	filename    string
	memType     string
	contentHash string
}

var reMemoryType = regexp.MustCompile(`(?m)^type:\s*(.+)$`)

func parseMemoryType(content string) string {
	if m := reMemoryType.FindStringSubmatch(content); m != nil {
		return strings.TrimSpace(m[1])
	}
	return ""
}
