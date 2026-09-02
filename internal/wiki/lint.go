package wiki

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"
)

// LintConfig is what the audit can be told to do differently. Only staleness is a choice: every
// other finding is read off the index, which either has the edge or does not.
//
// There is deliberately no repair option. The one mechanically repairable finding used to be a
// page missing the `## Backlinks` section its inbound references entitled it to; there is no page
// to repair now, and the `xrefs` table cannot be missing an edge the graph knows about, because
// the graph is built FROM it.
type LintConfig struct {
	StaleDays int
}

type LintReport struct {
	TotalPages    int
	Orphans       []string
	BrokenLinks   []BrokenLinkInfo
	StalePages    []string
	EmptyPages    []string
	MissingFields []FieldIssue
	// WeakFields lists pages missing a RECOMMENDED OKF field. It is reported and never
	// counted in Errors: OKF v0.2 §11 forbids rejecting a concept over an optional field,
	// so these are quality hints, not conformance failures.
	WeakFields []FieldIssue
	Errors     int
}

type FieldIssue struct {
	Page          string
	MissingFields []string
}

func (r *LintReport) HasIssues() bool {
	return r.Errors > 0
}

func (r *LintReport) Summary() string {
	if r.Errors == 0 {
		return fmt.Sprintf("✓ %d pages — no issues found", r.TotalPages)
	}
	return fmt.Sprintf("⚠ %d pages — %d issue(s) found", r.TotalPages, r.Errors)
}

// LintWiki audits a compiled wiki.
//
// It reads the INDEX, not a directory of pages. Every check it makes was a text scan over a
// rendered page — frontmatter parsed with regexes, the body's words counted after stripping the
// frontmatter and the backlinks section — and every one of those facts is a column: `doc_type` is
// OKF's one required field, `title` and `summary` are the recommended ones, `word_count` was
// computed at compile time from the same body, and staleness is what the compiler decided rather
// than something re-derived from a date in a file.
func LintWiki(ctx context.Context, wikiDir string, cfg LintConfig) (*LintReport, error) {
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("opening the wiki index: %w", err)
	}
	defer func() { _ = db.Close() }()
	return LintWikiFrom(ctx, db, wikiDir, cfg)
}

// LintWikiFrom audits an already-open local or mounted index.
func LintWikiFrom(ctx context.Context, db *WikiDB, source string, cfg LintConfig) (*LintReport, error) {
	if db == nil {
		return nil, fmt.Errorf("wiki index not open")
	}
	report := &LintReport{}
	chunks, err := db.Chunks(ctx)
	if err != nil {
		return nil, fmt.Errorf("reading the indexed pages: %w", err)
	}
	// An empty index is REFUSED rather than reported as a clean wiki. Opening a store creates it,
	// so a lint pointed at a directory that holds no wiki would otherwise answer
	// "0 pages — no issues found", which is the most misleading answer available here.
	if len(chunks) == 0 {
		return nil, fmt.Errorf("the wiki at %s holds no pages — index it first", source)
	}
	edges, err := db.AllXRefs(ctx)
	if err != nil {
		return nil, fmt.Errorf("reading the cross-references: %w", err)
	}

	pages := make([]PageEdges, 0, len(chunks))
	for _, c := range chunks {
		pages = append(pages, PageEdges{Slug: c.Slug, Title: c.Title, Targets: edges[c.Slug]})
	}
	graph := BuildCrossRefGraphFromRefs(pages)

	report.TotalPages = len(graph.AllPages)

	report.Orphans = OrphanPages(graph)
	report.Errors += len(report.Orphans)

	report.BrokenLinks = BrokenLinks(graph)
	report.Errors += len(report.BrokenLinks)

	for _, c := range chunks {
		if cfg.StaleDays > 0 && chunkIsStale(c, cfg.StaleDays) {
			report.StalePages = append(report.StalePages, c.Slug)
			report.Errors++
		}

		if c.WordCount <= 10 {
			report.EmptyPages = append(report.EmptyPages, c.Slug)
			report.Errors++
		}

		if missing := missingRequiredChunkFields(c); len(missing) > 0 {
			report.MissingFields = append(report.MissingFields, FieldIssue{
				Page:          c.Slug,
				MissingFields: missing,
			})
			report.Errors++
		}

		if weak := missingRecommendedChunkFields(c); len(weak) > 0 {
			report.WeakFields = append(report.WeakFields, FieldIssue{
				Page:          c.Slug,
				MissingFields: weak,
			})
		}
	}

	sort.Strings(report.StalePages)
	sort.Strings(report.EmptyPages)

	return report, nil
}

// missingRequiredChunkFields reports OKF's conformance contract, on columns.
//
// OKF v0.2 §11 requires exactly one thing of a concept document: a non-empty `type`. Everything
// else is RECOMMENDED, and §11 is explicit that a consumer MUST NOT reject a document over an
// optional field.
func missingRequiredChunkFields(c WikiChunk) []string {
	var missing []string
	if strings.TrimSpace(c.DocType) == "" {
		missing = append(missing, "type")
	}
	return missing
}

// missingRecommendedChunkFields reports the quality hints, which are never counted as errors.
func missingRecommendedChunkFields(c WikiChunk) []string {
	var missing []string
	if strings.TrimSpace(c.Title) == "" {
		missing = append(missing, "title")
	}
	if strings.TrimSpace(c.Summary) == "" {
		missing = append(missing, "description")
	}
	return missing
}

// chunkIsStale answers the two questions the frontmatter readers asked, in the order OKF gives
// them: an explicit staleness instant wins, and age is the fallback.
//
// `stale_since` is what the compiler concluded — the source or a dependency changed — so it is a
// decision, not a date to compare against a window, and a page carrying one is stale at any
// staleDays. `updated` is the source's own date, and a page with none is NOT reported: §11 forbids
// rejecting a concept over a missing optional field, and a page whose age is unknown is not a page
// known to be old.
func chunkIsStale(c WikiChunk, staleDays int) bool {
	if strings.TrimSpace(c.StaleSince) != "" {
		return true
	}
	t, ok := parseFMInstant(c.Updated)
	if !ok {
		return false
	}
	return time.Since(t).Hours() > float64(staleDays*24)
}

// parseFMInstant reads the date shapes a page's `updated` column can carry.
//
// It is what is left of a family of frontmatter regexes — `generated`, inline and block,
// `stale_after`, `updated` — that parsed a rendered page to recover facts the compiler already
// knew. The compiler writes them as columns now, so only the date parsing survived.
func parseFMInstant(raw string) (time.Time, bool) {
	s := strings.TrimSpace(raw)
	s = strings.Trim(s, "\"'")
	s = strings.TrimSpace(s)
	for _, layout := range []string{time.RFC3339, "2006-01-02T15:04:05Z", "2006-01-02 15:04:05", "2006-01-02"} {
		if t, err := time.Parse(layout, s); err == nil {
			return t, true
		}
	}
	return time.Time{}, false
}
