package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type WikiResult struct {
	ArticlesWritten int
}

// GenerateMemoryWikiFromTable compiles a scope's wiki from its store instead of from files.
//
// The table supplies the whole corpus in one read. `FastPathCheck` compares it with the compiled
// wiki, and each record carries its embedding in the authoritative store.
func GenerateMemoryWikiFromTable(ctx context.Context, tbl *MemoryTable, wikiDir string, logger ...*slog.Logger) (*WikiResult, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating wiki dir: %w", err)
	}

	records, err := tbl.List(ctx)
	if err != nil {
		return nil, fmt.Errorf("listing the memory store: %w", err)
	}

	docs := make([]memDoc, 0, len(records))
	for _, rec := range records {
		doc := memDocFromRecord(rec)
		docs = append(docs, doc)
	}

	return compileMemoryWiki(ctx, docs, wikiDir, logger...), nil
}

// memDocFromRecord is the store's row in the shape the compiler expects.
//
// `filename` is the logical source label exposed by wiki results and the sort's final tiebreak.
func memDocFromRecord(rec MemoryRecord) memDoc {
	rel := MemoryFileName(rec.ID)
	if rec.Superseded && rec.RevisionID != "" {
		rel = HistoryPath(rec.ID, rec.RevisionID)
	}
	return memDoc{
		id:          rec.ID,
		title:       rec.Title,
		createdAt:   rec.CreatedAt,
		important:   rec.Important,
		mandatory:   rec.Mandatory,
		tags:        append([]string(nil), rec.Tags...),
		body:        rec.Body,
		filename:    rel,
		memType:     rec.Type,
		contentHash: rec.ContentHash,
		revisionID:  rec.RevisionID,
		revision:    rec.Revision,
		superseded:  rec.Superseded,
		previous:    rec.Previous,
		next:        rec.Next,
	}
}

// compileMemoryWiki turns an enumerated set of documents into the compiled index.
//
// It is the SOURCE-AGNOSTIC half of the generator, and factoring it out is what makes moving the
// store a change of enumeration rather than a rewrite: sorting, slug resolution, the incremental
// gate and synchronization all take documents and never ask where they came from.
func compileMemoryWiki(ctx context.Context, docs []memDoc, wikiDir string, logger ...*slog.Logger) *WikiResult {
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

	// FAST PATH: the index's own slug/hash projection detects additions, changes and deletions.
	fastEntries := make([]wiki.DocHashEntry, len(docs))
	for i, doc := range docs {
		fastEntries[i] = wiki.DocHashEntry{
			ContentHash: doc.contentHash,
			Slug:        slugs[i],
		}
	}
	if wiki.FastPathCheck(ctx, wikiDir, fastEntries) {
		return result
	}

	// NO PAGES ARE WRITTEN, and nothing is pruned.
	//
	// There used to be a loop here rendering `<slug>.md` per memory and a pass deleting the pages
	// no memory claimed any more. Both are gone: `SyncDB` below receives these same docs and is
	// the only place a memory becomes readable, so the page was a second copy of it — written,
	// pruned, and then read by a fallback that existed because the two could disagree.
	//
	// ArticlesWritten now counts the documents the synchronization carries rather than files a loop
	// created. It keeps its meaning for every caller that reports it, and it is what decides
	// whether a sync log entry is written below.
	result.ArticlesWritten = len(docs)

	// Build the compiled index — the only artifact.
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
			Created:     doc.createdAt,
			Important:   doc.important,
			Mandatory:   doc.mandatory,
			Tags:        memoryWikiTags(doc),
			ContentHash: doc.contentHash,
			// The chain, as columns. A memory id is the entity: every revision of a memory shares
			// it, which is what lets the index answer "these two hits are one memory".
			//
			// `revision`, `previous` and `next` are here rather than only in the page's frontmatter
			// because the page is not written. They are what makes the chain WALKABLE — the id says
			// which chain a revision belongs to, and only these say where in it — so a compiled
			// memory without them is a revision nobody can step out of.
			EntityID:   doc.id,
			RevisionID: doc.revisionID,
			Superseded: doc.superseded,
			Revision:   doc.revision,
			Previous:   doc.previous,
			Next:       doc.next,
		}
		if doc.superseded {
			chunk.CurrentID = doc.id
		}
		wikiChunks = append(wikiChunks, chunk)
	}

	// Pass a log entry only when the source table changed.
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
	// A failure cannot be swallowed: LanceDB is the only query surface, so an empty or stale store
	// must be visible to the operator. The report uses the caller's logger when supplied and the
	// default logger otherwise.
	//
	// Replacing a discarded error with a log line was only half of it, because the line went to
	// `slogutil.Resolve(nil)` — which is the NOP handler. The report was written and
	// discarded, so the failure stayed exactly as invisible as it had been, and the next
	// session saw no error. errLogger is the fix: a caller's logger when there is one, the default
	// logger when there is not, never the NOP. It is logged rather than returned because the
	// authoritative memory-table read has already succeeded and the wiki can be retried.
	if err := wiki.SyncDB(ctx, wikiDir, wikiChunks, nil, logEntry); err != nil {
		errLogger(logger).Error("memory wiki index build failed, so this scope's memories are "+
			"not searchable", "dir", wikiDir, "error", err)
	}

	return result
}

