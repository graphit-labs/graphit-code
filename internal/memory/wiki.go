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

// isMemorySourceFile reports whether a filename in the memory store is a
// memory page rather than a generated artifact of the wiki itself.
func isMemorySourceFile(name string) bool {
	if filepath.Ext(name) != ".md" {
		return false
	}
	return name != "index.md" && name != "log.md" && !strings.HasPrefix(name, "Memory_Wiki")
}

// memorySourceFileNames lists the cache keys of the memories currently on disk.
func memorySourceFileNames(rawDir string) []string {
	entries, err := os.ReadDir(rawDir)
	if err != nil {
		return nil
	}
	names := make([]string, 0, len(entries))
	for _, e := range entries {
		if e.IsDir() || !isMemorySourceFile(e.Name()) {
			continue
		}
		names = append(names, e.Name())
	}
	return names
}

func GenerateMemoryWiki(ctx context.Context, rawDir, wikiDir string, logger ...*slog.Logger) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	// The prefix carries shards beside the raw markdown, so a memory somebody else
	// wrote arrives with its embedding vector already computed. Importing before the
	// cache is opened is what lets this build find it instead of running the model
	// again; a shard whose hash does not match its local source is inert, so it cannot
	// mislead the build. Exported again at the end — see shardsync.go.
	//
	// Hooked here rather than in RunCycle because this is the real funnel: the memory
	// service compiles through IndexMemories, which does not go through a cycle.
	if _, err := ImportShards(rawDir, wikiDir); err != nil {
		slogutil.Resolve(firstLogger(logger)).Warn("memory shard import",
			"raw", rawDir, "wiki", wikiDir, "error", err)
	}

	// Load process cache for incremental builds.
	processCache, _ := wiki.NewWikiProcessCache(wikiDir)
	validPaths := make(map[string]bool)

	// STAT PRE-CHECK (shared AST pattern via wiki.StatPreCheck)
	// Memory files live in rawDir; use it as baseDir for relPath resolution.
	if wiki.StatPreCheck(rawDir, wikiDir, processCache, wiki.StatPreCheckOpts{
		CurrentSourceFiles: func() []string { return memorySourceFileNames(rawDir) },
	}) {
		return &WikiResult{}, nil
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
		if e.IsDir() || !isMemorySourceFile(e.Name()) {
			continue
		}
		name := e.Name()
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
			// Record mtime so next sync can skip via StatPreCheck Phase A.
			if info, statErr := os.Stat(absPath); statErr == nil {
				processCache.StoreMtime(name, info.ModTime().UnixNano(), info.Size())
			}
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

	// SAFETY: resolve each slug ONCE. Recomputing it per consumer let the page
	// and the DB chunk disagree whenever two memories share a title.
	slugs := make([]string, len(docs))
	usedSlugs := make(map[string]bool, len(docs))
	for i, doc := range docs {
		slugs[i] = wiki.UniqueSlug(wiki.SafeSlug(doc.title), usedSlugs)
	}

	// FAST PATH: use wiki.FastPathCheck — checks processCache (O(1) per entry,
	// no disk I/O) and a single ReadDir to detect deletions.
	fastEntries := make([]wiki.DocHashEntry, len(docs))
	for i, doc := range docs {
		fastEntries[i] = wiki.DocHashEntry{
			CacheKey:    doc.filename,
			ContentHash: doc.contentHash,
			Slug:        slugs[i],
		}
	}
	if wiki.FastPathCheck(ctx, wikiDir, fastEntries, processCache) {
		_ = processCache.Save()
		return result, nil
	}

	for i, doc := range docs {
		slug := slugs[i]
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
	for i, doc := range docs {
		now := time.Now().UTC().Format("2006-01-02")
		wc := len(strings.Fields(doc.body))

		wikiChunks = append(wikiChunks, wiki.WikiChunk{
			Slug:        slugs[i],
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

	// The error is REPORTED — and reported somewhere a person will actually see it.
	//
	// It used to be `_ =`, and that turned every failure of the index build into a silent
	// success: pages and shards were written, "Memory index complete" was printed, and the
	// store stayed empty. Search still answered — it falls back to a BM25 scan over the
	// .md files — so nothing looked broken while every query re-read the whole directory.
	//
	// Replacing `_ =` with a log line was only half of it, because the line went to
	// `slogutil.Resolve(nil)` — which is the NOP handler. The report was written and
	// discarded, so the failure stayed exactly as invisible as it had been, and the next
	// session read "RebuildDB returns no error" off an empty log and looked elsewhere for
	// a bug that was announcing itself into /dev/null. errLogger is the fix: a caller's
	// logger when there is one, the default logger when there is not, never the NOP.
	//
	// Measured on the machine this was written on: 152 pages and 152 shards beside a 16 KB
	// database, surviving a full `memory index --reset`, with no error anywhere.
	//
	// It is a warning rather than a returned error because the pages ARE written and the
	// shard export below still has to run: a failed index is a degraded wiki, not a
	// lost memory.
	if err := wiki.RebuildDB(ctx, wikiDir, wikiChunks, nil, logEntry, processCache); err != nil {
		errLogger(logger).Error("memory wiki index build failed; search will fall back "+
			"to scanning the pages", "dir", wikiDir, "error", err)
	}

	// Back out into the raw directory, so the next publish carries what this build
	// computed. Only here, on the full-build path: the cache now covers exactly the
	// memories the prefix holds, which is what makes mirroring — and therefore
	// dropping a deleted memory's shard — safe. The fast paths above changed nothing,
	// so they have nothing to publish.
	if err := ExportShards(rawDir, wikiDir); err != nil {
		slogutil.Resolve(firstLogger(logger)).Warn("memory shard export",
			"raw", rawDir, "wiki", wikiDir, "error", err)
	}

	return result, nil
}

// firstLogger picks the optional logger a caller passed, if any.
func firstLogger(logger []*slog.Logger) *slog.Logger {
	if len(logger) > 0 {
		return logger[0]
	}
	return nil
}

// errLogger resolves the logger for a failure that must not be swallowed.
//
// It exists because `slogutil.Resolve` answers a nil logger with the NOP handler, which is
// the right default for chatter and the wrong one for the report that says the index is not
// there. IndexMemories — the funnel every `memory index` goes through — passes no logger, so
// resolving to NOP means the one caller that matters is precisely the one that reports into
// nothing.
func errLogger(logger []*slog.Logger) *slog.Logger {
	if l := firstLogger(logger); l != nil {
		return l
	}
	return slog.Default()
}

func memoryEntityPageWithHash(id, title, createdAt string, important bool, body, memType, contentHash string) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	docType := memType
	if docType == "" {
		docType = "memory"
	}

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "type: %s\n", wiki.YAMLScalar(docType))
	_, _ = fmt.Fprintf(&b, "title: %s\n", wiki.YAMLScalar(title))
	_, _ = fmt.Fprintf(&b, "id: %s\n", wiki.YAMLScalar(id))
	wiki.WriteOKFGenerated(&b, wiki.OKFActor("memory"), now)
	wiki.WriteOKFSources(&b, fmt.Sprintf("memory/%s.md", id))
	summary := wiki.ExtractSummary(body)
	if summary != "" {
		summaryEscaped := strings.ReplaceAll(summary, "\n", " ")
		_, _ = fmt.Fprintf(&b, "description: %s\n", wiki.YAMLScalar(summaryEscaped))
	}
	if contentHash != "" {
		_, _ = fmt.Fprintf(&b, "content_hash: %s\n", wiki.YAMLScalar(contentHash))
	}
	if createdAt != "" {
		_, _ = fmt.Fprintf(&b, "created: %s\n", wiki.YAMLScalar(createdAt))
	}
	tags := []string{"memory"}
	if important {
		tags = append(tags, "important")
	}
	if memType != "" && memType != "memory" {
		tags = append(tags, memType)
	}
	b.WriteString("tags:\n")
	for _, t := range tags {
		_, _ = fmt.Fprintf(&b, "  - %s\n", wiki.YAMLScalar(t))
	}
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

	// §8: an index file carries no frontmatter; §12 allows okf_version in a bundle-root
	// index.md and nowhere else. A memory wiki is its own bundle, so this is that file.
	//
	// This page was missed entirely by the first OKF pass — it still declared `updated`
	// and an inline tag list, and every catalog entry below was a [[wikilink]].
	_, _ = fmt.Fprintf(&b, "---\nokf_version: \"%s\"\n---\n\n", wiki.OKFVersion)
	b.WriteString("# Memory Wiki\n\n")
	_, _ = fmt.Fprintf(&b, "> %s memory wiki. **Start here.** Scan the catalog below, then follow the links to drill into specific pages.\n", brand.DisplayName)
	_, _ = fmt.Fprintf(&b, "> Check [log](log.md) for the timeline of updates. Last updated: %s\n\n", now)

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
			link := memoryPageLink(d.title)
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
			link := memoryPageLink(d.title)
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
			link := memoryPageLink(d.title)
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
	b.WriteString("3. **Read the Log** — [log](log.md) shows the timeline of wiki updates.\n\n")
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "*Generated by %s · %s · [log](log.md)*\n", brand.DisplayName, now)
	return b.String()
}

// memoryPageLink renders a catalog entry as the standard markdown link OKF specifies for
// links between concepts (§6.1). It replaced [[wikilink]] output, which the first OKF pass
// converted on entity pages and left untouched here.
func memoryPageLink(title string) string {
	slug := wiki.SafeSlug(title)
	label := title
	if strings.TrimSpace(label) == "" {
		label = slug
	}
	return fmt.Sprintf("[%s](%s.md)", label, slug)
}

func appendMemLog(logPath string, totalMemories, articlesWritten int, logger *slog.Logger) {
	log := slogutil.Resolve(logger)
	now := time.Now().UTC()
	entries := []wiki.LogEntry{{
		Kind: wiki.LogUpdate,
		Text: fmt.Sprintf("Wiki regenerated at %s UTC — %d memories, %d article(s) written.",
			now.Format("15:04:05"), totalMemories, articlesWritten),
	}}
	if err := wiki.AppendOKFLogEntries(logPath, "Memory Wiki Update Log", now.Format("2006-01-02"), entries); err != nil {
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
