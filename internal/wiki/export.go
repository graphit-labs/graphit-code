package wiki

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"gopkg.in/yaml.v3"
)

// ExportResult reports what an export produced.
type ExportResult struct {
	OutputDir string `json:"output_dir"`
	Pages     int    `json:"pages"`
	HasLog    bool   `json:"has_log"`
}

type okfPageFrontmatter struct {
	Type  string `yaml:"type"`
	Title string `yaml:"title"`
	// Generated is a POINTER so a read can omit it. §5.2 requires an actor inside it, and the actor
	// names the producing module — which the index does not record. The export knows it from its
	// moduleTag argument; a read genuinely does not, and inventing one would be a claim about
	// provenance rather than a missing field. A read is not a published bundle file, so §11's
	// conformance contract does not apply to it.
	Generated   *okfGenerated  `yaml:"generated,omitempty"`
	Sources     []okfSourceRef `yaml:"sources,omitempty"`
	Description string         `yaml:"description,omitempty"`
	Tags        []string       `yaml:"tags,omitempty"`
	ID          string         `yaml:"id"`
	Confidence  float64        `yaml:"confidence"`
	ContentHash string         `yaml:"content_hash,omitempty"`
	Created     string         `yaml:"created,omitempty"`
	Breadcrumb  string         `yaml:"breadcrumb,omitempty"`
	Cluster     *int           `yaml:"cluster,omitempty"`
	ClusterName string         `yaml:"cluster_name,omitempty"`
	Important   bool           `yaml:"important,omitempty"`
	Mandatory   bool           `yaml:"mandatory,omitempty"`
	StaleAfter  string         `yaml:"stale_after,omitempty"`
	StaleSince  string         `yaml:"stale_since,omitempty"`
	StaleReason string         `yaml:"stale_reason,omitempty"`

	// The revision chain, under the names the memory protocol documents: `id` above is the entity
	// every revision shares, `previous` and `next` are the two directions of the chain, and
	// `superseded` with `current` marks a revision that is not what the project holds now.
	Superseded bool   `yaml:"superseded,omitempty"`
	Current    string `yaml:"current,omitempty"`
	RevisionID string `yaml:"revision_id,omitempty"`
	Revision   int    `yaml:"revision,omitempty"`
	Previous   string `yaml:"previous,omitempty"`
	Next       string `yaml:"next,omitempty"`
}

type okfGenerated struct {
	By string `yaml:"by"`
	At string `yaml:"at"`
}

type okfSourceRef struct {
	Resource string `yaml:"resource"`
}

// ExportMarkdown renders a wiki's index into a directory of markdown pages.
//
// moduleTag names the producer in `generated.by` — "knowledge" or "memory" — because that is the
// one fact about a page that is not in the index: the index does not record which generator
// compiled it, and OKF §5.3 derives a trust tier from the actor.
func ExportMarkdown(ctx context.Context, wikiDir, outDir, moduleTag string) (*ExportResult, error) {
	if strings.TrimSpace(outDir) == "" {
		return nil, fmt.Errorf("an output directory is required")
	}

	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("opening the wiki index: %w", err)
	}
	defer func() { _ = db.Close() }()

	chunks, err := db.Chunks(ctx)
	if err != nil {
		return nil, fmt.Errorf("reading the indexed pages: %w", err)
	}
	if len(chunks) == 0 {
		return nil, fmt.Errorf("the wiki at %s holds no pages — index it first", wikiDir)
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

	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("creating the output directory: %w", err)
	}

	result := &ExportResult{OutputDir: outDir}
	actor := OKFActor(moduleTag)
	for _, c := range chunks {
		page, renderErr := renderPage(c, actor, edges[c.Slug], graph)
		if renderErr != nil {
			return nil, fmt.Errorf("rendering %s: %w", c.Slug, renderErr)
		}
		if err := os.WriteFile(filepath.Join(outDir, c.Slug+".md"), []byte(page), 0o644); err != nil {
			return nil, fmt.Errorf("writing %s: %w", c.Slug, err)
		}
		result.Pages++
	}

	if err := os.WriteFile(filepath.Join(outDir, "index.md"),
		[]byte(renderIndexPage(chunks)), 0o644); err != nil {
		return nil, fmt.Errorf("writing index.md: %w", err)
	}

	entries, err := db.QuerySyncLog(ctx, syncLogKeep)
	if err == nil && len(entries) > 0 {
		if err := renderLogPage(filepath.Join(outDir, "log.md"), moduleTag, entries); err != nil {
			return nil, fmt.Errorf("writing log.md: %w", err)
		}
		result.HasLog = true
	}

	return result, nil
}

