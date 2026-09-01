package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path"
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
//
// The forked-id rejection is the guard against a name that carries a memory id plus something
// else — `<ulid>_important_.md` being the shape that forked 184 memories in this repository. A
// file like that is not a memory: it is a twin created by a write path that recovered the id from
// the file name. Compiling it produced a second page for one memory, which search then answered
// with twice.
func isMemorySourceFile(name string) bool {
	if filepath.Ext(name) != ".md" {
		return false
	}
	if name == "index.md" || name == "log.md" || strings.HasPrefix(name, "Memory_Wiki") {
		return false
	}
	return !isForkedMemoryFileName(name)
}

// isForkedMemoryFileName reports whether a name is a memory id with something appended.
//
// Deliberately narrow: it rejects `<ulid><anything>.md` and nothing else, so a store whose files
// are named by some other convention still compiles. The general protection against a duplicate
// page is dedupMemoryDocsByID, which works on the declared id rather than on the name.
func isForkedMemoryFileName(name string) bool {
	stem := strings.TrimSuffix(name, ".md")
	if len(stem) <= 26 || IsMemoryID(stem) {
		return false
	}
	return IsMemoryID(stem[:26])
}

// memorySourceFileNames lists the cache keys of everything the wiki compiles: the live memories
// at the top level, and every archived revision under history/.
//
// The archives are included so a change to one — a forward pointer repointed by a later update —
// invalidates the stat pre-check and gets recompiled. Leaving them out made the chain's own
// metadata the one thing a rebuild could not notice.
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
	return append(names, historySourceFileNames(rawDir)...)
}

