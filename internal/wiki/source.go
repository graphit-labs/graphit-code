package wiki

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/textslice"
)

// ErrPageNotFound reports a page reference that resolved to nothing. Callers use
// it to decide whether listing the available pages would help — it would for a
// mistyped slug, and it would not for a reference that was refused for escaping
// the wiki directory, where suggesting alternatives only buries the reason.
var ErrPageNotFound = errors.New("wiki page not found")

// PageResult is one wiki page, sliced as requested.
type PageResult struct {
	Page       string            `json:"page"`
	File       string            `json:"file"`
	Title      string            `json:"title,omitempty"`
	Source     string            `json:"source"`
	TotalLines int               `json:"total_lines"`
	StartLine  int               `json:"start_line,omitempty"`
	EndLine    int               `json:"end_line,omitempty"`
	Matches    []textslice.Match `json:"matches,omitempty"`
}

// firstHeading returns the page's first markdown heading, skipping YAML
// frontmatter, so the caller can confirm it opened the page it meant to.
func firstHeading(content string) string {
	lines := strings.Split(content, "\n")
	i := 0
	if len(lines) > 0 && strings.TrimSpace(lines[0]) == "---" {
		for i = 1; i < len(lines); i++ {
			if strings.TrimSpace(lines[i]) == "---" {
				i++
				break
			}
		}
	}
	for ; i < len(lines); i++ {
		line := strings.TrimSpace(lines[i])
		if strings.HasPrefix(line, "#") {
			return strings.TrimSpace(strings.TrimLeft(line, "#"))
		}
	}
	return ""
}

// ---------- reading a page out of the index ----------

// A MOUNTED WIKI HAS NO FILES, which is the whole reason these exist.
//
// A knowledge artifact published to the Hub is read where it lives — the engine queries the
// objects on S3 and nothing is downloaded. So there is no directory to walk and no `.md` to open,
// and the page has to come from the index.
//
// It is the same text. The wiki compiles ONE chunk per document, so `chunks.body` is the page
// body, not a slice of it — which is what makes this a faithful read rather than an approximation.
// If that ever becomes many chunks per page, this returns the first and is wrong; the invariant is
// asserted by TestReadPageFromIndexReturnsTheWholePage.

// ReadPageAt opens the index at wikiDir and reads one page out of it.
//
// This is the only way a page is read. It replaced a file-backed twin that did `os.ReadFile`
// on `<wikiDir>/<slug>.md`, which existed because the pages were the source of truth — they
// are not, and they are not written any more, so there is nothing to open.
//
// The mounted-from-the-Hub case is no longer special. It used to be the one path that had to
// come out of the index, since a published context downloads nothing; now every path does, and
// the only difference between a local wiki and a published one is the URI the store opens.
func ReadPageAt(ctx context.Context, wikiDir, page string, req textslice.Request) (*PageResult, error) {
	if wikiDir == "" {
		return nil, fmt.Errorf("wiki directory not found — the wiki may not have been built yet")
	}
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil, fmt.Errorf("opening the wiki index: %w", err)
	}
	defer func() { _ = db.Close() }()
	return ReadPageFrom(ctx, db, page, req)
}

// ListPagesAt lists the slugs the index at wikiDir holds, so a caller that guessed a slug wrong
// can be told what does exist.
func ListPagesAt(ctx context.Context, wikiDir string) []string {
	if wikiDir == "" {
		return nil
	}
	db, err := OpenWikiDB(ctx, wikiDir)
	if err != nil {
		return nil
	}
	defer func() { _ = db.Close() }()
	return ListPagesFrom(ctx, db)
}