func pageFrontmatter(c WikiChunk, actor string) okfPageFrontmatter {
	fm := okfPageFrontmatter{
		Type:        c.DocType,
		Title:       c.Title,
		Description: strings.ReplaceAll(c.Summary, "\n", " "),
		ID:          c.Slug,
		Confidence:  c.Confidence,
		ContentHash: c.ContentHash,
		Created:     c.Created,
		Breadcrumb:  c.Breadcrumb,
		ClusterName: c.ClusterName,
		Important:   c.Important,
		Mandatory:   c.Mandatory,
		RevisionID:  c.RevisionID,
		Revision:    c.Revision,
		Previous:    c.Previous,
		Next:        c.Next,
	}
	if c.EntityID != "" {
		fm.ID = c.EntityID
	}
	if c.Superseded {
		fm.Superseded = true
		fm.Current = c.CurrentID
		if fm.Current == "" {
			fm.Current = c.EntityID
		}
	}
	if fm.Type == "" {
		fm.Type = "document"
	}
	if actor != "" {
		at := c.Updated
		if at == "" {
			at = time.Now().UTC().Format("2006-01-02")
		}
		fm.Generated = &okfGenerated{By: actor, At: at}
	}
	if c.Source != "" {
		fm.Sources = []okfSourceRef{{Resource: c.Source}}
	}
	fm.Tags = append([]string(nil), c.Tags...)
	if c.ClusterID >= 0 {
		cluster := c.ClusterID
		fm.Cluster = &cluster
	}
	if c.StaleSince != "" {
		fm.StaleAfter = c.StaleSince
		fm.StaleSince = c.StaleSince
		fm.StaleReason = c.StaleReason
	}
	return fm
}

func RenderPageHeader(c WikiChunk, actor string) (string, error) {
	block, err := yaml.Marshal(pageFrontmatter(c, actor))
	if err != nil {
		return "", fmt.Errorf("rendering the frontmatter of %s: %w", c.Slug, err)
	}
	return "---\n" + string(block) + "---\n", nil
}

func renderPage(c WikiChunk, actor string, outbound []string, graph *CrossRefGraph) (string, error) {
	fm := pageFrontmatter(c, actor)
	block, err := yaml.Marshal(fm)
	if err != nil {
		return "", err
	}

	var b strings.Builder
	b.WriteString("---\n")
	b.Write(block)
	b.WriteString("---\n\n")
	title := c.Title
	if title == "" {
		title = c.Slug
	}
	_, _ = fmt.Fprintf(&b, "# %s\n\n", title)
	if c.Superseded {
		b.WriteString("> 🕓 **Superseded revision — this is not what the project believes now.**\n")
		if c.Revision > 0 {
			_, _ = fmt.Fprintf(&b, "> This is revision %d of `%s`.\n", c.Revision, fm.ID)
		} else {
			_, _ = fmt.Fprintf(&b, "> This is an earlier revision of `%s`.\n", fm.ID)
		}
		if c.Next != "" {
			_, _ = fmt.Fprintf(&b, "> It was replaced by `%s`.\n", c.Next)
		} else {
			b.WriteString("> Nothing replaced it: what it belonged to was deleted.\n")
		}
		if c.Previous != "" {
			_, _ = fmt.Fprintf(&b, "> The revision before this one is `%s`.\n", c.Previous)
		}
		_, _ = fmt.Fprintf(&b, "> The current revision of this chain is `%s`.\n\n", fm.Current)
	}
	if c.StaleSince != "" {
		_, _ = fmt.Fprintf(&b, "> ⚠️ **Stale since %s** — %s. Content may be outdated.\n\n",
			c.StaleSince, c.StaleReason)
	}
	if c.Breadcrumb != "" {
		_, _ = fmt.Fprintf(&b, "*📍 %s*\n\n", c.Breadcrumb)
	}
	if c.Summary != "" {
		_, _ = fmt.Fprintf(&b, "> %s\n\n", strings.ReplaceAll(c.Summary, "\n", " "))
	}
	if c.Source != "" {
		_, _ = fmt.Fprintf(&b, "*Provenance: [%s](%s)*\n\n", c.Source, c.Source)
	}
	if len(outbound) > 0 {
		b.WriteString("## Cross-References\n\n")
		for _, target := range outbound {
			_, _ = fmt.Fprintf(&b, "- [%s](%s.md)\n", target, target)
		}
		b.WriteString("\n")
	}
	if body := strings.TrimSpace(StripFrontmatter(c.Body)); body != "" {
		b.WriteString("## Content\n\n")
		b.WriteString(body + "\n")
	}

	page := b.String()
	if inbound := graph.Inbound[c.Slug]; len(inbound) > 0 {
		page = injectBacklinksSection(page, inbound, graph.Titles)
	}
	return strings.TrimRight(page, "\n") + "\n", nil
}