func memoryWikiTags(doc memDoc) []string {
	tags := []string{"wiki"}
	if doc.memType != "" {
		tags = append(tags, doc.memType)
	}
	tags = append(tags, doc.tags...)
	if doc.superseded {
		tags = append(tags, "superseded")
	}
	if doc.important {
		tags = append(tags, "important")
	}
	if doc.mandatory {
		tags = append(tags, "mandatory")
	}
	return uniqueStrings(tags)
}

func uniqueStrings(values []string) []string {
	seen := make(map[string]bool, len(values))
	out := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value != "" && !seen[value] {
			seen[value] = true
			out = append(out, value)
		}
	}
	return out
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

// The names of the revision-chain facts, as an agent reading a memory sees them.
//
// They used to be the frontmatter keys of a compiled page, and the search layer read them back off
// the file — a deliberate trade at the time: at most top_k small reads, against a wiki schema that
// stayed shared with the knowledge wiki instead of growing a memory-only concept.
//
// Both halves of that reversed. Supersession turned out NOT to be a memory-only concept — a spec
// replaced by a later spec is the same shape — so it became four generic columns, and the file reads
// went with them. What is left here is the vocabulary: these are the names the memory protocol
// documents, and what an exported page's frontmatter is keyed by.
const (
	PageFieldMemoryID   = "id"
	PageFieldSuperseded = "superseded"
	PageFieldCurrent    = "current"
	PageFieldRevisionID = "revision_id"
)

// memoryEntityPage and memoryPageLink are GONE, with the pages they rendered.
//
// Everything the page carried is a column: type, title, id (`entity_id`), `superseded`,
// `current_id`, `revision_id`, `revision`, `previous`, `next`, `created`, `content_hash`, the
// summary, and the body. The four PageField constants above name the frontmatter keys it used, and
// they stay because the memory protocol documents them as the names of these facts.
//
// What the page ALSO carried was presentation — the superseded banner naming the revision before
// and after, the type emoji, the important star, the age nudge. That is a rendering of the same
// columns, and it is rendered by `wiki.ExportMarkdown` for whoever asks for markdown, rather than
// by every build for nobody in particular.

// memoryIndexPage and appendMemLog are GONE.
//
// They rendered `index.md` and appended `log.md` into the wiki directory — the catalogue of live
// memories grouped by type, and one line per regeneration. Neither is written any more: the
// catalogue is a `Browse` query (`wiki.WikiOverview`, `graphit wiki browse`) and the history is the
// `sync_log` table (`graphit wiki log`). `graphit wiki export --wiki memory` renders both as files
// for whoever wants the tree.
//
// What was deliberate in memoryIndexPage and is preserved in the query it became: a SUPERSEDED
// revision has no catalogue entry. It stays searchable and readable by slug, and listing it would
// multiply the index by the revision count and bury the current memories in their own history.

type memDoc struct {
	id          string
	title       string
	createdAt   string
	important   bool
	mandatory   bool
	tags        []string
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

var reMemoryType = regexp.MustCompile(`(?m)^type:\s*(.+)$`)

func parseMemoryType(content string) string {
	if m := reMemoryType.FindStringSubmatch(content); m != nil {
		return strings.TrimSpace(m[1])
	}
	return ""
}