// ReadPageFrom reads a page out of an open index, applying the same slicing as ReadPageAt.
func ReadPageFrom(ctx context.Context, db *WikiDB, page string, req textslice.Request) (*PageResult, error) {
	if db == nil {
		return nil, fmt.Errorf("wiki index not open")
	}
	if strings.TrimSpace(page) == "" {
		return nil, fmt.Errorf("page is required")
	}
	slug := trimPageExt(strings.TrimSpace(page))

	// A page reference is a SLUG — a key in a column — so a separator or a parent reference is a
	// malformed reference rather than a page that happens to be missing.
	//
	// Nothing can escape anything any more: there is no directory to walk, so this is no longer a
	// containment check. It survives because the distinction it draws is still useful — a caller
	// that mistyped a slug is helped by the list of what exists, and a caller that passed a path
	// is helped by being told it passed a path.
	if strings.ContainsAny(slug, `/\`) || slug == ".." || strings.HasPrefix(slug, "..") {
		return nil, fmt.Errorf("page %q is not a slug: pass the slug a search returned, not a path", page)
	}

	chunk, err := db.Chunk(ctx, slug)
	if errors.Is(err, ErrPageNotFound) {
		// Slugs are generated from titles, so the one a human types rarely matches the casing
		// the generator produced. The file-backed reader resolved this with a case-insensitive
		// directory match; a column filter is exact, so the tolerance has to be kept here or it
		// disappears silently. Only on a miss, so the hit path stays one indexed lookup.
		if resolved, ok := resolveSlugCaseInsensitively(ctx, db, slug); ok {
			slug = resolved
			chunk, err = db.Chunk(ctx, slug)
		}
	}
	if err != nil {
		return nil, err
	}

	// A PAGE IS ITS FRONTMATTER AND ITS BODY, and this is where that was quietly lost.
	//
	// When page reads moved off the file and onto the index, this returned the `body` column alone —
	// so everything the header carried became unreachable through the tool that exists to read a
	// page. That is not a cosmetic loss: the memory protocol instructs an agent to walk a revision
	// chain by reading `previous` / `next` off the page, and the instruction silently stopped
	// working. Rebuilding the header from the columns restores what an `os.ReadFile` of the compiled
	// `.md` used to return, and the slicing below applies to the whole page exactly as it did then.
	//
	// No flag, and no second shape of "read a page": a dual-read path is the thing this line of work
	// exists to remove, and the header is a dozen short lines against a body measured in hundreds.
	header, headerErr := RenderPageHeader(*chunk, "")
	if headerErr != nil {
		return nil, headerErr
	}
	// `page` is the caller's reference; this is the page ITSELF.
	fullPage := header + "\n" + chunk.Body

	sliced, err := textslice.Apply(fullPage, req)
	if err != nil {
		return nil, err
	}
	title := chunk.Title
	if title == "" {
		title = firstHeading(chunk.Body)
	}
	return &PageResult{
		Page:       slug,
		File:       slug + ".md",
		Title:      title,
		Source:     sliced.Source,
		TotalLines: sliced.TotalLines,
		StartLine:  sliced.StartLine,
		EndLine:    sliced.EndLine,
		Matches:    sliced.Matches,
	}, nil
}

// resolveSlugCaseInsensitively finds the indexed slug that differs from want only in casing.
func resolveSlugCaseInsensitively(ctx context.Context, db *WikiDB, want string) (string, bool) {
	slugs, err := db.Slugs(ctx)
	if err != nil {
		return "", false
	}
	for _, s := range slugs {
		if strings.EqualFold(s, want) {
			return s, true
		}
	}
	return "", false
}

// ListPagesFrom returns the slugs an open index holds, so a caller that guessed wrong can be told
// what does exist.
func ListPagesFrom(ctx context.Context, db *WikiDB) []string {
	if db == nil {
		return nil
	}
	slugs, err := db.Slugs(ctx)
	if err != nil {
		return nil
	}
	return slugs
}

// trimPageExt drops a trailing `.md` in any casing.
//
// The extension is tolerated because every wiki tool hands slugs back with one, and its casing
// is tolerated for the same reason the slug's is: a human retypes what they saw, not what the
// generator produced.
func trimPageExt(page string) string {
	if len(page) >= 3 && strings.EqualFold(page[len(page)-3:], ".md") {
		return page[:len(page)-3]
	}
	return page
}