// historySourceFileNames lists every archived revision, as a path relative to the raw directory.
//
// This is the deliberate walk into history/ that the rest of the package does not do. Every
// listing elsewhere reads one level and skips directories, which is what keeps a superseded
// revision out of the CATALOGUE of what this project knows; compiling it is a different question,
// and the answer to that one is yes.
func historySourceFileNames(rawDir string) []string {
	chains, err := os.ReadDir(filepath.Join(rawDir, HistoryDirName))
	if err != nil {
		return nil
	}
	var names []string
	for _, chain := range chains {
		if !chain.IsDir() {
			continue
		}
		revisions, err := os.ReadDir(filepath.Join(rawDir, HistoryDirName, chain.Name()))
		if err != nil {
			continue
		}
		for _, rev := range revisions {
			if rev.IsDir() || filepath.Ext(rev.Name()) != ".md" {
				continue
			}
			names = append(names, path.Join(HistoryDirName, chain.Name(), rev.Name()))
		}
	}
	sort.Strings(names)
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
		if doc, ok := buildMemDoc(rawDir, e.Name(), processCache, validPaths); ok {
			docs = append(docs, doc)
		}
	}

	// Two files claiming one id is one memory compiled twice, and the page it produces is
	// indistinguishable from a real memory in a search result. Resolve it here rather than
	// downstream, so nothing further in the pipeline ever sees the duplicate.
	docs = dedupMemoryDocsByID(docs, validPaths)

	for _, rel := range historySourceFileNames(rawDir) {
		if doc, ok := buildMemDoc(rawDir, rel, processCache, validPaths); ok {
			docs = append(docs, doc)
		}
	}

	// Prune deleted files from cache.
	if processCache != nil {
		processCache.Prune(validPaths)
	}

	sort.Slice(docs, func(i, j int) bool {
		// Live memories before superseded revisions: the head of a chain is what a reader wants,
		// and the index page is ordered by this.
		if docs[i].superseded != docs[j].superseded {
			return docs[j].superseded
		}
		if docs[i].important != docs[j].important {
			return docs[i].important
		}
		if docs[i].createdAt != docs[j].createdAt {
			return docs[i].createdAt > docs[j].createdAt
		}
		return docs[i].filename < docs[j].filename
	})

	result := &WikiResult{}

	// SAFETY: resolve each slug ONCE. Recomputing it per consumer let the page
	// and the DB chunk disagree whenever two memories share a title.
	slugs := make([]string, len(docs))
	usedSlugs := make(map[string]bool, len(docs))
	for i, doc := range docs {
		slugs[i] = wiki.UniqueSlug(doc.slugBase(), usedSlugs)
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
		page := memoryEntityPage(doc)
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

		chunk := wiki.WikiChunk{
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
			// The chain, as columns. A memory id is the entity: every revision of a memory shares
			// it, which is what lets the index answer "these two hits are one memory".
			EntityID:   doc.id,
			RevisionID: doc.revisionID,
			Superseded: doc.superseded,
		}
		if doc.superseded {
			chunk.CurrentID = doc.id
		}
		wikiChunks = append(wikiChunks, chunk)
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

// Frontmatter keys that carry the revision chain onto a compiled page.
//
// They are read back by the search layer, which is what lets it recognise two hits as one memory
// and name the current revision of an old one. Keeping them as page frontmatter rather than as
// columns in the wiki database is a deliberate trade: the search reads at most top_k small files,
// and the database schema stays shared with the knowledge wiki instead of growing a memory-only
// concept.
const (
	PageFieldMemoryID   = "id"
	PageFieldSuperseded = "superseded"
	PageFieldCurrent    = "current"
	PageFieldRevisionID = "revision_id"
)

func memoryEntityPage(doc memDoc) string {
	id, title, createdAt, important := doc.id, doc.title, doc.createdAt, doc.important
	body, memType, contentHash := doc.body, doc.memType, doc.contentHash

	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	docType := memType
	if docType == "" {
		docType = "memory"
	}

	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "type: %s\n", wiki.YAMLScalar(docType))
	_, _ = fmt.Fprintf(&b, "title: %s\n", wiki.YAMLScalar(title))
	_, _ = fmt.Fprintf(&b, "%s: %s\n", PageFieldMemoryID, wiki.YAMLScalar(id))
	if doc.superseded {
		_, _ = fmt.Fprintf(&b, "%s: true\n", PageFieldSuperseded)
		_, _ = fmt.Fprintf(&b, "%s: %s\n", PageFieldCurrent, wiki.YAMLScalar(id))
		_, _ = fmt.Fprintf(&b, "%s: %s\n", PageFieldRevisionID, wiki.YAMLScalar(doc.revisionID))
		if doc.revision > 0 {
			_, _ = fmt.Fprintf(&b, "revision: %d\n", doc.revision)
		}
		if doc.previous != "" {
			_, _ = fmt.Fprintf(&b, "previous: %s\n", wiki.YAMLScalar(doc.previous))
		}
		if doc.next != "" {
			_, _ = fmt.Fprintf(&b, "next: %s\n", wiki.YAMLScalar(doc.next))
		}
	}
	wiki.WriteOKFGenerated(&b, wiki.OKFActor("memory"), now)
	wiki.WriteOKFSources(&b, path.Join("memory", filepath.ToSlash(doc.filename)))
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
	if doc.superseded {
		tags = append(tags, "superseded")
	}
	b.WriteString("tags:\n")
	for _, t := range tags {
		_, _ = fmt.Fprintf(&b, "  - %s\n", wiki.YAMLScalar(t))
	}
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", title)

	if doc.superseded {
		b.WriteString("> 🕓 **Superseded revision — this is not what the project believes now.**\n")
		if doc.revision > 0 {
			_, _ = fmt.Fprintf(&b, "> This is revision %d of memory `%s`.\n", doc.revision, id)
		} else {
			_, _ = fmt.Fprintf(&b, "> This is an earlier revision of memory `%s`.\n", id)
		}
		if doc.next != "" {
			_, _ = fmt.Fprintf(&b, "> It was replaced by `%s`. ", doc.next)
		} else {
			b.WriteString("> Nothing replaced it: the memory it belonged to was deleted. ")
		}
		_, _ = fmt.Fprintf(&b, "The current revision of this chain is memory `%s`.\n", id)
		if doc.previous != "" {
			_, _ = fmt.Fprintf(&b, "> The revision before this one is `%s`.\n", doc.previous)
		}
		b.WriteString("\n")
	}

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

	// A superseded revision is old by definition, so the staleness nudge would be noise on it —
	// and worse than noise, since the fix it suggests is to update a revision that no longer
	// exists as a memory.
	if createdAt != "" && !doc.superseded {
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
	// The catalogue is what this project knows, so a superseded revision has no entry in it. It
	// stays searchable and readable by slug; listing it here would multiply the index by the
	// revision count and bury the current memories in their own history.
	live := make([]memDoc, 0, len(docs))
	for _, d := range docs {
		if !d.superseded {
			live = append(live, d)
		}
	}
	docs = live

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

	revisionID string
	revision   int
	superseded bool
	previous   string
	next       string
}

// slugBase is the slug a document claims before collision handling.
//
// A superseded revision suffixes its own address, because it usually shares its title with the
// live memory — and letting wiki.UniqueSlug settle that with a `_2` would make the two pages
// indistinguishable from two unrelated memories that happen to share a title. The suffix also
// makes the slug stable across rebuilds, which a positional `_2` is not.
func (d memDoc) slugBase() string {
	base := wiki.SafeSlug(d.title)
	if !d.superseded || d.revisionID == "" {
		return base
	}
	return base + "--r" + d.revisionID
}

// buildMemDoc reads one memory file — live or archived — into a memDoc, using the process cache
// when the content has not changed.
//
// rel is relative to rawDir, and it is also the cache key, which is why an archived revision keys
// on `history/<id>/<rev>.md` rather than on a bare file name.
func buildMemDoc(rawDir, rel string, processCache *wiki.WikiProcessCache, validPaths map[string]bool) (memDoc, bool) {
	absPath := filepath.Join(rawDir, filepath.FromSlash(rel))

	data, err := os.ReadFile(absPath)
	if err != nil {
		return memDoc{}, false
	}

	fm := ParseMemoryFrontmatter(string(data))

	// Supersession is decided by WHERE the file is, not by what it says about itself. An archive
	// written before `revision_id` existed declares nothing, and trusting the frontmatter alone
	// compiled it as though it were a live memory — a second page under the same title as the
	// memory it is the history of, which is exactly the duplicate this work set out to remove.
	// The location cannot be wrong: a file under history/ is a superseded revision.
	superseded := isHistorySource(rel) || fm.IsArchivedRevision()

	// The declared id is authoritative. Deriving it from the file name is what forked memories
	// into twins whose id carried the name's suffix, so the name is only a fallback for a file
	// whose frontmatter has no id at all — and for an archive the fallback is its chain directory,
	// never its own name, which is the revision's address rather than the memory's.
	id := fm.ID
	if id == "" {
		if superseded {
			id = path.Base(path.Dir(filepath.ToSlash(rel)))
		} else {
			id = MemoryIDFromFileName(path.Base(rel))
		}
	}

	revisionID := fm.RevisionID
	if superseded && revisionID == "" {
		revisionID = RevisionIDFromHistoryPath(rel)
	}

	contentHash := wiki.ContentHash(data)
	validPaths[rel] = true

	doc := memDoc{
		id:          id,
		createdAt:   fm.CreatedAt,
		important:   IsImportantContent(string(data)),
		filename:    rel,
		contentHash: contentHash,
		revisionID:  revisionID,
		revision:    fm.Revision,
		superseded:  superseded,
		previous:    fm.Previous,
		next:        fm.Next,
	}

	if processCache != nil && !processCache.HasChanged(rel, contentHash) {
		if cached := processCache.Get(rel, contentHash); len(cached) > 0 {
			cc := cached[0]
			doc.title = cc.Title
			doc.body = cc.Body
			doc.memType = cc.DocType
			doc.contentHash = cc.ContentHash
			return doc, true
		}
	}

	doc.title = fm.Title
	if doc.title == "" {
		doc.title, doc.createdAt = parseMemoryMeta(absPath)
	}
	doc.body = extractBodyAfterFrontmatter(string(data))
	doc.memType = parseMemoryType(string(data))

	if processCache != nil {
		processCache.Store(rel, contentHash, []wiki.CachedChunk{{
			Title:       doc.title,
			Body:        doc.body,
			DocType:     doc.memType,
			ContentHash: contentHash,
		}})
		if info, statErr := os.Stat(absPath); statErr == nil {
			processCache.StoreMtime(rel, info.ModTime().UnixNano(), info.Size())
		}
	}
	return doc, true
}

// dedupMemoryDocsByID keeps one document per memory id.
//
// The winner is the file named after the id it declares, because that is the only name a write
// path produces. Anything else claiming the same id is a twin — the residue of an id recovered
// from a file name — and its page would otherwise sit beside the real one in every search result.
func dedupMemoryDocsByID(docs []memDoc, validPaths map[string]bool) []memDoc {
	best := make(map[string]int, len(docs))
	for i, doc := range docs {
		if doc.id == "" {
			continue
		}
		prev, seen := best[doc.id]
		if !seen {
			best[doc.id] = i
			continue
		}
		if doc.filename == MemoryFileName(doc.id) && docs[prev].filename != MemoryFileName(doc.id) {
			delete(validPaths, docs[prev].filename)
			best[doc.id] = i
			continue
		}
		delete(validPaths, doc.filename)
	}

	kept := make([]memDoc, 0, len(docs))
	for i, doc := range docs {
		if doc.id == "" || best[doc.id] == i {
			kept = append(kept, doc)
		}
	}
	return kept
}

var reMemoryType = regexp.MustCompile(`(?m)^type:\s*(.+)$`)

func parseMemoryType(content string) string {
	if m := reMemoryType.FindStringSubmatch(content); m != nil {
		return strings.TrimSpace(m[1])
	}
	return ""
}