func renderIndexPage(chunks []WikiChunk) string {
	var b strings.Builder
	now := time.Now().UTC().Format("2006-01-02")
	_, _ = fmt.Fprintf(&b, "---\nokf_version: \"%s\"\n---\n\n", OKFVersion)
	b.WriteString("# Wiki Index\n\n")
	_, _ = fmt.Fprintf(&b, "> %s wiki, exported from its index on %s. **Start here.**\n\n",
		brand.DisplayName, now)

	stale := 0
	for _, c := range chunks {
		if c.StaleSince != "" {
			stale++
		}
	}
	_, _ = fmt.Fprintf(&b, "**%d pages**", len(chunks))
	if stale > 0 {
		_, _ = fmt.Fprintf(&b, " · ⚠️ **%d stale**", stale)
	}
	b.WriteString("\n\n---\n\n")

	byType := make(map[string][]WikiChunk)
	for _, c := range chunks {
		t := c.DocType
		if t == "" {
			t = "document"
		}
		byType[t] = append(byType[t], c)
	}
	types := make([]string, 0, len(byType))
	for t := range byType {
		types = append(types, t)
	}
	sort.Strings(types)

	for _, t := range types {
		_, _ = fmt.Fprintf(&b, "## %s\n\n", t)
		for _, c := range byType[t] {
			title := c.Title
			if title == "" {
				title = c.Slug
			}
			_, _ = fmt.Fprintf(&b, "- [%s](%s.md)", title, c.Slug)
			if c.Summary != "" {
				summary := strings.ReplaceAll(c.Summary, "\n", " ")
				if len(summary) > 80 {
					summary = summary[:80] + "…"
				}
				_, _ = fmt.Fprintf(&b, " — %s", summary)
			}
			if c.StaleSince != "" {
				b.WriteString(" ⚠️")
			}
			b.WriteString("\n")
		}
		b.WriteString("\n")
	}
	b.WriteString("---\n")
	_, _ = fmt.Fprintf(&b, "*Exported by %s · %s*\n", brand.DisplayName, now)
	return b.String()
}

func renderLogPage(logPath, moduleTag string, entries []SyncLogEntry) error {
	_ = os.Remove(logPath)
	header := strings.ToUpper(moduleTag[:1]) + moduleTag[1:] + " Wiki Update Log"

	for i := len(entries) - 1; i >= 0; i-- {
		e := entries[i]
		date := e.Timestamp
		if t, ok := parseFMInstant(e.Timestamp); ok {
			date = t.UTC().Format("2006-01-02")
		}
		label := func(slug string) string {
			line := fmt.Sprintf("[%s](%s.md)", slug, slug)
			if d, ok := e.Details[slug]; ok {
				if d.Title != "" {
					line = fmt.Sprintf("[%s](%s.md)", d.Title, slug)
				}
				if d.Summary != "" {
					summary := d.Summary
					if len(summary) > 120 {
						summary = summary[:120] + "…"
					}
					line += " — " + summary
				}
			}
			return line
		}

		logEntries := []LogEntry{{
			Kind: LogUpdate,
			Text: fmt.Sprintf("Compiled %d change(s) — %d document(s), %d page(s) written, %d backlinked.",
				len(e.Added)+len(e.Updated)+len(e.Deleted), e.TotalDocs, e.ArticlesWritten, e.BacklinksAdded),
		}}
		for _, slug := range e.Added {
			logEntries = append(logEntries, LogEntry{Kind: LogCreation, Text: "Added " + label(slug)})
		}
		for _, slug := range e.Updated {
			logEntries = append(logEntries, LogEntry{Kind: LogUpdate, Text: "Updated " + label(slug)})
		}
		for _, slug := range e.Deleted {
			logEntries = append(logEntries, LogEntry{Kind: LogDeprecation, Text: fmt.Sprintf("Removed `%s`.", slug)})
		}
		if err := AppendOKFLogEntries(logPath, header, date, logEntries); err != nil {
			return err
		}
	}
	return nil
}
